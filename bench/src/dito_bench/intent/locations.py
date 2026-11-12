"""Where the intent bench keeps what it computes: the local cache and the committed results."""

from functools import cache
from pathlib import Path

from .. import env
from ..paths import CACHE_DIR, RESULTS_DIR


@cache
def chip_slug(chip: str | None = None) -> str:
    return env.chip_slug(chip)


def cache_dir(chip: str | None = None) -> Path:
    """Probe runs: local only, git-ignored, rebuilt from the models when missing."""
    return CACHE_DIR / "intent" / chip_slug(chip)


def results_dir(chip: str | None = None) -> Path:
    """Fit reports, tables and charts: committed."""
    return RESULTS_DIR / "intent" / chip_slug(chip)
