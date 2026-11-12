"""Write the `.heads` asset the app bundles from a fit on the shipped prompt variant."""

import json
from pathlib import Path
from typing import Any

from ..models import catalog_id
from ..probe.runs import FIT_MODE, run_prefix
from ..prompts import SHIPPED_VARIANT, prompt_sha256
from .asset import HEADS_DIR, HeadsAsset, sha256_file, write_heads
from .fit import Fit


def export_heads(fit: Fit, chip: str | None = None) -> dict[str, Any]:
    """Write `<catalog id>.heads`. The GGUF hashed is the file the fitted run's meta names.

    Returns the path, the size in bytes, the intent head's parts and the direct mix weight (None
    when the intent head stands alone).
    """
    if fit.variant != SHIPPED_VARIANT:
        msg = f"only the shipped variant {SHIPPED_VARIANT} is exported, not {fit.variant}"
        raise ValueError(msg)
    meta_path = run_prefix(fit.key, fit.variant, FIT_MODE, "test", chip).with_suffix(".meta.json")
    gguf = Path(json.loads(meta_path.read_text(encoding="utf-8"))["model"])
    if not gguf.is_file():
        msg = f"{gguf}, the model of the run behind this fit, is not on this machine"
        raise FileNotFoundError(msg)
    asset = HeadsAsset(
        catalog_id(fit.key),
        sha256_file(gguf),
        prompt_sha256(),
        fit.direct,
        [fit.intent, *fit.slots],
    )
    path = HEADS_DIR / f"{asset.model}.heads"
    size = write_heads(path, asset)
    return {
        "path": path,
        "bytes": size,
        "parts": [list(p) for p in fit.intent.parts],
        "direct_weight": None if fit.direct is None else fit.direct.weight,
    }
