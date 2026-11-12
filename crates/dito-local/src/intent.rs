//! Intent decisions on a loaded chat model: the request from `dito-core`, one pass of the runtime, then the heads.

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use dito_core::intent::{self, Decision};
use dito_core::trace_log;

use crate::runtime::LlmRuntime;
use crate::runtime_error::RuntimeError;

/// What the spoken `instruction` asks for, decided by the model with catalog id `model_id` in one prompt pass.
/// A model without bundled heads decides by its option logits alone. Blocking: call it where inference runs.
pub fn decide(
    runtime: &dyn LlmRuntime,
    model_id: &str,
    instruction: &str,
    stop: &AtomicBool,
) -> Result<Decision, RuntimeError> {
    let heads = intent::bundled(model_id);
    let started = Instant::now();
    let reading = runtime.read(&intent::request(instruction, heads), stop)?;
    trace_log!(
        model = model_id,
        heads = heads.is_some(),
        read_ms = started.elapsed().as_millis() as u64,
        option_logits = ?reading.option_logits,
        "router read"
    );
    intent::decide(heads, &reading).map_err(RuntimeError::engine)
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use crate::catalog::{ModelFormat, default_catalog};

    /// Heads are valid for one GGUF file and one router prompt. A catalog file or a prompt changed without a refit
    /// (`mise run bench intent all`) must fail here, because the app would otherwise route worse without a sign.
    #[test]
    fn bundled_heads_match_the_catalog_file_and_the_router_prompt() {
        let prompt = hex::encode(Sha256::digest(dito_core::intent::fingerprint_text()));
        let mut checked = 0;
        for spec in default_catalog() {
            let (Some(heads), ModelFormat::Chat { model, .. }) = (dito_core::intent::bundled(&spec.id), spec.format)
            else {
                continue;
            };
            let file = spec.files.iter().find(|file| file.name == model).expect("the chat file is in the catalog");
            assert_eq!(Some(&heads.gguf_sha256), file.sha256.as_ref(), "{}: heads fitted on another file", spec.id);
            assert_eq!(heads.prompt_sha256, prompt, "{}: heads fitted on another prompt", spec.id);
            checked += 1;
        }
        assert_eq!(checked, 3);
    }
}
