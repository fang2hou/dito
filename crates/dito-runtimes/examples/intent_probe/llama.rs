//! A thin wrapper over the llama.cpp C API (the tree vendored by `llama-cpp-sys-2`, the one Dito links).
//!
//! It exposes what a decision probe needs and the safe binding hides: the logits of the last position, the residual
//! stream entering any block, and a second sequence that can be made a copy of the resident prefix.

use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr;

use llama_cpp_sys_2 as sys;

// Declared in llama.cpp `src/llama-ext.h`, which is C++: the symbols carry Itanium-mangled names. They are not part
// of the public `llama.h` the bindings are generated from, so they are declared here by hand.
unsafe extern "C" {
    #[link_name = "_Z30llama_set_embeddings_layer_inpP13llama_contextjb"]
    fn set_embeddings_layer_inp(ctx: *mut sys::llama_context, lid: u32, value: bool);
    #[link_name = "_Z30llama_get_embeddings_layer_inpP13llama_contextj"]
    fn get_embeddings_layer_inp(ctx: *mut sys::llama_context, lid: u32) -> *mut f32;
}

/// The same prefill as `dito-runtimes` `llama.rs`: Qwen hybrid-thinking checkpoints answer directly after an empty
/// think block.
const EMPTY_THINK_BLOCK: &str = "<think>\n\n</think>\n\n";
const CONTEXT_TOKENS: u32 = 4096;
/// Also the micro-batch size, so one decode call is always one micro-batch and the kept layer rows are in token order.
const BATCH_TOKENS: u32 = 1024;

unsafe extern "C" fn quiet(_: sys::ggml_log_level, _: *const c_char, _: *mut c_void) {}

pub struct Llama {
    model: *mut sys::llama_model,
    vocab: *const sys::llama_vocab,
    ctx: *mut sys::llama_context,
    template: CString,
    pub n_embd: usize,
    pub n_layer: u32,
    pub n_vocab: usize,
    batch: sys::llama_batch,
    /// Position of the first token of the batch decoded last, and its length.
    last_batch: (usize, usize),
}

impl Llama {
    /// Load the model with every layer offloaded. llama.cpp logs stay off unless `DITO_PROBE_VERBOSE` is set.
    pub fn load(path: &str) -> Self {
        // SAFETY: plain C API calls with valid, NUL-terminated arguments. The model, context and chat template
        // pointers are checked before use, and the vocabulary is owned by the loaded model.
        unsafe {
            if std::env::var_os("DITO_PROBE_VERBOSE").is_none() {
                sys::llama_log_set(Some(quiet), ptr::null_mut());
            }
            sys::llama_backend_init();
            let mut model_params = sys::llama_model_default_params();
            model_params.n_gpu_layers = 999;
            let c_path = CString::new(path).expect("model path without NUL");
            let model = sys::llama_model_load_from_file(c_path.as_ptr(), model_params);
            assert!(!model.is_null(), "model failed to load: {path}");
            let threads = std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(4);
            let mut params = sys::llama_context_default_params();
            params.n_ctx = CONTEXT_TOKENS;
            params.n_batch = BATCH_TOKENS;
            params.n_ubatch = BATCH_TOKENS;
            // Sequence 0 holds the resident prefix, sequence 1 the request being decoded.
            params.n_seq_max = 2;
            params.n_threads = threads;
            params.n_threads_batch = threads;
            let ctx = sys::llama_init_from_model(model, params);
            assert!(!ctx.is_null(), "context creation failed");
            let template = sys::llama_model_chat_template(model, ptr::null());
            assert!(!template.is_null(), "the GGUF has no chat template");
            let vocab = sys::llama_model_get_vocab(model);
            Self {
                model,
                vocab,
                ctx,
                template: CStr::from_ptr(template).to_owned(),
                n_embd: sys::llama_model_n_embd(model) as usize,
                n_layer: sys::llama_model_n_layer(model) as u32,
                n_vocab: sys::llama_vocab_n_tokens(vocab) as usize,
                batch: sys::llama_batch_init(BATCH_TOKENS as i32, 0, 1),
                last_batch: (0, 0),
            }
        }
    }

    /// One GGUF metadata value as text, `None` when the key is absent. Values longer than 511 bytes are cut.
    pub fn meta(&self, key: &str) -> Option<String> {
        let key = CString::new(key).ok()?;
        let mut buf = vec![0u8; 512];
        // SAFETY: the key is NUL-terminated and the buffer is writable for its whole length.
        let n = unsafe { sys::llama_model_meta_val_str(self.model, key.as_ptr(), buf.as_mut_ptr().cast(), buf.len()) };
        (n >= 0).then(|| String::from_utf8_lossy(&buf[..(n as usize).min(buf.len() - 1)]).into_owned())
    }

    /// Every architecture-specific metadata pair of the GGUF (keys that start with `<architecture>.`), values cut to
    /// 255 bytes.
    pub fn architecture_meta(&self) -> Vec<(String, String)> {
        let prefix = format!("{}.", self.meta("general.architecture").unwrap_or_default());
        let mut out = Vec::new();
        let text =
            |b: &[u8]| String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).into_owned();
        // SAFETY: indices are below the reported count and the buffers are writable for their length.
        unsafe {
            for i in 0..sys::llama_model_meta_count(self.model) {
                let (mut key, mut value) = (vec![0u8; 256], vec![0u8; 256]);
                sys::llama_model_meta_key_by_index(self.model, i, key.as_mut_ptr().cast(), key.len());
                sys::llama_model_meta_val_str_by_index(self.model, i, value.as_mut_ptr().cast(), value.len());
                let key = text(&key);
                if key.starts_with(&prefix) {
                    out.push((key, text(&value)));
                }
            }
        }
        out
    }

    /// System and user turns in the GGUF's chat template, the assistant turn opened, the empty think block, then
    /// `assistant_prefix`.
    pub fn render(&self, system: &str, user: &str, assistant_prefix: &str) -> String {
        let roles = [c"system", c"user"];
        let contents = [
            CString::new(system).expect("system prompt without NUL"),
            CString::new(user).expect("user text without NUL"),
        ];
        let messages: Vec<sys::llama_chat_message> = roles
            .iter()
            .zip(&contents)
            .map(|(role, content)| sys::llama_chat_message { role: role.as_ptr(), content: content.as_ptr() })
            .collect();
        let mut buf = vec![0u8; (system.len() + user.len()) * 2 + 1024];
        // SAFETY: the messages point at live C strings and the buffer is writable for its length.
        let n = unsafe {
            sys::llama_chat_apply_template(
                self.template.as_ptr(),
                messages.as_ptr(),
                messages.len(),
                true,
                buf.as_mut_ptr().cast(),
                buf.len() as i32,
            )
        };
        assert!(n >= 0, "the chat template is not one llama.cpp can apply");
        assert!(n as usize <= buf.len(), "the rendered prompt does not fit the {} byte buffer", buf.len());
        buf.truncate(n as usize);
        let mut prompt = String::from_utf8(buf).expect("the rendered prompt is UTF-8");
        prompt.push_str(EMPTY_THINK_BLOCK);
        prompt.push_str(assistant_prefix);
        prompt
    }

    /// With `special`, a BOS token is added where the vocabulary has one and special tokens in the text are parsed as
    /// tokens; without it the text is tokenized literally.
    pub fn tokenize(&self, text: &str, special: bool) -> Vec<i32> {
        let mut tokens = vec![0i32; text.len() + 16];
        // SAFETY: the text pointer and length describe a live str; the token buffer is writable for its length.
        let n = unsafe {
            sys::llama_tokenize(
                self.vocab,
                text.as_ptr().cast(),
                text.len() as i32,
                tokens.as_mut_ptr(),
                tokens.len() as i32,
                special,
                special,
            )
        };
        assert!(n >= 0, "tokenize failed");
        tokens.truncate(n as usize);
        tokens
    }

    /// The bytes one token stands for. A piece can be a partial UTF-8 sequence, so these are bytes, not text.
    pub fn piece(&self, token: i32) -> Vec<u8> {
        let mut buf = [0u8; 64];
        // SAFETY: the buffer is writable for its length.
        let n =
            unsafe { sys::llama_token_to_piece(self.vocab, token, buf.as_mut_ptr().cast(), buf.len() as i32, 0, true) };
        buf[..n.max(0) as usize].to_vec()
    }

    pub fn is_end(&self, token: i32) -> bool {
        // SAFETY: the vocabulary pointer lives as long as the model.
        unsafe { sys::llama_vocab_is_eog(self.vocab, token) }
    }

    /// Ask the context to keep the input of these layers for every token of a batch. Layer `l` is the residual
    /// stream entering block `l`; the last one, `n_layer - 1`, is what the final block reads.
    pub fn keep_layers(&mut self, layers: &[u32]) {
        for &layer in layers {
            assert!(layer < self.n_layer, "layer {layer} out of range");
            // SAFETY: the context is live and the layer index is in range.
            unsafe { set_embeddings_layer_inp(self.ctx, layer, true) };
        }
    }

    /// Empty every sequence of the memory.
    pub fn clear(&mut self) {
        // SAFETY: the context is live.
        unsafe { sys::llama_memory_clear(sys::llama_get_memory(self.ctx), true) };
    }

    /// Make sequence 1 a copy of sequence 0 (the resident prefix), dropping whatever it held. The copy is what lets
    /// a model whose memory cannot drop the tail of a sequence (the recurrent state of Qwen3.5) reuse a prefix.
    pub fn fork(&mut self) {
        // SAFETY: the context is live; removing a whole sequence always succeeds.
        unsafe {
            let memory = sys::llama_get_memory(self.ctx);
            sys::llama_memory_seq_rm(memory, 1, -1, -1);
            sys::llama_memory_seq_cp(memory, 0, 1, -1, -1);
        }
    }

    /// Decode a prompt longer than one batch in batches of at most `BATCH_TOKENS`, in order. Only the last batch's
    /// kept layer rows remain readable, so this is for prefixes and reference passes, not for hidden-state capture.
    pub fn decode_chunked(&mut self, tokens: &[i32], start: usize, seq: i32) {
        for (i, chunk) in tokens.chunks(BATCH_TOKENS as usize).enumerate() {
            self.decode(chunk, start + i * BATCH_TOKENS as usize, seq);
        }
    }

    /// Decode `tokens` at positions `start..` of sequence `seq`, with an output only at the last one.
    pub fn decode(&mut self, tokens: &[i32], start: usize, seq: i32) {
        assert!(!tokens.is_empty() && tokens.len() <= BATCH_TOKENS as usize, "batch of {} tokens", tokens.len());
        // SAFETY: the batch was allocated by `llama_batch_init(BATCH_TOKENS, 0, 1)`: BATCH_TOKENS entries in the
        // token, pos, n_seq_id and logits arrays and room for one sequence id per token, which the assert above
        // keeps every index below.
        unsafe {
            for (i, &token) in tokens.iter().enumerate() {
                *self.batch.token.add(i) = token;
                *self.batch.pos.add(i) = (start + i) as i32;
                *self.batch.n_seq_id.add(i) = 1;
                *(*self.batch.seq_id.add(i)) = seq;
                *self.batch.logits.add(i) = i8::from(i + 1 == tokens.len());
            }
            self.batch.n_tokens = tokens.len() as i32;
            assert_eq!(sys::llama_decode(self.ctx, self.batch), 0, "decode failed");
        }
        self.last_batch = (start, tokens.len());
    }

    /// Logits of the last decoded position. Only valid after a `decode`.
    pub fn logits(&self) -> &[f32] {
        // SAFETY: after a successful decode with one output the pointer covers `n_vocab` floats.
        unsafe { std::slice::from_raw_parts(sys::llama_get_logits_ith(self.ctx, -1), self.n_vocab) }
    }

    /// The kept input of `layer` for absolute position `pos` of the batch decoded last. `layer` must have been
    /// passed to `keep_layers` before that decode.
    pub fn layer_row(&self, layer: u32, pos: usize) -> &[f32] {
        let (start, len) = self.last_batch;
        assert!(pos >= start && pos < start + len, "position {pos} is outside the last batch");
        // SAFETY: llama.cpp keeps `n_embd` floats per batch slot for a kept layer, and `pos - start` is a slot of the
        // last batch (at most BATCH_TOKENS of them, the context's batch size).
        unsafe {
            let base = get_embeddings_layer_inp(self.ctx, layer);
            std::slice::from_raw_parts(base.add((pos - start) * self.n_embd), self.n_embd)
        }
    }

    /// Absolute position of the first token of the batch decoded last.
    pub fn batch_start(&self) -> usize {
        self.last_batch.0
    }
}

impl Drop for Llama {
    fn drop(&mut self) {
        // SAFETY: each pointer was created in `load` and is freed once.
        unsafe {
            sys::llama_batch_free(self.batch);
            sys::llama_free(self.ctx);
            sys::llama_model_free(self.model);
        }
    }
}
