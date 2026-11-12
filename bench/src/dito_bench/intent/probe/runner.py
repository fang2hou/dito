"""Run the `intent_probe` example of `dito-runtimes`, the instrument of this experiment.

One pass per request through the production llama.cpp binding: the option-letter logits at the
answer position and, when asked, the hidden states of chosen layers. Runs are stored in the run
cache (`runs.py`); one whose cases or prompt changed since it was taken is taken again.
"""

import json
import os
import subprocess
import time
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from ...paths import REPO_ROOT
from ..data import INTENTS, load_split
from ..models import resolve_gguf
from .runs import request_lines, run_exists, run_prefix, stale

PROBE_EXAMPLE = "intent_probe"

# Generating the answer, the baseline a readout replaces: what the model is told to add to the
# system prompt, and how many tokens it may write.
GENERATE = {
    "letter": ("", 4),
    "json": (
        '\n\nReply with JSON only, in the form {"intent": "<name>"}, where <name> is one of: '
        + ", ".join(INTENTS)
        + ".",
        24,
    ),
}


def build_example(example: str) -> Path:
    """Build a `dito-runtimes` example in release mode and return its binary."""
    subprocess.run(
        ["cargo", "build", "--release", "--locked", "-p", "dito-runtimes", "--example", example],
        cwd=REPO_ROOT,
        check=True,
    )
    target = Path(os.environ.get("CARGO_TARGET_DIR", REPO_ROOT / "target"))
    return target / "release" / "examples" / example


def mode_name(*, full: bool, generate: str | None, layers: str, tag: str) -> str:
    mode = "full" if full else "cached"
    if generate:
        mode += f"+gen-{generate}"
    if layers != "none":
        mode += f"+L{layers}"
    return mode + tag


@dataclass(frozen=True, slots=True)
class Job:
    key: str
    variant: str
    split: str


def build_requests(variant: str, split: str, generate: str | None) -> list[dict[str, Any]]:
    requests = request_lines(variant, load_split(split))
    if generate:
        suffix, max_tokens = GENERATE[generate]
        for request in requests:
            request["system"] += suffix
            request["generate"] = max_tokens
    return requests


def run_probes(
    keys: Sequence[str],
    variants: Sequence[str],
    splits: Sequence[str],
    *,
    chip: str | None = None,
    layers: str = "none",
    full: bool = False,
    generate: str | None = None,
    tag: str = "",
    gguf: Path | None = None,
    log: Callable[[str], None] = print,
) -> list[Path]:
    """Probe every model, variant and split whose run is missing, and return the new runs.

    `layers` is `none`, `all` or comma-separated layer numbers; `full` decodes the whole prompt
    every time instead of keeping the shared prefix resident. `gguf` overrides the model file and
    needs exactly one key.
    """
    if gguf is not None and len(keys) != 1:
        msg = "--gguf names one file, so it needs exactly one model"
        raise SystemExit(msg)
    mode = mode_name(full=full, generate=generate, layers=layers, tag=tag)
    pending = []
    for key in keys:
        for variant in variants:
            for split in splits:
                prefix = run_prefix(key, variant, mode, split, chip)
                if not run_exists(key, variant, mode, split, chip):
                    pending.append(Job(key, variant, split))
                elif stale(prefix, build_requests(variant, split, generate)):
                    log(f"stale {key} {variant}.{mode} {split}: the cases or the prompt changed")
                    pending.append(Job(key, variant, split))
                else:
                    log(f"skip {key} {variant}.{mode} {split}")
    if not pending:
        return []
    # Fail before any run starts, and before the build, when a model file is missing.
    models = {key: resolve_gguf(key, gguf) for key in {job.key for job in pending}}
    binary = build_example(PROBE_EXAMPLE)
    done = []
    for job in pending:
        out = run_prefix(job.key, job.variant, mode, job.split, chip)
        out.parent.mkdir(parents=True, exist_ok=True)
        requests = build_requests(job.variant, job.split, generate)
        requests_path = out.with_suffix(".requests.jsonl")
        requests_path.write_text(
            "\n".join(json.dumps(r, ensure_ascii=False) for r in requests) + "\n", encoding="utf-8"
        )
        command = [str(binary), str(models[job.key]), str(requests_path), str(out)]
        command += ["--layers", layers]
        if not full:
            command.append("--prefix-cache")
        started = time.perf_counter()
        subprocess.run(command, check=True)
        log(
            f"done {job.key} {job.variant}.{mode} {job.split} "
            f"({len(requests)} requests, {time.perf_counter() - started:.0f}s)"
        )
        done.append(out)
    return done
