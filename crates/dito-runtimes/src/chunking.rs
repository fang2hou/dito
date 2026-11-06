//! Long clips are cut at pauses before recognition, and the pieces' text is joined back.

/// SenseVoice-Small was trained on utterances up to 30 s; fed longer clips it loses punctuation and
/// whole syllables. Cut at the quietest 25 ms frame inside the last 6 s of a 28 s stretch so the
/// seam lands in a pause when there is one.
const CHUNK: usize = 28 * 16_000;
const SEARCH_BACK: usize = 6 * 16_000;
const FRAME: usize = 400; // 25 ms

pub(crate) fn split_at_pauses(samples: &[f32]) -> Vec<&[f32]> {
    if samples.len() <= CHUNK {
        return vec![samples];
    }
    let mut cuts = Vec::new();
    let mut start = 0;
    while samples.len() - start > CHUNK {
        let from = start + CHUNK - SEARCH_BACK;
        let mut best = from;
        let mut best_energy = f32::INFINITY;
        let mut i = (from / FRAME + 1) * FRAME; // first frame fully inside the search span
        while i + FRAME <= start + CHUNK {
            let energy = samples[i..i + FRAME].iter().map(|s| s * s).sum::<f32>();
            if energy < best_energy {
                best_energy = energy;
                best = i + FRAME / 2;
            }
            i += FRAME;
        }
        cuts.push(best);
        start = best;
    }
    cuts.dedup();
    let mut spans: Vec<&[f32]> = Vec::with_capacity(cuts.len() + 1);
    let mut start = 0;
    for cut in cuts {
        spans.push(&samples[start..cut]);
        start = cut;
    }
    spans.push(&samples[start..]);
    spans
}

/// CJK chunk seams carry their own punctuation and take no separator; Latin seams get a space so
/// words from neighboring chunks cannot merge.
pub(crate) fn join_chunks(texts: Vec<String>) -> String {
    let mut out = String::new();
    for text in texts {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if !out.is_empty()
            && out.ends_with(|c: char| c.is_ascii_alphanumeric())
            && text.starts_with(|c: char| c.is_ascii_alphanumeric())
        {
            out.push(' ');
        }
        out.push_str(text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3 s of tone then 1 s of silence, repeated — pauses land every 4 s.
    fn pattern(seconds: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(seconds * 16_000);
        while v.len() < seconds * 16_000 {
            v.extend(std::iter::repeat_n(0.3, 3 * 16_000));
            v.extend(std::iter::repeat_n(0.0, 16_000));
        }
        v.truncate(seconds * 16_000);
        v
    }

    #[test]
    fn short_clips_stay_whole() {
        let v = pattern(20);
        let spans = split_at_pauses(&v);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].len(), v.len());
    }

    #[test]
    fn long_clips_are_split_inside_pauses() {
        let v = pattern(60);
        let spans = split_at_pauses(&v);
        assert!(spans.len() > 1, "60 s must be split, got {} spans", spans.len());
        assert!(spans.iter().all(|s| s.len() <= CHUNK), "a span exceeds the model's window");
        for span in &spans {
            let seam = span.last().expect("nonempty span");
            assert!(seam.abs() < 0.05, "cut landed in speech, amplitude {}", seam);
        }
        assert_eq!(spans.iter().map(|s| s.len()).sum::<usize>(), v.len(), "samples lost at a seam");
    }

    #[test]
    fn continuous_speech_is_split_anyway() {
        let v = vec![0.3; CHUNK + 16_000];
        let spans = split_at_pauses(&v);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans.iter().map(|s| s.len()).sum::<usize>(), v.len());
    }

    #[test]
    fn chunk_seams_join_without_merging_words() {
        assert_eq!(join_chunks(vec!["第一段。".into(), "第二段。".into()]), "第一段。第二段。");
        assert_eq!(join_chunks(vec!["first part".into(), "second part".into()]), "first part second part");
        assert_eq!(join_chunks(vec!["value is 300".into(), "毫秒左右".into()]), "value is 300毫秒左右");
        assert_eq!(join_chunks(vec!["".into(), "  ".into()]), "");
    }
}
