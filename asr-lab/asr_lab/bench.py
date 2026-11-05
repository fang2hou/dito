"""Benchmark every model over the synthetic test set, or compare them on one file."""

from __future__ import annotations

import datetime as dt
import re
from pathlib import Path

import pandas as pd
from jiwer import cer, wer

from .models import DEFAULT_BENCHMARK, load_model
from .paths import RESULTS_DIR
from .testset import ensure_testset, load_manifest


def _error_rate(ref: str, hyp: str, lang: str) -> tuple[float, str]:
    """WER for English, CER (whitespace-stripped) for zh / ja / mixed clips."""
    if lang == "en":
        return wer(ref.lower(), hyp.lower()), "WER"
    strip = lambda s: re.sub(r"\s+", "", s)
    return cer(strip(ref), strip(hyp)), "CER"


def run_benchmark(
    models: list[str] | None = None, warmup: bool = True, save: bool = True
) -> pd.DataFrame:
    clips = [c for c in load_manifest() if Path(c["path"]).exists()]
    if not clips:
        clips = ensure_testset()
    rows = []
    for key in models or DEFAULT_BENCHMARK:
        model = load_model(key)
        if warmup:
            shortest = min(clips, key=lambda c: Path(c["path"]).stat().st_size)
            model.transcribe(shortest["path"])
            print(f"[warmup] {model.name} ready")
        for clip in clips:
            t = model.transcribe(clip["path"])
            err, metric = _error_rate(clip["text"], t.text, clip["lang"])
            rows.append(
                {
                    "clip": clip["name"],
                    "lang": clip["lang"],
                    "model": model.name,
                    "detected": t.language,
                    metric: round(err, 3),
                    "elapsed_s": round(t.elapsed_s, 3),
                    "x_realtime": round(t.rtf, 1),
                    "text": t.text,
                }
            )
    df = pd.DataFrame(rows)
    if save and not df.empty:
        RESULTS_DIR.mkdir(parents=True, exist_ok=True)
        stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
        df.to_csv(RESULTS_DIR / f"bench-{stamp}.csv", index=False)
    return df


def compare(audio: Path | str, models: list[str] | None = None) -> pd.DataFrame:
    """Transcribe one file with every model; no ground truth, for side-by-side reading."""
    rows = []
    for key in models or DEFAULT_BENCHMARK:
        model = load_model(key)
        t = model.transcribe(audio)
        rows.append(
            {
                "model": model.name,
                "detected": t.language,
                "elapsed_s": round(t.elapsed_s, 3),
                "x_realtime": round(t.rtf, 1),
                "text": t.text,
            }
        )
    return pd.DataFrame(rows)


if __name__ == "__main__":
    with pd.option_context("display.width", 250, "display.max_colwidth", 48):
        print(run_benchmark().to_string(index=False))
