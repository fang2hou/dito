//! Local ASR end-to-end smoke: load the catalog's Qwen3-ASR entry through the runtime loader and
//! transcribe a wav.
//!
//! Usage:
//! cargo run -p dito-runtimes --example local_asr_e2e -- <model_dir> <audio.wav> [hotword1,hotword2]

use std::path::Path;

use dito_local::RuntimeLoader;
use dito_runtimes::Runtimes;

/// Minimal RIFF reader for 16-bit PCM mono wav files (as produced by asr-lab).
fn read_wav_mono_f32(path: &Path) -> Result<Vec<f32>, String> {
    let b = std::fs::read(path).map_err(|e| e.to_string())?;
    if b.len() < 44 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".into());
    }
    let mut pos = 12;
    let mut fmt: Option<(u16, u32, u16)> = None; // (channels, rate, bits)
    let mut data: Option<(usize, usize)> = None; // (offset, len)
    while pos + 8 <= b.len() {
        let id = &b[pos..pos + 4];
        let len = u32::from_le_bytes(b[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = pos + 8;
        match id {
            b"fmt " => {
                let channels = u16::from_le_bytes(b[body + 2..body + 4].try_into().unwrap());
                let rate = u32::from_le_bytes(b[body + 4..body + 8].try_into().unwrap());
                let bits = u16::from_le_bytes(b[body + 14..body + 16].try_into().unwrap());
                fmt = Some((channels, rate, bits));
            }
            b"data" => data = Some((body, len.min(b.len() - body))),
            _ => {}
        }
        pos = body + len + (len & 1);
    }
    let (channels, rate, bits) = fmt.ok_or("missing fmt chunk")?;
    if channels != 1 || rate != 16_000 || bits != 16 {
        return Err(format!("expected 16 kHz mono s16, got {rate} Hz {channels}ch {bits}bit"));
    }
    let (off, len) = data.ok_or("missing data chunk")?;
    let n = len / 2;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let lo = b[off + i * 2];
        let hi = b[off + i * 2 + 1];
        out.push(i16::from_le_bytes([lo, hi]) as f32 / 32768.0);
    }
    Ok(out)
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let (dir, wav) = (args.get(1).ok_or("model dir arg")?, args.get(2).ok_or("wav arg")?);
    let hotwords: Vec<String> =
        args.get(3).map(|s| s.split(',').filter(|w| !w.is_empty()).map(String::from).collect()).unwrap_or_default();
    let model_id = args.get(4).map(String::as_str).unwrap_or("qwen3-asr-0.6b");

    let spec = dito_local::catalog::default_catalog()
        .into_iter()
        .find(|m| m.id == model_id)
        .ok_or("catalog entry not found")?;
    let asr = Runtimes.load_asr(&spec, Path::new(dir))?;

    let samples = read_wav_mono_f32(Path::new(wav))?;
    for pass in [false, true] {
        let words: Vec<String> = if pass { hotwords.clone() } else { vec![] };
        let t = std::time::Instant::now();
        let out = asr.transcribe(&samples, None, &words)?;
        println!("[{:?} hotwords={pass}] {}", t.elapsed(), out.text);
        println!("  language: {:?}", out.language);
    }
    Ok(())
}
