"""Production parity: the app path against the numpy path.

The app path is the `intent_decide` example of `dito-runtimes`, which loads a model through the
runtime and applies the heads bundled in `dito-core`; the numpy path is this project's. Also stores
`production.json`, the accuracy and timing of the app path.
"""

import json
import subprocess
import tempfile
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import Any

import numpy as np

from ..data import INTENTS, load_cases
from ..locations import results_dir
from ..models import MODEL_KEYS, catalog_id, resolve_gguf
from ..probe.runner import build_example
from ..probe.runs import FIT_MODE, load_run
from ..prompts import SHIPPED_VARIANT, prompt_sha256
from ..stats import AUTO
from .asset import HEADS_DIR, intent_probabilities, read_heads

DECIDE_EXAMPLE = "intent_decide"


def heads_path(key: str) -> Path:
    return HEADS_DIR / f"{catalog_id(key)}.heads"


def run_decide(binary: Path, key: str, items: Sequence[tuple[str, str]]) -> list[dict[str, Any]]:
    """Decide every (id, text) with one model load. Output lines follow the input order."""
    models_dir = resolve_gguf(key).parent.parent
    with tempfile.TemporaryDirectory() as tmp:
        requests, out = Path(tmp) / "instructions.jsonl", Path(tmp) / "decisions.jsonl"
        lines = (json.dumps({"id": i, "text": t}, ensure_ascii=False) for i, t in items)
        requests.write_text("\n".join(lines) + "\n", encoding="utf-8")
        subprocess.run([binary, models_dir, catalog_id(key), requests, out], check=True)
        lines = out.read_text(encoding="utf-8").splitlines()
    return [json.loads(line) for line in lines]


def decide_rows(keys: Sequence[str], texts: Sequence[str]) -> list[dict[str, str]]:
    """One row per model and instruction, as the app would decide it."""
    for key in keys:
        resolve_gguf(key)
    binary = build_example(DECIDE_EXAMPLE)
    rows = []
    for key in keys:
        decisions = run_decide(binary, key, [(str(n), t) for n, t in enumerate(texts)])
        for text, d in zip(texts, decisions, strict=True):
            op = d["rewrite_op"]
            rows.append(
                {
                    "model": key,
                    "text": text,
                    "intent": d["intent"],
                    "confidence": f"{d['confidence']:.3f}",
                    "auto": "yes" if d["confidence"] >= AUTO else "no",
                    "rewrite op": op["label"] if d["intent"] == "rewrite" and op else "",
                    "ms": f"{d['elapsed_ms']:.1f}",
                }
            )
    return rows


def parity_rows(
    keys: Sequence[str], chip: str | None = None, log: Callable[[str], None] = print
) -> list[dict[str, Any]]:
    """Run the test cases through the app path and compare it with numpy.

    The numpy side reads the `.heads` asset back and applies it to the stored hidden states and
    option logits of the shipped variant, so the comparison covers the asset, the folding and the
    Rust implementation of the heads.
    """
    for key in keys:
        resolve_gguf(key)
    binary = build_example(DECIDE_EXAMPLE)
    cases = load_cases()
    rows = []
    for key in keys:
        asset = read_heads(heads_path(key))
        if asset.prompt_sha256 != prompt_sha256():
            msg = f"{heads_path(key)} was fitted on another prompt; run `intent export`"
            raise ValueError(msg)
        log(f"{key}: deciding {len(cases)} test cases")
        decisions = run_decide(binary, key, [(c.id, c.text) for c in cases])
        if [d["id"] for d in decisions] != [c.id for c in cases]:
            msg = f"{key}: intent_decide returned the cases in another order"
            raise ValueError(msg)
        run = load_run(key, SHIPPED_VARIANT, "test", FIT_MODE, chip)
        expected = intent_probabilities(asset, run)
        got = np.asarray([d["probabilities"] for d in decisions])
        predicted = np.asarray([INTENTS.index(d["intent"]) for d in decisions])
        confidence = np.asarray([d["confidence"] for d in decisions])
        auto = confidence >= AUTO
        elapsed = np.asarray([d["elapsed_ms"] for d in decisions])
        # The decision record carries no token count; the probe run decoded the same prompts.
        tokens = [d["prompt_tokens"] for d in decisions if "prompt_tokens" in d] or [
            r["prompt_tokens"] for r in run.records
        ]
        rows.append(
            {
                "model": key,
                "cases": len(cases),
                "max_abs_diff": float(np.abs(expected - got).max()),
                "same_top": float((expected.argmax(axis=1) == got.argmax(axis=1)).mean()),
                "accuracy": float((predicted == run.labels).mean()),
                "auto": float(auto.mean()),
                "auto_acc": float((predicted[auto] == run.labels[auto]).mean())
                if auto.any()
                else float("nan"),
                "auto_wrong": int((predicted[auto] != run.labels[auto]).sum()),
                "p50_ms": float(np.percentile(elapsed, 50)),
                "p95_ms": float(np.percentile(elapsed, 95)),
                "prompt_tokens": int(np.median(tokens)) if tokens else None,
            }
        )
    return rows


def production_path(chip: str | None = None) -> Path:
    return results_dir(chip) / "production.json"


def read_production(chip: str | None = None) -> list[dict[str, Any]]:
    path = production_path(chip)
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else []


def write_production(rows: Sequence[dict[str, Any]], chip: str | None = None) -> Path:
    """Store `rows`, keeping the stored rows of models that were not run this time."""
    merged = {r["model"]: r for r in read_production(chip)} | {r["model"]: r for r in rows}
    path = production_path(chip)
    path.parent.mkdir(parents=True, exist_ok=True)
    ordered = [merged[k] for k in MODEL_KEYS if k in merged]
    path.write_text(json.dumps(ordered, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    return path
