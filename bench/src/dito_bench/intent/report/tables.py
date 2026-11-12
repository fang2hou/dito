"""Compact Markdown tables for the intent reports, and the fit report as text."""

from collections.abc import Sequence
from typing import Any

from ..data import INTENTS, TAGS
from ..train.fit_report import intent_key


def md(columns: Sequence[str], rows: Sequence[Sequence[object]]) -> str:
    lines = ["| " + " | ".join(columns) + " |", "|" + "---|" * len(columns)]
    lines += ["| " + " | ".join(str(cell) for cell in row) + " |" for row in rows]
    return "\n".join(lines)


def num(value: object) -> str:
    """Floats with three decimals, anything else as it is, a missing value as nothing."""
    if value is None:
        return ""
    return f"{value:.3f}" if isinstance(value, float) else str(value)


def pct(value: float | None) -> str:
    return "n/a" if value is None else f"{value * 100:.1f}"


FIT_COLUMNS = [
    "method",
    "oof_nll",
    "acc",
    "f1",
    "ece",
    "auto",
    "auto_acc",
    "auto_wrong",
    "won",
    "lost",
    "p",
    "zh",
    "ja",
    "en",
    *TAGS,
    *(intent_key(i) for i in INTENTS),
]


def fit_markdown(report: dict[str, Any]) -> str:
    """The tables of one fit report: methods, layer scan, slot heads and learning curve."""
    chosen = report["chosen"]
    title = f"{report['model']} {report['variant']}: train {report['train']}, test {report['test']}"
    out = [
        f"## {title}\n",
        md(FIT_COLUMNS, [[num(r.get(c)) for c in FIT_COLUMNS] for r in report["rows"]]),
        "\nlayer scan (oof = out-of-fold on the training split; * = chosen by out-of-fold loss)\n",
    ]
    scan = report["scan"]
    layers = sorted({s["layer"] for s in scan})
    rows = []
    for layer in layers:
        cells = [
            next(s for s in scan if s["layer"] == layer and s["pool"] == p)
            for p in ("last", "span")
        ]
        mark = "*" if layer == chosen["layer"] else ""
        rows.append(
            [f"{layer}{mark}"] + [num(v) for c in cells for v in (c["oof_acc"], c["test_acc"])]
        )
    columns = ["layer", "last oof acc", "last test acc", "span oof acc", "span test acc"]
    out.append(md(columns, rows))
    out.append("\nslot heads on the same hidden state\n")
    slot_columns = ["task", "classes", "train", "test", "layer", "pool", "majority", "acc", "ece"]
    out.append(md(slot_columns, [[num(s[c]) for c in slot_columns] for s in report["slots"]]))
    out.append("\nlearning curve (head on last + span of the chosen layer)\n")
    curve_columns = ["train", "acc", "auto", "auto_acc"]
    out.append(
        md(
            ["training cases", *curve_columns[1:]],
            [[num(c[k]) for k in curve_columns] for c in report["learning_curve"]],
        )
    )
    return "\n".join(out)
