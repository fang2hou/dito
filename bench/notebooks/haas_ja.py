"""Repeatable Haas-ja CPU exploration, using only existing bench dependencies.

Run from bench: uv run python notebooks/haas_ja.py --threads 4
Each invocation measures one recognizer in a fresh process. No microphone data is used.
"""

import argparse
import json
import resource
import statistics
import tempfile
import time
from pathlib import Path

import numpy as np
import soundfile as sf

from dito_bench import env
from dito_bench.asr.haas import HaasJaSherpa
from dito_bench.asr.registry import get_asr_spec
from dito_bench.audio import SAMPLE_RATE, load_16k_mono, sha256_file
from dito_bench.paths import AUDIO_DIR, RESULTS_DIR


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--threads", type=int, choices=(1, 2, 4, 8), default=4)
    parser.add_argument("--repeats", type=int, default=7)
    args = parser.parse_args()
    if args.repeats < 2:
        parser.error("--repeats must be at least 2")
    spec = get_asr_spec("haas-ja")
    spec.weights.fetch()  # Exclude network from initialization timing.
    start = time.perf_counter()
    model = HaasJaSherpa(spec.weights, num_threads=args.threads)
    load_s = time.perf_counter() - start
    samples = load_16k_mono(AUDIO_DIR / "ja_01.wav")
    records = []
    with tempfile.TemporaryDirectory() as temporary:
        paths = [AUDIO_DIR / "ja_01.wav", AUDIO_DIR / "ja_en_mix_01.wav"]
        for seconds in (15, 30):
            path = Path(temporary) / f"ja_repeated_{seconds}s.wav"
            count = seconds * SAMPLE_RATE
            sf.write(path, np.tile(samples, count // len(samples) + 1)[:count], SAMPLE_RATE)
            paths.append(path)
        for path in paths:
            start = time.perf_counter()
            first = model.transcribe(path)
            first_wall_s = time.perf_counter() - start
            measured = []
            wall = []
            for _ in range(args.repeats):
                start = time.perf_counter()
                result = model.transcribe(path)
                wall.append(time.perf_counter() - start)
                measured.append(result.elapsed_s)
            median = statistics.median(measured)
            records.append(
                {
                    "clip": path.stem,
                    "audio_sha256": sha256_file(path),
                    "audio_s": first.audio_s,
                    "first_decode_s": first.elapsed_s,
                    "first_wall_s": first_wall_s,
                    "decode_s": measured,
                    "wall_s": wall,
                    "median_decode_s": median,
                    "p90_decode_s": float(np.percentile(measured, 90)),
                    "rtfx": first.audio_s / median,
                    "text": first.text,
                }
            )
    output = {
        "environment": env.describe(spec.packages),
        "weights_identity": spec.weights.identity(),
        "threads": args.threads,
        "repeats": args.repeats,
        "initialization_s": load_s,
        "peak_process_rss_bytes": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
        "timing": "Decode excludes WAV IO and accept_waveform; wall includes both. "
        "One unmeasured warmup per clip. Long clips repeat synthetic Japanese audio, "
        "cropped to length, for scaling only. RSS is macOS process peak over the whole run.",
        "records": records,
    }
    directory = RESULTS_DIR / "exploration" / env.chip_slug()
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / f"haas-ja-threads-{args.threads}.json"
    path.write_text(json.dumps(output, ensure_ascii=False, indent=2) + "\n")
    print(path)


if __name__ == "__main__":
    main()
