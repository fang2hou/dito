//! Intent decisions through the app path: the catalog model loaded by `Runtimes`, the router prompt and bundled heads
//! of `dito-core`, and `dito_local::intent::decide`. `mise run bench intent decide` and `parity` run it.
//!
//!   cargo run --release -p dito-runtimes --example intent_decide -- <models dir> <catalog id> <in.jsonl> <out.jsonl>
//!
//! `<models dir>` holds `<catalog id>/<gguf>`, as Dito's own model folder does. An input line is `{"id", "text"}`.
//! An output line is `{"id", "intent", "confidence", "probabilities", "rewrite_op", "elapsed_ms"}`, with the
//! probabilities in intent order and `rewrite_op` either `null` or `{"label", "confidence"}`. The first instruction
//! is decided once before timing starts: that call decodes the router's prompt prefix, which then stays resident.

use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use dito_local::{RuntimeLoader, default_catalog, intent};
use dito_runtimes::Runtimes;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Instruction {
    id: String,
    text: String,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [models_dir, model_id, input, output] = args.as_slice() else {
        eprintln!("usage: intent_decide <models dir> <catalog id> <in.jsonl> <out.jsonl>");
        std::process::exit(2);
    };
    // llama.cpp writes several lines to stderr for every decode; set `DITO_PROBE_VERBOSE` to see them.
    if std::env::var_os("DITO_PROBE_VERBOSE").is_none() {
        llama_cpp_2::send_logs_to_tracing(llama_cpp_2::LogOptions::default().with_logs_enabled(false));
    }
    let spec = default_catalog()
        .into_iter()
        .find(|spec| spec.id == *model_id)
        .unwrap_or_else(|| panic!("{model_id} is not in the catalog"));
    let runtime = Runtimes.load_llm(&spec, &Path::new(models_dir).join(model_id)).expect("model loads");
    let instructions: Vec<Instruction> = BufReader::new(std::fs::File::open(input).expect("input file"))
        .lines()
        .map(|line| line.expect("input line"))
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(&line).expect("instruction JSON"))
        .collect();
    let stop = AtomicBool::new(false);
    if let Some(first) = instructions.first() {
        intent::decide(runtime.as_ref(), model_id, &first.text, &stop).expect("warm-up decision");
    }

    let mut out = BufWriter::new(std::fs::File::create(output).expect("output file"));
    for instruction in &instructions {
        let started = Instant::now();
        let decision = intent::decide(runtime.as_ref(), model_id, &instruction.text, &stop).expect("decision");
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let record = json!({
            "id": instruction.id,
            "intent": decision.intent.label(),
            "confidence": decision.confidence,
            "probabilities": decision.probabilities,
            "rewrite_op": decision.rewrite_op.map(|(op, confidence)| json!({"label": op.label(), "confidence": confidence})),
            "elapsed_ms": elapsed_ms,
        });
        writeln!(out, "{record}").expect("output line");
    }
    out.flush().expect("flush output");
}
