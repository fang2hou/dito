//! Reading a decision from a chat model in one prompt pass, with no token sampled.
//!
//! The safe `llama-cpp-2` API cannot do this: it exposes neither the input of a chosen layer nor its raw handles.
//! This module owns a second llama.cpp context on the already loaded model and drives it through the C API.
//!
//! The prompt prefix every request shares (the router's system turn) stays decoded in sequence 0. Each request copies
//! it into sequence 1 and decodes only its own remainder. Qwen3.5 (`qwen35`) interleaves full attention blocks with
//! gated delta net blocks that carry recurrent state, so its memory cannot drop the tail of a sequence; a copy is the
//! only reuse that works. The bench checked the copy against a full decode on all 300 test cases of the 2B (largest
//! logit difference 0.02, same top option everywhere).

use std::ffi::c_void;
use std::ptr::NonNull;

use dito_core::intent::{Part, Pool, Reading};
use dito_local::RuntimeError;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_sys_2 as sys;

// Declared in llama.cpp `src/llama-ext.h`, a C++ header the bindings skip: the symbols carry Itanium-mangled names.
// They belong to the llama.cpp tree vendored by llama-cpp-sys-2 0.1.160, which `Cargo.toml` pins.
unsafe extern "C" {
    #[link_name = "_Z30llama_set_embeddings_layer_inpP13llama_contextjb"]
    fn set_embeddings_layer_inp(ctx: *mut sys::llama_context, layer: u32, value: bool);
    #[link_name = "_Z30llama_get_embeddings_layer_inpP13llama_contextj"]
    fn get_embeddings_layer_inp(ctx: *mut sys::llama_context, layer: u32) -> *mut f32;
}

/// Room for both sequences: the resident prefix and one request's copy of it plus the instruction.
const CONTEXT_TOKENS: u32 = 4096;
/// The most tokens one decode call takes. A request's remainder must fit, because hidden states are kept for the
/// last batch only.
const BATCH_TOKENS: usize = 1024;
const RESIDENT_SEQUENCE: i32 = 0;
const REQUEST_SEQUENCE: i32 = 1;

const _: () = assert!(size_of::<LlamaModel>() == size_of::<*mut c_void>());

fn raw_model(model: &LlamaModel) -> *mut sys::llama_model {
    // SAFETY: in llama-cpp-2 0.1.160 `LlamaModel` is `#[repr(transparent)]` over `Ptr<llama_model>`, which is
    // `#[repr(transparent)]` over `NonNull<llama_model>`, so it has exactly the layout of that pointer. The version is
    // pinned in `Cargo.toml` and the size is asserted above.
    unsafe { std::mem::transmute_copy::<LlamaModel, *mut sys::llama_model>(model) }
}

/// One prompt, tokenized by the caller.
pub(crate) struct Pass<'a> {
    pub tokens: &'a [i32],
    pub option_tokens: &'a [i32],
    /// Token positions `[start, end)` of the instruction, when a part pools over it.
    pub span: Option<(usize, usize)>,
    pub parts: &'a [Part],
}

pub(crate) struct Reader {
    ctx: NonNull<sys::llama_context>,
    batch: sys::llama_batch,
    n_embd: usize,
    n_vocab: usize,
    n_layer: u32,
    /// The tokens decoded in the resident sequence.
    resident: Vec<i32>,
    /// Layers whose input the context keeps for every token of a batch.
    kept: Vec<u32>,
}

// SAFETY: a llama.cpp context is not bound to the thread that created it. `Chat` keeps the reader behind a mutex, so
// one thread uses it at a time.
unsafe impl Send for Reader {}

impl Reader {
    /// The model must outlive the reader; `Chat` declares its reader before its model so it is dropped first.
    pub(crate) fn new(model: &LlamaModel) -> Result<Self, RuntimeError> {
        let threads = std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(4);
        let raw = raw_model(model);
        // SAFETY: `raw` is the live model handle; the parameters are the library's defaults with plain fields set.
        let ctx = unsafe {
            let mut params = sys::llama_context_default_params();
            params.n_ctx = CONTEXT_TOKENS;
            params.n_batch = BATCH_TOKENS as u32;
            params.n_ubatch = BATCH_TOKENS as u32;
            params.n_seq_max = 2;
            params.n_threads = threads;
            params.n_threads_batch = threads;
            sys::llama_init_from_model(raw, params)
        };
        let ctx = NonNull::new(ctx).ok_or_else(|| RuntimeError::engine("the decision context could not be created"))?;
        Ok(Self {
            ctx,
            // SAFETY: allocates token, position, sequence and output arrays for `BATCH_TOKENS` entries.
            batch: unsafe { sys::llama_batch_init(BATCH_TOKENS as i32, 0, 1) },
            n_embd: model.n_embd() as usize,
            n_vocab: model.n_vocab() as usize,
            n_layer: model.n_layer(),
            resident: Vec::new(),
            kept: Vec::new(),
        })
    }

    /// Decode what `pass` adds to the resident prefix and read its option logits and hidden states. `anchor` gives
    /// the tokens of the same prompt with an empty user turn; it is only called when the prefix has to be rebuilt.
    pub(crate) fn read(
        &mut self,
        pass: &Pass<'_>,
        anchor: impl FnOnce() -> Result<Vec<i32>, RuntimeError>,
    ) -> Result<Reading, RuntimeError> {
        let tokens = pass.tokens;
        if tokens.is_empty() {
            return Err(RuntimeError::engine("empty prompt"));
        }
        self.keep(pass.parts)?;
        if self.resident.is_empty() || tokens.len() <= self.resident.len() || !tokens.starts_with(&self.resident) {
            let anchor = anchor()?;
            let shared = tokens.iter().zip(&anchor).take_while(|(a, b)| a == b).count();
            self.set_resident(&tokens[..shared.min(tokens.len() - 1)])?;
        }
        let start = self.resident.len();
        let rest = &tokens[start..];
        if rest.len() > BATCH_TOKENS {
            return Err(RuntimeError::engine(format!("an instruction of {} tokens is too long to route", rest.len())));
        }
        if let Some((from, to)) = pass.span
            && (from < start || to > tokens.len() || from >= to)
        {
            return Err(RuntimeError::engine("the instruction is not in the part of the prompt decoded per request"));
        }

        // SAFETY: the context is live. Removing a whole sequence always succeeds, and the copy takes every position.
        unsafe {
            let memory = sys::llama_get_memory(self.ctx.as_ptr());
            sys::llama_memory_seq_rm(memory, REQUEST_SEQUENCE, -1, -1);
            sys::llama_memory_seq_cp(memory, RESIDENT_SEQUENCE, REQUEST_SEQUENCE, -1, -1);
        }
        self.decode(rest, start, REQUEST_SEQUENCE)?;

        // SAFETY: after a successful decode with an output at the last token, the pointer covers `n_vocab` floats.
        let logits = unsafe {
            let logits = sys::llama_get_logits_ith(self.ctx.as_ptr(), -1);
            if logits.is_null() {
                return Err(RuntimeError::engine("the model returned no logits"));
            }
            std::slice::from_raw_parts(logits, self.n_vocab)
        };
        let option_logits = pass
            .option_tokens
            .iter()
            .map(|&token| logits.get(token as usize).copied().ok_or_else(|| RuntimeError::engine("option token")))
            .collect::<Result<Vec<f32>, _>>()?;

        let last = tokens.len() - 1;
        let mut features = Vec::with_capacity(pass.parts.len());
        for part in pass.parts {
            let rows = self.layer_rows(part.layer, rest.len())?;
            let row = |position: usize| &rows[(position - start) * self.n_embd..][..self.n_embd];
            features.push(match (part.pool, pass.span) {
                (Pool::Last, _) => row(last).to_vec(),
                (Pool::Span, Some((from, to))) => {
                    let mut mean = vec![0f32; self.n_embd];
                    for position in from..to {
                        mean.iter_mut().zip(row(position)).for_each(|(m, x)| *m += x);
                    }
                    mean.iter_mut().for_each(|m| *m /= (to - from) as f32);
                    mean
                }
                (Pool::Span, None) => return Err(RuntimeError::engine("a part pools over a span the pass lacks")),
            });
        }
        Ok(Reading { option_logits, features })
    }

    /// Ask the context to keep the input of these layers. Layer `l` is the residual stream entering block `l`.
    fn keep(&mut self, parts: &[Part]) -> Result<(), RuntimeError> {
        for part in parts {
            if self.kept.contains(&part.layer) {
                continue;
            }
            if part.layer >= self.n_layer {
                return Err(RuntimeError::engine(format!("layer {} of a {}-layer model", part.layer, self.n_layer)));
            }
            // SAFETY: the context is live and the layer index is in range.
            unsafe { set_embeddings_layer_inp(self.ctx.as_ptr(), part.layer, true) };
            self.kept.push(part.layer);
        }
        Ok(())
    }

    fn set_resident(&mut self, prefix: &[i32]) -> Result<(), RuntimeError> {
        self.resident.clear();
        // SAFETY: the context is live.
        unsafe { sys::llama_memory_clear(sys::llama_get_memory(self.ctx.as_ptr()), true) };
        let mut position = 0;
        for chunk in prefix.chunks(BATCH_TOKENS) {
            self.decode(chunk, position, RESIDENT_SEQUENCE)?;
            position += chunk.len();
        }
        self.resident.extend_from_slice(prefix);
        Ok(())
    }

    /// Decode `tokens` at positions `start..` of `sequence`, with an output only at the last one.
    fn decode(&mut self, tokens: &[i32], start: usize, sequence: i32) -> Result<(), RuntimeError> {
        debug_assert!(!tokens.is_empty() && tokens.len() <= BATCH_TOKENS);
        // SAFETY: the batch was allocated for `BATCH_TOKENS` entries with one sequence id each, and `tokens` is no
        // longer than that.
        let status = unsafe {
            for (i, &token) in tokens.iter().enumerate() {
                *self.batch.token.add(i) = token;
                *self.batch.pos.add(i) = (start + i) as i32;
                *self.batch.n_seq_id.add(i) = 1;
                *(*self.batch.seq_id.add(i)) = sequence;
                *self.batch.logits.add(i) = i8::from(i + 1 == tokens.len());
            }
            self.batch.n_tokens = tokens.len() as i32;
            sys::llama_decode(self.ctx.as_ptr(), self.batch)
        };
        if status == 0 { Ok(()) } else { Err(RuntimeError::engine(format!("llama_decode returned {status}"))) }
    }

    /// The kept input of `layer` for the `count` tokens of the batch decoded last, one row of `n_embd` per token.
    fn layer_rows(&self, layer: u32, count: usize) -> Result<&[f32], RuntimeError> {
        // SAFETY: the layer was registered by `keep` before the decode, so the context holds one row per token of
        // that batch.
        unsafe {
            let rows = get_embeddings_layer_inp(self.ctx.as_ptr(), layer);
            if rows.is_null() {
                return Err(RuntimeError::engine(format!("the context kept no input of layer {layer}")));
            }
            Ok(std::slice::from_raw_parts(rows, count * self.n_embd))
        }
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        // SAFETY: both were created in `new` and are freed once.
        unsafe {
            sys::llama_batch_free(self.batch);
            sys::llama_free(self.ctx.as_ptr());
        }
    }
}
