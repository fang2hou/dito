//! Startup arguments: what `main` parses before any service exists.

use std::path::PathBuf;

use anyhow::Result;

#[derive(Debug, Clone, Default)]
pub struct Opts {
    pub mock: bool,
    pub real_mic: bool,
    pub log_sink: bool,
    pub open_settings: bool,
    pub demo_run: bool,
    pub wav: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub screenshots: Option<PathBuf>,
    pub system_lang: Option<dito_core::i18n::Lang>,
}

impl Opts {
    /// Automated runs (smoke test, screenshots) must not take the foreground from the person using the Mac.
    pub fn is_headless(&self) -> bool {
        self.demo_run || self.screenshots.is_some()
    }

    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut o = Opts::default();
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--mock" => {
                    require_feature("--mock", "mock", cfg!(feature = "mock"))?;
                    o.mock = true;
                }
                "--real-mic" => o.real_mic = true,
                "--log-sink" => o.log_sink = true,
                "--open-settings" => o.open_settings = true,
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
                    require_feature("--screenshots", "screenshots", cfg!(feature = "screenshots"))?;
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

fn require_feature(flag: &str, feature: &str, compiled_in: bool) -> Result<()> {
    anyhow::ensure!(
        compiled_in,
        "{flag} needs a build with the `{feature}` feature (cargo run -p dito-app --features {feature})"
    );
    Ok(())
}

const HELP: &str = "dito — voice dictation

  --mock              mock cloud endpoints + synthetic microphone (build with --features mock)
  --real-mic          with --mock: record from the microphone instead of a synthetic tone
  --log-sink          do not type into other apps; log the text instead
  --open-settings     open the settings window at launch (for checking a build)
  --wav <file>        use this 16-bit mono WAV as the recording instead of the microphone
  --demo-run          press the shortcut programmatically once, print the result, and exit
  --data-dir <dir>    config, history and models live here (default: ~/Library/Application Support/com.fang2hou.Dito, debug builds: com.fang2hou.Dito-dev)
  --lang <ja|en|zh>   act as if the system language were this (default: ask macOS)
  --screenshots <dir> render all windows to PNG and exit (build with --features screenshots)";

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Opts> {
        Opts::parse(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn unknown_arguments_fail_with_usage() {
        let err = parse(&["--nope"]).unwrap_err().to_string();
        assert!(err.contains("unknown argument: --nope"), "{err}");
    }

    #[test]
    fn feature_flags_fail_when_the_feature_is_not_compiled_in() {
        assert_eq!(parse(&["--mock"]).is_ok(), cfg!(feature = "mock"));
        assert_eq!(parse(&["--screenshots", "out"]).is_ok(), cfg!(feature = "screenshots"));
        if !cfg!(feature = "mock") {
            assert!(parse(&["--mock"]).unwrap_err().to_string().contains("`mock` feature"));
        }
    }
}
