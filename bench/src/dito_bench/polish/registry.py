"""The polish models the bench knows about. To add one, add a spec."""

from collections.abc import Callable
from dataclasses import dataclass

from ..paths import MODELS_DIR
from ..weights import Weights
from .base import Polisher

POLISH_DIR = MODELS_DIR / "polish"


def _llama(weights: Weights) -> Polisher:
    from .llama import LlamaPolisher

    return LlamaPolisher(weights)


def _mlx(weights: Weights) -> Polisher:
    from .mlx import MlxPolisher

    return MlxPolisher(weights)


@dataclass(frozen=True, slots=True)
class PolishSpec:
    key: str
    label: str
    runtime: str
    weights: Weights
    load: Callable[[Weights], Polisher]
    packages: tuple[str, ...]
    in_default_set: bool = True


def _gguf(key: str, label: str, repo: str, quant: str, *, default: bool = True) -> PolishSpec:
    return PolishSpec(
        key,
        label,
        "llama.cpp Metal",
        Weights(repo, POLISH_DIR / key, (f"*{quant}*",)),
        _llama,
        ("llama-cpp-python",),
        default,
    )


def _mlx_4bit(key: str, label: str, repo: str, *, default: bool = False) -> PolishSpec:
    return PolishSpec(
        key, label, "MLX 4-bit", Weights(repo, POLISH_DIR / key), _mlx, ("mlx", "mlx-lm"), default
    )


POLISH_MODELS: dict[str, PolishSpec] = {
    spec.key: spec
    for spec in (
        _gguf(
            "qwen3-1.7b", "Qwen3 1.7B Q4_K_M", "unsloth/Qwen3-1.7B-GGUF", "Q4_K_M", default=False
        ),
        _gguf(
            "qwen3-4b",
            "Qwen3 4B Instruct 2507 Q4_K_M",
            "unsloth/Qwen3-4B-Instruct-2507-GGUF",
            "Q4_K_M",
        ),
        _gguf("qwen3.5-2b", "Qwen3.5 2B Q4_K_M", "unsloth/Qwen3.5-2B-GGUF", "Q4_K_M"),
        _gguf("qwen3.5-4b", "Qwen3.5 4B Q4_K_M", "unsloth/Qwen3.5-4B-GGUF", "Q4_K_M"),
        _gguf(
            "qwen3.5-9b", "Qwen3.5 9B Q4_K_M", "unsloth/Qwen3.5-9B-GGUF", "Q4_K_M", default=False
        ),
        _gguf(
            "qwen3.5-9b-q3", "Qwen3.5 9B Q3_K_M", "unsloth/Qwen3.5-9B-GGUF", "Q3_K_M", default=False
        ),
        _mlx_4bit("qwen3.5-4b-mlx", "Qwen3.5 4B", "mlx-community/Qwen3.5-4B-MLX-4bit"),
        _mlx_4bit("qwen3.5-9b-mlx", "Qwen3.5 9B", "mlx-community/Qwen3.5-9B-MLX-4bit"),
        _mlx_4bit("qwen3.8-27b-mlx", "Qwen3.8 27B", "mlx-community/Qwen3.8-27B-4bit"),
    )
}


def get_polish_spec(key: str) -> PolishSpec:
    try:
        return POLISH_MODELS[key]
    except KeyError:
        msg = f"unknown polish model {key!r}; available: {', '.join(sorted(POLISH_MODELS))}"
        raise KeyError(msg) from None


def default_polish_keys() -> list[str]:
    return [key for key, spec in POLISH_MODELS.items() if spec.in_default_set]
