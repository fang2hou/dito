"""Regenerate every intent artifact: probe runs, fit reports, heads assets, parity, tables, charts.

Existing probe runs are skipped, so on a machine that already has them this only refits. The heads
assets it writes go to `crates/dito-core/src/intent/heads/`, where they are committed build inputs.
"""

import json
from collections.abc import Callable, Sequence
from pathlib import Path

from .experiments.efficiency import EFFICIENCY_RUNS, efficiency_markdown
from .experiments.efficiency import rows as efficiency_rows
from .experiments.learning_curve import add_learning_curve
from .experiments.order import order_markdown
from .experiments.rounds import round1_path, rounds_markdown
from .experiments.sweep import SWEEP_MODEL, sweep_markdown
from .locations import results_dir
from .probe.runner import run_probes
from .probe.runs import FIT_MODE
from .prompts import FIRST_VARIANT, SHIPPED_VARIANT, VARIANTS
from .report.charts import write_charts
from .report.oof import oof_markdown
from .report.summary import summary_markdown
from .train.export import export_heads
from .train.fit import Fit, fit_variant
from .train.fit_report import write_report
from .train.parity import parity_rows, write_production


def _write(path: Path, text: str, log: Callable[[str], None]) -> None:
    path.write_text(text.strip("\n") + "\n", encoding="utf-8")
    log(f"wrote {path}")


def fit_with_curve(
    key: str,
    variant: str,
    *,
    chip: str | None = None,
    mode: str = FIT_MODE,
    log: Callable[[str], None] = print,
) -> Fit:
    """The fit of a model and variant, with the learning curve its report carries."""
    fit = fit_variant(key, variant, chip=chip, mode=mode, log=log)
    add_learning_curve(fit, chip, mode)
    return fit


def run_all(
    keys: Sequence[str],
    sweep_model: str = SWEEP_MODEL,
    chip: str | None = None,
    log: Callable[[str], None] = print,
) -> None:
    for key in keys:
        run_probes(
            [key],
            [FIRST_VARIANT, SHIPPED_VARIANT],
            ["train", "test"],
            chip=chip,
            layers="all",
            log=log,
        )
        for full, generate in EFFICIENCY_RUNS:
            run_probes(
                [key], [FIRST_VARIANT], ["test"], chip=chip, full=full, generate=generate, log=log
            )
    run_probes([sweep_model], list(VARIANTS), ["train", "test"], chip=chip, log=log)

    for key in keys:
        for variant in (FIRST_VARIANT, SHIPPED_VARIANT):
            fit = fit_with_curve(key, variant, chip=chip, log=log)
            log(f"wrote {write_report(fit, chip)}")
            if variant == SHIPPED_VARIANT:
                log(f"wrote {export_heads(fit, chip)['path']}")
    write_production(parity_rows(keys, chip, log), chip)

    out = results_dir(chip)
    efficiency = efficiency_rows(keys, FIRST_VARIANT, chip)
    _write(out / "summary.md", summary_markdown(keys, chip), log)
    _write(out / "oof_errors.md", oof_markdown(keys, chip=chip), log)
    if all(round1_path(key, chip).exists() for key in keys):
        _write(out / "rounds.md", rounds_markdown(keys, chip), log)
    _write(out / "efficiency.md", efficiency_markdown(efficiency), log)
    (out / "efficiency.json").write_text(json.dumps(efficiency, indent=1) + "\n", encoding="utf-8")
    _write(
        out / f"prompt_sweep.{sweep_model}.md",
        sweep_markdown(sweep_model, ["train", "test"], chip=chip),
        log,
    )
    _write(out / f"option_order.{sweep_model}.md", order_markdown(sweep_model, chip=chip), log)
    for path in write_charts(efficiency, keys, chip):
        log(f"wrote {path}")
