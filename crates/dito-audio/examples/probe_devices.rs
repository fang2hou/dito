//! List input devices and measure a short live RMS level on each, marking the system default.
//! Run while making noise: the device whose level moves is the one the user is actually on.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(3);
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.description().ok().map(|d| d.name().to_string()));
    let devices: Vec<cpal::Device> = host.input_devices().map(|it| it.collect()).unwrap_or_default();
    if devices.is_empty() {
        println!("no input devices visible (check microphone permission)");
        return;
    }
    println!("system default: {}\n", default_name.as_deref().unwrap_or("(none)"));

    let done = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();
    for d in devices {
        let name = d.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "?".into());
        let is_default = default_name.as_deref() == Some(name.as_str());
        let peak = Arc::new(AtomicU32::new(0));
        let done = done.clone();
        let peak2 = peak.clone();
        let name_for_err = name.clone();
        let h = std::thread::spawn(move || {
            let ok = (|| -> Result<cpal::Stream, String> {
                let config = d.default_input_config().map_err(|e| e.to_string())?;
                let ch = config.channels() as usize;
                let fmt = config.sample_format();
                let peak = peak2;
                let measure = move |mono: &[f32]| {
                    let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len().max(1) as f32).sqrt();
                    let prev = f32::from_bits(peak.load(Ordering::Relaxed));
                    if rms > prev {
                        peak.store(rms.to_bits(), Ordering::Relaxed);
                    }
                };
                let mut mono = Vec::new();
                let name_err = name_for_err.clone();
                let err = move |e: cpal::Error| eprintln!("  stream error on {name_err}: {e}");
                let stream = match fmt {
                    SampleFormat::F32 => d.build_input_stream(
                        config.into(),
                        move |data: &[f32], _: &_| {
                            downmix(data, ch, &mut mono);
                            measure(&mono);
                        },
                        err,
                        None,
                    ),
                    SampleFormat::I16 => d.build_input_stream(
                        config.into(),
                        move |data: &[i16], _: &_| {
                            downmix(data, ch, &mut mono);
                            measure(&mono);
                        },
                        err,
                        None,
                    ),
                    SampleFormat::I32 => d.build_input_stream(
                        config.into(),
                        move |data: &[i32], _: &_| {
                            downmix(data, ch, &mut mono);
                            measure(&mono);
                        },
                        err,
                        None,
                    ),
                    other => return Err(format!("unsupported format {other}")),
                }
                .map_err(|e| e.to_string())?;
                stream.play().map_err(|e| e.to_string())?;
                Ok(stream)
            })();
            match ok {
                Ok(s) => {
                    while !done.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    drop(s);
                }
                Err(e) => eprintln!("{name_for_err}: {e}"),
            }
        });
        handles.push((h, name, is_default, peak));
    }

    println!("listening for {secs}s — make some noise…");
    std::thread::sleep(Duration::from_secs(secs));
    done.store(true, Ordering::Relaxed);
    println!("\npeak RMS per device:");
    let mut rows: Vec<_> = handles
        .into_iter()
        .map(|(h, name, is_default, peak)| {
            let _ = h.join();
            let rms = f32::from_bits(peak.load(Ordering::Relaxed));
            (name, is_default, rms)
        })
        .collect();
    rows.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    for (name, is_default, rms) in rows {
        let bar = "█".repeat((rms * 150.0) as usize);
        println!("  {:>8.5} {} {:?} {}", rms, if is_default { "[default]" } else { "          " }, name, bar);
    }
}

fn downmix<T: Sample>(data: &[T], channels: usize, mono: &mut Vec<f32>)
where
    f32: cpal::FromSample<T>,
{
    mono.clear();
    for frame in data.chunks(channels.max(1)) {
        let sum: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum();
        mono.push(sum / frame.len() as f32);
    }
}
