"""The ASR models the bench knows about. To add one, write an adapter and add a spec."""

from collections.abc import Callable
from dataclasses import dataclass

from ..paths import MODELS_DIR
from ..weights import Weights
from .base import AsrModel


def _sensevoice(weights: Weights) -> AsrModel:
    from .sensevoice import SenseVoiceSherpa

    return SenseVoiceSherpa(weights)


def _whisper(weights: Weights) -> AsrModel:
    from .whisper import WhisperMlx

    return WhisperMlx(weights)


def _qwen3(weights: Weights) -> AsrModel:
    from .qwen3 import Qwen3AsrMlx

    return Qwen3AsrMlx(weights)


@dataclass(frozen=True, slots=True)
class AsrSpec:
    key: str
    label: str
    runtime: str
    weights: Weights
    load: Callable[[Weights], AsrModel]
    packages: tuple[str, ...]
    supports_hotwords: bool = True
    in_default_set: bool = True
    note: str = ""


_SHERPA = ("sherpa-onnx",)
_WHISPER = ("mlx", "mlx-whisper")
_QWEN3 = ("mlx", "mlx-qwen3-asr")

ASR_MODELS: dict[str, AsrSpec] = {
    spec.key: spec
    for spec in (
        AsrSpec(
            "sensevoice",
            "SenseVoice-Small int8",
            "sherpa-onnx CPU",
            Weights(
                "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17",
                MODELS_DIR / "sensevoice",
                ("model.int8.onnx", "tokens.txt"),
            ),
            _sensevoice,
            _SHERPA,
            supports_hotwords=False,
            note="The engine Dito ships today. zh, en, ja, ko, yue.",
        ),
        AsrSpec(
            "whisper-turbo",
            "Whisper large-v3-turbo",
            "MLX",
            Weights("mlx-community/whisper-large-v3-turbo"),
            _whisper,
            _WHISPER,
            note="Multilingual baseline.",
        ),
        AsrSpec(
            "whisper-large-v3",
            "Whisper large-v3",
            "MLX",
            Weights("mlx-community/whisper-large-v3"),
            _whisper,
            _WHISPER,
            in_default_set=False,
            note="More accurate baseline, slower.",
        ),
        AsrSpec(
            "qwen3-asr-0.6b",
            "Qwen3-ASR 0.6B",
            "MLX",
            Weights("Qwen/Qwen3-ASR-0.6B"),
            _qwen3,
            _QWEN3,
            note="Fast tier. 30 languages and 22 Chinese dialects.",
        ),
        AsrSpec(
            "qwen3-asr-1.7b",
            "Qwen3-ASR 1.7B",
            "MLX",
            Weights("Qwen/Qwen3-ASR-1.7B"),
            _qwen3,
            _QWEN3,
            note="Accuracy tier.",
        ),
        AsrSpec(
            "r2t2",
            "Confucius4-R2T2",
            "MLX",
            Weights("netease-youdao/Confucius4-R2T2", MODELS_DIR / "r2t2"),
            _qwen3,
            _QWEN3,
            in_default_set=False,
            note=(
                "Streaming fine-tune of Qwen3-ASR-1.7B under the NetEase Youdao model license. "
                "Read MODEL_LICENSE in the repository before use."
            ),
        ),
    )
}


def get_asr_spec(key: str) -> AsrSpec:
    try:
        return ASR_MODELS[key]
    except KeyError:
        msg = f"unknown ASR model {key!r}; available: {', '.join(sorted(ASR_MODELS))}"
        raise KeyError(msg) from None


def default_asr_keys() -> list[str]:
    return [key for key, spec in ASR_MODELS.items() if spec.in_default_set]
