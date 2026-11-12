"""The direct readout by prompt variant: accuracy, confidence and cost, per split."""

from typing import Any

import numpy as np

from ..data import LANGS, TAGS
from ..probe.runs import load_run, run_exists
from ..prompts import VARIANTS
from ..report.tables import md, num
from ..stats import metrics, softmax

# The prompt sweep was recorded for one model only.
SWEEP_MODEL = "qwen3.5-4b"
COLUMNS = [
    "variant",
    "acc",
    "f1",
    "ece",
    "auto",
    "auto_acc",
    "tokens",
    "ms",
    "zh",
    "ja",
    "en",
    *TAGS,
]


def rows(
    key: str, split: str, mode: str = "cached", chip: str | None = None
) -> list[dict[str, Any]]:
    """One row per variant that has a run in `mode`, in the order of `VARIANTS`."""
    out = []
    for name in VARIANTS:
        if not run_exists(key, name, mode, split, chip):
            continue
        run = load_run(key, name, split, mode, chip)
        p = softmax(run.logits)
        correct = p.argmax(axis=1) == run.labels
        result: dict[str, Any] = {"variant": name, **metrics(p, run.labels)}
        result["tokens"] = int(np.median([r["prompt_tokens"] for r in run.records]))
        result["ms"] = float(np.percentile(run.elapsed, 50))
        for lang in LANGS:
            result[lang] = float(correct[[c.lang == lang for c in run.cases]].mean())
        for tag in TAGS:
            mask = np.asarray([tag in c.tags for c in run.cases])
            result[tag] = float(correct[mask].mean()) if mask.any() else float("nan")
        out.append(result)
    return out


def sweep_markdown(
    key: str, splits: list[str], mode: str = "cached", chip: str | None = None
) -> str:
    blocks = []
    for split in splits:
        table = rows(key, split, mode, chip)
        cells = [[f"{r[c]:.0f}" if c == "ms" else num(r[c]) for c in COLUMNS] for r in table]
        blocks.append(f"## {key} {split}\n\n{md(COLUMNS, cells)}")
    return "\n\n".join(blocks)
