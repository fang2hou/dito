"""Measure the external haas-mlx port without adding it as a bench dependency.

Clone https://github.com/matsuurajun/haas-mlx outside this repository, then run:
uv run python notebooks/haas_ja_mlx.py --source /path/to/haas-mlx
Requires the existing mlx extra and its sentencepiece dependency.
"""

import argparse
import importlib
import json
import resource
import statistics
import subprocess
import sys
import time
from pathlib import Path

import numpy as np

from dito_bench import env
from dito_bench.audio import SAMPLE_RATE, load_16k_mono, sha256_file
from dito_bench.paths import AUDIO_DIR, MODELS_DIR, RESULTS_DIR
from dito_bench.weights import Weights


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.source.resolve() / "src"))
    model_module = importlib.import_module("haas_mlx.model")
    mx = importlib.import_module("mlx.core")
    spm = importlib.import_module("sentencepiece")
    weights = Weights("mrjnzzz/Haas-ja-v1.0-mlx", MODELS_DIR / "haas-ja-mlx")
    directory = weights.fetch()
    start = time.perf_counter()
    model = model_module.HaasModel.load(directory)
    mx.eval(model.parameters())
    tokenizer = spm.SentencePieceProcessor(model_file=str(directory / "tokenizer.model"))
    load_s = time.perf_counter() - start
    records = []
    source = load_16k_mono(AUDIO_DIR / "ja_01.wav")
    inputs = [
        (name, load_16k_mono(AUDIO_DIR / f"{name}.wav")) for name in ("ja_01", "ja_en_mix_01")
    ]
    for seconds in (15, 30):
        count = seconds * SAMPLE_RATE
        inputs.append(
            (f"ja_repeated_{seconds}s", np.tile(source, count // len(source) + 1)[:count])
        )
    for name, samples in inputs:
        waveform = mx.array(samples)
        mx.eval(waveform)
        measured = []
        first_s = 0.0
        text = ""
        for iteration in range(8):
            mx.synchronize()
            start = time.perf_counter()
            tokens = model.transcribe_tokens(waveform)
            text = tokenizer.decode(tokens)
            mx.synchronize()
            elapsed = time.perf_counter() - start
            if iteration == 0:
                first_s = elapsed
            else:
                measured.append(elapsed)
        median = statistics.median(measured)
        records.append(
            {
                "clip": name,
                "audio_s": len(samples) / SAMPLE_RATE,
                "first_s": first_s,
                "elapsed_s": measured,
                "median_s": median,
                "p90_s": float(np.percentile(measured, 90)),
                "rtfx": len(samples) / SAMPLE_RATE / median,
                "text": text,
            }
        )
    revision = subprocess.run(
        ["git", "-C", str(args.source), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    output = {
        "environment": env.describe(("mlx", "sentencepiece")),
        "weights_identity": weights.identity(),
        "runtime_source_revision": revision,
        "source_clip_sha256": sha256_file(AUDIO_DIR / "ja_01.wav"),
        "initialization_s": load_s,
        "peak_process_rss_bytes": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
        "mlx_peak_memory_bytes": mx.get_peak_memory(),
        "timing": "Seven warm repetitions after one warmup per input. Includes log-mel, "
        "encoder, decoder and tokenizer, excludes WAV IO and waveform GPU upload. "
        "Synchronizes Metal before and after timing. Long inputs repeat synthetic speech.",
        "records": records,
    }
    directory = RESULTS_DIR / "exploration" / env.chip_slug()
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / "haas-ja-mlx-bf16.json"
    path.write_text(json.dumps(output, ensure_ascii=False, indent=2) + "\n")
    print(path)


if __name__ == "__main__":
    main()
