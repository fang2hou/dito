"""Readout versus generation on the same prompt: time, accuracy, and how often the written answer
fails to parse."""

import json
import re
from collections.abc import Sequence
from typing import Any

import numpy as np

from ..data import INTENTS
from ..probe.runs import load_run, run_exists
from ..prompts import FIRST_VARIANT, VARIANTS
from ..report.tables import md
from ..stats import softmax

# The first-variant test runs the study compares, as (`full`, `generate`) for the probe: readout and
# generation, with the whole prompt decoded each time or the prefix resident.
EFFICIENCY_RUNS = ((True, None), (True, "letter"), (True, "json"), (False, None))
MODES = (
    ("readout, whole prompt", "full", None),
    ("readout, resident prefix", "cached", None),
    ("generate a letter, whole prompt", "full+gen-letter", "letter"),
    ("generate JSON, whole prompt", "full+gen-json", "json"),
    ("generate JSON, resident prefix", "cached+gen-json", "json"),
)
COLUMNS = ["model", "path", "accuracy", "unparsed", "output tokens", "p50 ms", "p95 ms"]


def parse(text: str, kind: str, variant: str) -> str | None:
    """The intent a generated answer names, or None when it does not parse."""
    if kind == "letter":
        v = VARIANTS[variant]
        first = text.strip()[:1]
        return v.order[v.labels.index(first)] if first in v.labels else None
    match = re.search(r"\{.*\}", text, re.S)
    try:
        intent = json.loads(match.group(0))["intent"] if match else None
    except json.JSONDecodeError, KeyError, TypeError:
        return None
    return intent if intent in INTENTS else None


def rows(
    keys: Sequence[str], variant: str = FIRST_VARIANT, chip: str | None = None
) -> list[dict[str, Any]]:
    out = []
    for key in keys:
        for label, mode, kind in MODES:
            if not run_exists(key, variant, mode, "test", chip):
                continue
            run = load_run(key, variant, "test", mode, chip)
            gold = [INTENTS[i] for i in run.labels]
            if kind is None:
                predicted = [INTENTS[i] for i in softmax(run.logits).argmax(axis=1)]
                times, tokens = run.elapsed, np.zeros(len(gold))
            else:
                predicted = [parse(r["generated"]["text"], kind, variant) for r in run.records]
                times = np.asarray([r["generated"]["total_ms"] for r in run.records])
                tokens = np.asarray([r["generated"]["tokens"] for r in run.records])
            out.append(
                {
                    "model": key,
                    "path": label,
                    "accuracy": float(
                        np.mean([p == g for p, g in zip(predicted, gold, strict=True)])
                    ),
                    "unparsed": int(sum(p is None for p in predicted)),
                    "output_tokens": float(tokens.mean()),
                    "p50_ms": float(np.percentile(times, 50)),
                    "p95_ms": float(np.percentile(times, 95)),
                }
            )
    return out


def efficiency_markdown(table: list[dict[str, Any]]) -> str:
    return md(
        COLUMNS,
        [
            [
                r["model"],
                r["path"],
                f"{r['accuracy']:.3f}",
                r["unparsed"],
                f"{r['output_tokens']:.1f}",
                f"{r['p50_ms']:.0f}",
                f"{r['p95_ms']:.0f}",
            ]
            for r in table
        ],
    )
