//! Reads an uploaded WAV file into the 16 kHz mono clip every engine consumes. Other containers are refused: decoding
//! them would need a codec library, and a client can convert to WAV first.

use crate::audio::{AudioClip, SAMPLE_RATE};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum WavError {
    NotWav,
    Unsupported(&'static str),
}

const FORMAT_PCM: u16 = 1;
const FORMAT_FLOAT: u16 = 3;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;

struct Format {
    tag: u16,
    channels: usize,
    rate: u32,
    bits: u16,
}

pub(super) fn decode(bytes: &[u8]) -> Result<AudioClip, WavError> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotWav);
    }
    let (mut format, mut data) = (None, None);
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]) as usize;
        let body = &bytes[at + 8..];
        // A streamed file may announce a data chunk longer than what arrived; the bytes there are the audio.
        let body = &body[..size.min(body.len())];
        match id {
            b"fmt " if body.len() >= 16 => {
                let word = |i: usize| u16::from_le_bytes([body[i], body[i + 1]]);
                let mut tag = word(0);
                if tag == FORMAT_EXTENSIBLE && body.len() >= 26 {
                    tag = word(24);
                }
                format = Some(Format {
                    tag,
                    channels: usize::from(word(2)),
                    rate: u32::from_le_bytes([body[4], body[5], body[6], body[7]]),
                    bits: word(14),
                });
            }
            b"data" => data = Some(body),
            _ => {}
        }
        at += 8 + size + (size & 1);
    }
    let (Some(format), Some(data)) = (format, data) else { return Err(WavError::NotWav) };
    if format.channels == 0 || format.rate == 0 {
        return Err(WavError::NotWav);
    }
    let samples: Vec<f32> = match (format.tag, format.bits) {
        (FORMAT_PCM, 16) => {
            data.as_chunks::<2>().0.iter().map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.).collect()
        }
        (FORMAT_FLOAT, 32) => data.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect(),
        _ => return Err(WavError::Unsupported("only 16-bit PCM and 32-bit float WAV files are supported")),
    };
    let mono: Vec<f32> =
        samples.chunks_exact(format.channels).map(|frame| frame.iter().sum::<f32>() / format.channels as f32).collect();
    Ok(AudioClip::new(resample(&mono, format.rate)))
}

/// Linear interpolation, averaging first when the rate goes down so the high end does not fold back into the speech
/// band. Speech recognition does not need a better filter than this.
fn resample(samples: &[f32], rate: u32) -> Vec<f32> {
    if rate == SAMPLE_RATE || samples.is_empty() {
        return samples.to_vec();
    }
    let step = f64::from(rate) / f64::from(SAMPLE_RATE);
    let window = step.floor().max(1.) as usize;
    let out_len = (samples.len() as f64 / step).floor() as usize;
    (0..out_len)
        .map(|i| {
            let position = i as f64 * step;
            let at = position.floor() as usize;
            let fraction = (position - at as f64) as f32;
            let sample = |n: usize| {
                let from = n.saturating_sub(window / 2);
                let to = (from + window).min(samples.len());
                samples[from..to].iter().sum::<f32>() / (to - from) as f32
            };
            let next = (at + 1).min(samples.len() - 1);
            sample(at) * (1. - fraction) + sample(next) * fraction
        })
        .collect()
}

#[cfg(test)]
pub(super) fn encode_pcm16(samples: &[i16], rate: u32, channels: u16) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_16_khz_mono_file_comes_through_unchanged() {
        let clip = decode(&encode_pcm16(&[0, 16384, -16384, 32767], 16_000, 1)).unwrap();
        assert_eq!(clip.samples.len(), 4);
        assert!((clip.samples[1] - 0.5).abs() < 1e-4 && (clip.samples[2] + 0.5).abs() < 1e-4);
    }

    #[test]
    fn stereo_is_averaged_and_other_rates_are_brought_to_16_khz() {
        let stereo = decode(&encode_pcm16(&[16384, -16384, 8192, 8192], 16_000, 2)).unwrap();
        assert_eq!(stereo.samples.len(), 2);
        assert!(stereo.samples[0].abs() < 1e-4, "opposite channels cancel");
        let tone: Vec<i16> = (0..48_000).map(|n| ((n as f32 * 0.05).sin() * 10_000.) as i16).collect();
        let down = decode(&encode_pcm16(&tone, 48_000, 1)).unwrap();
        assert_eq!(down.samples.len(), 16_000, "one second stays one second");
    }

    #[test]
    fn other_containers_and_sample_formats_are_refused() {
        assert_eq!(decode(b"ID3\x04 not a wav file at all"), Err(WavError::NotWav));
        let mut eight_bit = encode_pcm16(&[0, 0], 16_000, 1);
        eight_bit[34] = 8;
        assert!(matches!(decode(&eight_bit), Err(WavError::Unsupported(_))));
    }
}
