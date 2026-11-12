"""The stored fit report of a model and prompt variant, which the tables and charts read.

It lives in `results/intent/<chip>/<model>.<variant>.json` and is committed.
"""

import json
import math
from pathlib import Path
from typing import TYPE_CHECKING, Any

from ..locations import results_dir

if TYPE_CHECKING:
    from .fit import Fit

type Json = bool | int | float | str | list[Json] | dict[str, Json] | None


def intent_key(intent: str) -> str:
    """The report column of a gold intent's accuracy."""
    return f"intent:{intent}"


def report_path(key: str, variant: str, chip: str | None = None) -> Path:
    return results_dir(chip) / f"{key}.{variant}.json"


def _finite(value: Json) -> Json:
    """NaN (an empty share, such as auto accuracy with no case past the gate) as JSON null."""
    if isinstance(value, float) and math.isnan(value):
        return None
    if isinstance(value, dict):
        return {k: _finite(v) for k, v in value.items()}
    if isinstance(value, list):
        return [_finite(v) for v in value]
    return value


def write_report(fit: Fit, chip: str | None = None) -> Path:
    path = report_path(fit.key, fit.variant, chip)
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(_finite(fit.report), ensure_ascii=False, indent=1, allow_nan=False)
    path.write_text(text, encoding="utf-8")
    return path


def read_report(key: str, variant: str, chip: str | None = None) -> dict[str, Any]:
    path = report_path(key, variant, chip)
    if not path.exists():
        msg = f"{path} is missing; run `intent fit -m {key}` first"
        raise FileNotFoundError(msg)
    return json.loads(path.read_text(encoding="utf-8"))


def pick(report: dict[str, Any], prefix: str) -> dict[str, Any]:
    """The first row of a report whose method starts with `prefix`."""
    return next(r for r in report["rows"] if r["method"].startswith(prefix))
