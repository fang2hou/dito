//! Decision probe: one prompt pass per request, no sampling.
//!
//!   intent_probe <model.gguf> <requests.jsonl> <out-prefix> [--layers all|none|1,2,3] [--prefix-cache]
//!
//! This is the experiment instrument behind `bench` (`mise run bench intent run`): it records what a model computes
//! for an intent routing prompt so the Python side can fit and compare readouts. It is separate from the app path
//! in `crates/dito-runtimes/src`, which decides with the fitted heads and does not share this code.
//!
//! Request line: `{"id", "system", "user", "assistant_prefix"?, "options": ["A", ...], "span"?, "generate"?}`.
//! The prompt is `system` and `user` in the GGUF's chat template, the assistant turn opened, an empty think block,
//! then `assistant_prefix`. Every option must be a single token. `span` is the text of the instruction inside the
//! rendered prompt (its last occurrence is used). `generate` is a token budget for greedy generation after the
//! prompt pass.
//!
//! Outputs:
//! - `<out>.jsonl`: per request `id`, `option_logits` (the logit of each option at the answer position, the last
//!   prompt token), `prompt_tokens`, `decoded_tokens` (prompt tokens not served by the resident prefix), `span`
//!   (token range `[start, end)` or null), `elapsed_ms` (tokenization plus the prompt pass) and `generated`
//!   (`{"text", "tokens", "total_ms"}` or null), the baseline a readout replaces.
//! - `<out>.bin`: little-endian f32, shape `[requests, kept layers, 2, n_embd]`. Layer `l` is the residual stream
//!   entering block `l`. Pool 0 is the answer position, pool 1 the mean over the tokens of `span`. Where the
//!   instruction lies inside the resident prefix, or no span was found, pool 1 falls back to the answer position.
//! - `<out>.meta.json`: model facts and how the run was made.
//!
//! `--prefix-cache` decodes the longest token prefix shared by every request once, keeps it in sequence 0 and copies
//! it into sequence 1 for each request, then decodes only the remainder, the way a resident router would. The copy
//! is needed because Qwen3.5 (`qwen35`, gated delta net blocks with recurrent state between its full attention
//! blocks) cannot drop the tail of a sequence. The run stops with an error when the copy does not reproduce the
//! full decode of the first request.
//!
//! Set `DITO_PROBE_VERBOSE` to see llama.cpp logs.

mod llama;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::exit;
use std::time::Instant;

use llama::Llama;
use serde::Deserialize;
use serde_json::json;

const USAGE: &str =
    "usage: intent_probe <model.gguf> <requests.jsonl> <out-prefix> [--layers all|none|1,2,3] [--prefix-cache]";
/// How far an option logit of the sequence copy may drift from the full decode before the run is rejected.
const COPY_TOLERANCE: f32 = 0.25;

#[derive(Deserialize)]
struct Request {
    id: String,
    system: String,
    user: String,
    #[serde(default)]
    assistant_prefix: String,
    options: Vec<String>,
    span: Option<String>,
    generate: Option<usize>,
}

struct Prepared {
    tokens: Vec<i32>,
    option_tokens: Vec<i32>,
    /// Token positions `[start, end)` of the instruction span, when one was given and found.
    span: Option<(usize, usize)>,
    tokenize_ms: f64,
}

fn prepare(llama: &Llama, request: &Request) -> Prepared {
    let started = Instant::now();
    let prompt = llama.render(&request.system, &request.user, &request.assistant_prefix);
    let tokens = llama.tokenize(&prompt, true);
    let tokenize_ms = started.elapsed().as_secs_f64() * 1000.0;
    let option_tokens = request
        .options
        .iter()
        .map(|option| match llama.tokenize(option, false).as_slice() {
            [token] => *token,
            other => panic!("option {option:?} is {} tokens, not one", other.len()),
        })
        .collect();
    let span = request.span.as_deref().and_then(|text| {
        let offset = prompt.rfind(text)?;
        let start = llama.tokenize(&prompt[..offset], true).len();
        let end = llama.tokenize(&prompt[..offset + text.len()], true).len().min(tokens.len());
        (end > start).then_some((start, end))
    });
    Prepared { tokens, option_tokens, span, tokenize_ms }
}

/// Length of the longest token prefix every request shares, short enough to leave each one at least one token.
fn common_prefix(prepared: &[Prepared]) -> usize {
    let first = &prepared[0].tokens;
    let mut len = first.len();
    for p in prepared {
        len = len.min(p.tokens.len() - 1);
        len = first.iter().zip(&p.tokens).take(len).take_while(|(a, b)| a == b).count();
    }
    len
}

fn argmax(values: &[f32]) -> usize {
    values.iter().enumerate().fold((0, f32::MIN), |best, (i, &v)| if v > best.1 { (i, v) } else { best }).0
}

/// Decode one request: the whole prompt in sequence 0, or with a resident prefix only the remainder in sequence 1.
fn decode_request(llama: &mut Llama, prefix: Option<usize>, tokens: &[i32]) {
    match prefix {
        None => {
            llama.clear();
            llama.decode(tokens, 0, 0);
        }
        Some(prefix) => {
            llama.fork();
            llama.decode(&tokens[prefix..], prefix, 1);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("{USAGE}");
        exit(2);
    }
    let (model_path, requests_path, out_prefix) = (&args[0], &args[1], &args[2]);
    let mut layers_arg = "none".to_owned();
    let mut prefix_cache = false;
    let mut rest = args[3..].iter();
    while let Some(flag) = rest.next() {
        match flag.as_str() {
            "--layers" => match rest.next() {
                Some(value) => layers_arg = value.clone(),
                None => {
                    eprintln!("--layers needs a value\n{USAGE}");
                    exit(2);
                }
            },
            "--prefix-cache" => prefix_cache = true,
            other => {
                eprintln!("unknown flag {other}\n{USAGE}");
                exit(2);
            }
        }
    }

    let mut llama = Llama::load(model_path);
    let layers: Vec<u32> = match layers_arg.as_str() {
        "none" => Vec::new(),
        "all" => (0..llama.n_layer).collect(),
        list => list.split(',').map(|l| l.parse().expect("layer number")).collect(),
    };
    llama.keep_layers(&layers);

    let requests: Vec<Request> = BufReader::new(std::fs::File::open(requests_path).expect("requests file"))
        .lines()
        .map(|line| line.expect("line"))
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(&line).expect("request JSON"))
        .collect();
    if requests.is_empty() {
        eprintln!("{requests_path} holds no requests");
        exit(2);
    }
    let prepared: Vec<Prepared> = requests.iter().map(|r| prepare(&llama, r)).collect();

    // Warm up: Metal compiles its kernels on the first decode. The first request also gives the full-decode logits
    // the sequence copy is checked against.
    llama.clear();
    llama.decode_chunked(&prepared[0].tokens, 0, 0);
    let reference: Vec<f32> = prepared[0].option_tokens.iter().map(|&t| llama.logits()[t as usize]).collect();

    let mut prefix = None;
    if prefix_cache {
        let len = common_prefix(&prepared);
        assert!(len > 0, "the requests share no prefix");
        llama.clear();
        llama.decode_chunked(&prepared[0].tokens[..len], 0, 0);
        llama.fork();
        llama.decode(&prepared[0].tokens[len..], len, 1);
        let drift = prepared[0]
            .option_tokens
            .iter()
            .zip(&reference)
            .map(|(&t, r)| (llama.logits()[t as usize] - r).abs())
            .fold(0.0, |max, d| if d.is_nan() || d > max { d } else { max });
        if drift.is_nan() || drift >= COPY_TOLERANCE {
            eprintln!(
                "error: the sequence copy of the {len} token prefix does not reproduce the full decode of the first \
                 request (option logits differ by {drift}, the limit is {COPY_TOLERANCE})"
            );
            exit(1);
        }
        prefix = Some(len);
    }

    let mut out = BufWriter::new(std::fs::File::create(format!("{out_prefix}.jsonl")).expect("jsonl"));
    let mut bin = BufWriter::new(std::fs::File::create(format!("{out_prefix}.bin")).expect("bin"));
    let d = llama.n_embd;
    for (request, p) in requests.iter().zip(&prepared) {
        let started = Instant::now();
        decode_request(&mut llama, prefix, &p.tokens);
        let logits = llama.logits();
        let option_logits: Vec<f32> = p.option_tokens.iter().map(|&t| logits[t as usize]).collect();
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0 + p.tokenize_ms;

        let last = p.tokens.len() - 1;
        let batch_start = llama.batch_start();
        for &layer in &layers {
            for pool in ["last", "span"] {
                let pooled = match (pool, p.span) {
                    ("span", Some((start, end))) if start >= batch_start => {
                        let mut mean = vec![0f32; d];
                        for pos in start..end {
                            for (m, x) in mean.iter_mut().zip(llama.layer_row(layer, pos)) {
                                *m += x;
                            }
                        }
                        mean.iter_mut().for_each(|m| *m /= (end - start) as f32);
                        mean
                    }
                    _ => llama.layer_row(layer, last).to_vec(),
                };
                for value in pooled {
                    bin.write_all(&value.to_le_bytes()).expect("bin write");
                }
            }
        }

        // Greedy generation: what a router that writes its answer has to do after the same prompt pass.
        let mut generated = None;
        if let Some(max_tokens) = request.generate {
            let mut bytes = Vec::new();
            let mut count = 0;
            let mut position = p.tokens.len();
            while count < max_tokens {
                let token = argmax(llama.logits()) as i32;
                if llama.is_end(token) {
                    break;
                }
                bytes.extend(llama.piece(token));
                llama.decode(&[token], position, i32::from(prefix.is_some()));
                position += 1;
                count += 1;
            }
            let total_ms = started.elapsed().as_secs_f64() * 1000.0 + p.tokenize_ms;
            generated = Some(json!({"text": String::from_utf8_lossy(&bytes), "tokens": count, "total_ms": total_ms}));
        }

        let record = json!({
            "id": request.id,
            "option_logits": option_logits,
            "prompt_tokens": p.tokens.len(),
            "decoded_tokens": p.tokens.len() - batch_start,
            "span": p.span,
            "elapsed_ms": elapsed_ms,
            "generated": generated,
        });
        writeln!(out, "{record}").expect("jsonl write");
    }
    out.flush().expect("flush");
    bin.flush().expect("flush");

    let meta = json!({
        "model": model_path,
        "architecture": llama.meta("general.architecture"),
        "architecture_meta": llama.architecture_meta().into_iter().collect::<BTreeMap<_, _>>(),
        "name": llama.meta("general.name"),
        "n_layer": llama.n_layer,
        "n_embd": llama.n_embd,
        "n_vocab": llama.n_vocab,
        "layers": layers,
        "pools": ["last", "span"],
        "requests": requests.len(),
        "reuse": match prefix {
            None => json!({"mode": "none"}),
            Some(prefix_tokens) => json!({"mode": "fork", "prefix_tokens": prefix_tokens}),
        },
    });
    std::fs::write(format!("{out_prefix}.meta.json"), serde_json::to_string_pretty(&meta).expect("meta"))
        .expect("meta write");
}
