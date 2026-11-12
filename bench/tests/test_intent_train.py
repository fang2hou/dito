from pathlib import Path

import numpy as np

from dito_bench.intent.data import INTENTS, OPS
from dito_bench.intent.prompts import ROUTER_SYSTEM_PATH, SHIPPED_VARIANT, VARIANTS
from dito_bench.intent.train.asset import (
    DirectMix,
    HeadsAsset,
    NamedHead,
    read_heads,
    write_heads,
)
from dito_bench.intent.train.heads import Projected


def test_shipped_prompt_is_the_text_the_app_includes() -> None:
    built = VARIANTS[SHIPPED_VARIANT].build("x")["system"]
    assert built == ROUTER_SYSTEM_PATH.read_text(encoding="utf-8")


def test_heads_asset_round_trips(tmp_path: Path) -> None:
    rng = np.random.default_rng(0)
    n_embd, n = 32, 120
    x = rng.normal(size=(n, 2 * n_embd))
    y = np.arange(n) % len(INTENTS)
    x[:, : len(INTENTS)] += np.eye(len(INTENTS))[y] * 3
    fitted = Projected(x, y, [f"g{label}" for label in y], len(INTENTS))
    slot_y = np.arange(n) % len(OPS)
    slot = Projected(x, slot_y, [f"g{label}" for label in slot_y], len(OPS))
    asset = HeadsAsset(
        "qwen3.5-4b-instruct",
        "a" * 64,
        "b" * 64,
        DirectMix(0.8, 1.25),
        [
            NamedHead("intent", INTENTS, [(3, "last"), (4, "span")], fitted.folded()),
            NamedHead("rewrite.op", OPS, [(5, "span")], slot.folded()),
        ],
    )
    path = tmp_path / "model.heads"
    write_heads(path, asset)
    back = read_heads(path)

    assert (back.model, back.gguf_sha256, back.prompt_sha256) == (
        asset.model,
        asset.gguf_sha256,
        asset.prompt_sha256,
    )
    assert back.direct == asset.direct
    for want, got in zip(asset.heads, back.heads, strict=True):
        assert (got.name, got.labels, got.parts) == (want.name, want.labels, want.parts)
        assert got.head.temperature == want.head.temperature
        for field in ("mean", "scale", "weights", "bias"):
            expected = getattr(want.head, field).astype(np.float32)
            assert np.array_equal(getattr(got.head, field).astype(np.float32), expected)
    features = rng.normal(size=(50, 2 * n_embd))
    assert np.allclose(
        back.head("intent").head.probabilities(features), fitted.probabilities(features), atol=1e-4
    )
