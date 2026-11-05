//! Startup arguments: what `main` parses before any service exists.

use std::path::PathBuf;

use anyhow::Result;

#[derive(Debug, Clone, Default)]
pub struct Opts {
    /// Run against in-process mock cloud endpoints and a synthetic microphone.
    pub mock: bool,
    /// With `--mock`: use the real microphone instead of the synthetic tone.
    pub real_mic: bool,
    /// Do not type into other apps; only log what would be inserted.
    pub log_sink: bool,
    /// Start without opening the settings window (login item).
    pub background: bool,
    /// Drive one dictation without keyboard input, print the outcome, then exit (smoke test).
    pub demo_run: bool,
    /// Use this 16-bit mono WAV as the microphone's recording (real engines, no microphone or permissions needed).
    pub wav: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    /// Render every window to PNG files in this directory and exit (needs the `screenshots` feature).
    pub screenshots: Option<PathBuf>,
    /// Pretend the system language is this one (ja, en, zh) instead of asking macOS.
    pub system_lang: Option<dito_core::i18n::Lang>,
}

impl Opts {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut o = Opts::default();
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--mock" => o.mock = true,
                "--real-mic" => o.real_mic = true,
                "--log-sink" => o.log_sink = true,
                "--background" => o.background = true,
                "--demo-run" => o.demo_run = true,
                "--wav" => o.wav = Some(it.next().ok_or_else(|| anyhow::anyhow!("--wav needs a file"))?.into()),
                "--data-dir" => {
                    o.data_dir = Some(it.next().ok_or_else(|| anyhow::anyhow!("--data-dir needs a path"))?.into())
                }
                "--lang" => {
                    let v = it.next().ok_or_else(|| anyhow::anyhow!("--lang needs ja, en or zh"))?;
                    o.system_lang = Some(
                        dito_core::i18n::Lang::from_tag(&v).ok_or_else(|| anyhow::anyhow!("unknown language: {v}"))?,
                    );
                }
                "--screenshots" => {
                    o.screenshots =
                        Some(it.next().ok_or_else(|| anyhow::anyhow!("--screenshots needs a directory"))?.into())
                }
                "-h" | "--help" => {
                    println!("{HELP}");
                    std::process::exit(0);
                }
                other => anyhow::bail!("unknown argument: {other}\n\n{HELP}"),
            }
        }
        Ok(o)
    }
}

const HELP: &str = "dito — voice dictation

  --mock              mock cloud endpoints + synthetic microphone (no keys, no network)
  --real-mic          with --mock: record from the microphone instead of a synthetic tone
  --log-sink          do not type into other apps; log the text instead
  --background        do not open the settings window at launch
  --wav <file>        use this 16-bit mono WAV as the recording instead of the microphone
  --demo-run          press the shortcut programmatically once, print the result, and exit
  --data-dir <dir>    config, history and models live here (default: ~/Library/Application Support/com.fang2hou.Dito)
  --lang <ja|en|zh>   act as if the system language were this (default: ask macOS)
  --screenshots <dir> render all windows to PNG and exit (build with --features screenshots)";
