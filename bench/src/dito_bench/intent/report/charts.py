"""The charts of the intent report, as SVG files in the results folder.

Needs matplotlib (the `charts` extra); it is imported when a chart is drawn.
"""

from collections.abc import Sequence
from pathlib import Path
from types import ModuleType
from typing import TYPE_CHECKING, Any

from ... import env

if TYPE_CHECKING:
    from matplotlib.figure import Figure

from ..data import INTENTS, load_cases
from ..locations import results_dir
from ..models import MODEL_KEYS, short_name
from ..prompts import FIRST_VARIANT, SHIPPED_VARIANT
from ..train.fit_report import intent_key, pick, read_report

PALETTE = ("#4C78A8", "#F58518", "#54A24B")
LATENCY_PATHS = (
    ("generate JSON, whole prompt", "Generate JSON,\nthen parse"),
    ("generate a letter, whole prompt", "Generate one letter"),
    ("readout, whole prompt", "Read logits,\nwhole prompt"),
    ("readout, resident prefix", "Read logits,\nresident prefix"),
)


def _plt() -> ModuleType:
    try:
        import matplotlib
    except ImportError as error:
        msg = "charts need matplotlib: run `uv sync --extra charts` in bench/"
        raise SystemExit(msg) from error
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    plt.rcParams.update(
        {
            "font.size": 11,
            "axes.spines.top": False,
            "axes.spines.right": False,
            # Fixed ids and no date keep the SVG files identical between runs.
            "svg.hashsalt": "dito-intent",
        }
    )
    return plt


def _save(fig: Figure, path: Path) -> Path:
    fig.tight_layout()
    fig.savefig(path, metadata={"Date": None})
    return path


def _name(key: str) -> str:
    return f"Qwen3.5 {short_name(key).upper()}"


def accuracy_chart(models: Sequence[str], chip: str | None, out: Path) -> Path:
    plt = _plt()
    stages = (
        ("Direct readout,\nfirst prompt", FIRST_VARIANT, "direct"),
        ("Direct readout,\nbuilt prompt", SHIPPED_VARIANT, "direct"),
        ("Head,\nfirst prompt", FIRST_VARIANT, "SELECTED"),
        ("Head,\nbuilt prompt", SHIPPED_VARIANT, "SELECTED"),
    )
    fig, ax = plt.subplots(figsize=(9, 4.2))
    width = 0.26
    for i, key in enumerate(models):
        values = [pick(read_report(key, v, chip), p)["acc"] * 100 for _, v, p in stages]
        positions = [x + (i - (len(models) - 1) / 2) * width for x in range(len(stages))]
        bars = ax.bar(positions, values, width, label=_name(key), color=PALETTE[i])
        ax.bar_label(bars, fmt="%.1f", fontsize=9, padding=2)
    ax.set_xticks(range(len(stages)), [s[0] for s in stages])
    ax.set_ylim(40, 102)
    ax.set_ylabel(f"Test accuracy (%), {len(load_cases())} cases")
    ax.legend(frameon=False, ncols=3, loc="upper left")
    return _save(fig, out / "accuracy.svg")


def layers_chart(models: Sequence[str], chip: str | None, out: Path) -> Path:
    plt = _plt()
    fig, axes = plt.subplots(1, len(models), figsize=(12, 3.6), sharey=True, squeeze=False)
    for ax, key, color in zip(axes[0], models, PALETTE, strict=False):
        report = read_report(key, FIRST_VARIANT, chip)
        for pool, style in (("last", "-"), ("span", "--")):
            points = sorted(
                (s["layer"], s["test_acc"] * 100) for s in report["scan"] if s["pool"] == pool
            )
            label = "answer position" if pool == "last" else "mean over the instruction"
            ax.plot(*zip(*points, strict=True), style, color=color, label=label)
        ax.axhline(pick(report, "direct")["acc"] * 100, color="gray", linewidth=0.8, linestyle=":")
        ax.set_title(_name(key))
        ax.set_xlabel("Layer whose input the head reads")
    axes[0][0].set_ylabel("Test accuracy (%)")
    axes[0][0].set_ylim(20, 100)
    axes[0][0].legend(frameon=False, fontsize=9, loc="lower right")
    return _save(fig, out / "layers.svg")


def curve_chart(models: Sequence[str], chip: str | None, out: Path) -> Path:
    plt = _plt()
    fig, ax = plt.subplots(figsize=(6, 3.6))
    for key, color in zip(models, PALETTE, strict=False):
        curve = read_report(key, FIRST_VARIANT, chip)["learning_curve"]
        sizes, accuracy = [c["train"] for c in curve], [c["acc"] * 100 for c in curve]
        ax.plot(sizes, accuracy, "o-", color=color, label=_name(key))
    ax.set_xscale("log")
    sizes = [c["train"] for c in read_report(models[0], FIRST_VARIANT, chip)["learning_curve"]]
    ax.set_xticks(sizes, [str(n) for n in sizes])
    ax.set_xlabel("Training cases")
    ax.set_ylabel("Test accuracy (%)")
    ax.legend(frameon=False)
    return _save(fig, out / "learning_curve.svg")


def intents_chart(models: Sequence[str], chip: str | None, out: Path) -> Path:
    plt = _plt()
    fig, ax = plt.subplots(figsize=(10, 4))
    width = 0.26
    for i, key in enumerate(models):
        row = pick(read_report(key, SHIPPED_VARIANT, chip), "SELECTED")
        values = [row[intent_key(intent)] * 100 for intent in INTENTS]
        positions = [x + (i - (len(models) - 1) / 2) * width for x in range(len(INTENTS))]
        ax.bar(positions, values, width, label=_name(key), color=PALETTE[i])
    ax.set_xticks(range(len(INTENTS)), INTENTS)
    ax.set_ylim(40, 102)
    ax.set_ylabel("Test accuracy (%) by gold intent")
    ax.legend(frameon=False, ncols=3, loc="lower right")
    return _save(fig, out / "intents.svg")


def latency_chart(
    models: Sequence[str], efficiency: list[dict[str, Any]], chip: str | None, out: Path
) -> Path:
    plt = _plt()
    fig, ax = plt.subplots(figsize=(8, 3.8))
    width = 0.26
    for i, key in enumerate(models):
        values = [
            next((r["p50_ms"] for r in efficiency if r["model"] == key and r["path"] == path), 0)
            for path, _ in LATENCY_PATHS
        ]
        positions = [y + ((len(models) - 1) / 2 - i) * width for y in range(len(LATENCY_PATHS))]
        bars = ax.barh(positions, values, width, label=_name(key), color=PALETTE[i])
        ax.bar_label(bars, fmt="%.0f", fontsize=9, padding=2)
    ax.set_yticks(range(len(LATENCY_PATHS)), [label for _, label in LATENCY_PATHS])
    ax.invert_yaxis()
    ax.set_xlabel(f"Median time per decision (ms), {chip or env.chip()}")
    ax.legend(frameon=False)
    return _save(fig, out / "latency.svg")


def write_charts(
    efficiency: list[dict[str, Any]],
    models: Sequence[str] = MODEL_KEYS,
    chip: str | None = None,
) -> list[Path]:
    """Draw the five charts into the results folder and return their paths."""
    out = results_dir(chip)
    out.mkdir(parents=True, exist_ok=True)
    return [
        accuracy_chart(models, chip, out),
        layers_chart(models, chip, out),
        curve_chart(models, chip, out),
        intents_chart(models, chip, out),
        latency_chart(models, efficiency, chip, out),
    ]
