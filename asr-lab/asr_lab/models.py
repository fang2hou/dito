"""Uniform adapters over the local ASR runtimes.

Each adapter lazily loads its runtime and weights on first use and keeps them
resident, so per-call latency reflects inference rather than model loading.
`load_model` caches instances per key for the life of the process.
"""

from __future__ import annotations

import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import soundfile as sf

from .paths import MODELS_DIR

SENSEVOICE_REPO = "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17"
SENSEVOICE_DIR = MODELS_DIR / "sensevoice"


@dataclass
class Transcription:
    model: str
    audio_path: Path
    audio_s: float
    elapsed_s: float
    text: str
    language: str | None = None

    @property
    def rtf(self) -> float:
        """Audio seconds processed per compute second; higher is faster."""
        return self.audio_s / self.elapsed_s if self.elapsed_s > 0 else float("inf")


def _audio_seconds(audio: Path | str) -> float:
    info = sf.info(str(audio))
    return info.frames / info.samplerate


class SenseVoiceSherpa:
    """SenseVoice-Small int8 through sherpa-onnx — the engine dito ships today (CPU)."""

    name = "sensevoice-small-int8 · sherpa-onnx CPU"

    def __init__(self) -> None:
        import sherpa_onnx

        model, tokens = _ensure_sensevoice_files()
        self._rec = sherpa_onnx.OfflineRecognizer.from_sense_voice(
            model=str(model),
            tokens=str(tokens),
            num_threads=4,
            language="auto",
            use_itn=True,
        )

    def transcribe(self, audio: Path | str) -> Transcription:
        samples, sr = sf.read(str(audio), dtype="float32")
        if sr != 16_000:
            raise ValueError(f"{audio}: expected 16 kHz audio, got {sr} Hz")
        stream = self._rec.create_stream()
        stream.accept_waveform(16_000, samples)
        t0 = time.perf_counter()
        self._rec.decode_stream(stream)
        elapsed = time.perf_counter() - t0
        result = stream.result
        lang = (getattr(result, "lang", "") or "").strip("<|>") or None
        return Transcription(
            model=self.name,
            audio_path=Path(audio),
            audio_s=len(samples) / 16_000,
            elapsed_s=elapsed,
            text=result.text.strip(),
            language=lang,
        )


class WhisperMlx:
    """Whisper through mlx-whisper; ModelHolder keeps weights resident per repo."""

    def __init__(self, repo: str = "mlx-community/whisper-large-v3-turbo") -> None:
        self._repo = repo
        self.name = f"{repo.split('/')[-1]} · MLX"

    def transcribe(
        self, audio: Path | str, initial_prompt: str | None = None
    ) -> Transcription:
        import mlx_whisper

        # Pass a float32 ndarray instead of a path: mlx-whisper's file loader
        # shells out to ffmpeg, which we do not want to require for 16 kHz WAVs.
        samples, sr = sf.read(str(audio), dtype="float32", always_2d=True)
        if samples.shape[1] > 1:
            samples = samples.mean(axis=1)
        else:
            samples = samples[:, 0]
        if sr != 16_000:
            target = round(len(samples) * 16_000 / sr)
            xp = np.linspace(0.0, len(samples) - 1, target)
            samples = np.interp(xp, np.arange(len(samples)), samples).astype("float32")
        kwargs = {"initial_prompt": initial_prompt} if initial_prompt else {}
        t0 = time.perf_counter()
        out = mlx_whisper.transcribe(
            samples, path_or_hf_repo=self._repo, verbose=None, **kwargs
        )
        elapsed = time.perf_counter() - t0
        return Transcription(
            model=self.name,
            audio_path=Path(audio),
            audio_s=_audio_seconds(audio),
            elapsed_s=elapsed,
            text=out["text"].strip(),
            language=out.get("language"),
        )


class Qwen3AsrMlx:
    """Qwen3-ASR through the native MLX reimplementation (moona3k/mlx-qwen3-asr)."""

    def __init__(self, model: str = "Qwen/Qwen3-ASR-0.6B") -> None:
        from mlx_qwen3_asr import Session

        self._session = Session(model=model)
        self.name = f"{model.split('/')[-1].lower()} · MLX"

    def transcribe(
        self, audio: Path | str, language: str | None = None, context: str | None = None
    ) -> Transcription:
        """`context` = space-separated hotwords, injected as the system prompt."""
        t0 = time.perf_counter()
        result = self._session.transcribe(
            str(audio), language=language, context=context or ""
        )
        elapsed = time.perf_counter() - t0
        return Transcription(
            model=self.name,
            audio_path=Path(audio),
            audio_s=_audio_seconds(audio),
            elapsed_s=elapsed,
            text=result.text.strip(),
            language=result.language,
        )


MODELS: dict[str, Callable[[], object]] = {
    "sensevoice": SenseVoiceSherpa,
    "whisper-turbo": lambda: WhisperMlx("mlx-community/whisper-large-v3-turbo"),
    "whisper-large-v3": lambda: WhisperMlx("mlx-community/whisper-large-v3"),
    "qwen3-asr-0.6b": lambda: Qwen3AsrMlx("Qwen/Qwen3-ASR-0.6B"),
    "qwen3-asr-1.7b": lambda: Qwen3AsrMlx("Qwen/Qwen3-ASR-1.7B"),
    # Qwen3-ASR streaming fine-tune (netease-youdao, NetEase license) — local weights.
    "r2t2": lambda: Qwen3AsrMlx(str(MODELS_DIR / "r2t2")),
}

# Order shown in the benchmark table.
DEFAULT_BENCHMARK = ["sensevoice", "whisper-turbo", "qwen3-asr-0.6b", "qwen3-asr-1.7b"]

_cache: dict[str, object] = {}


def load_model(key: str):
    try:
        factory = MODELS[key]
    except KeyError:
        raise KeyError(f"unknown model {key!r}; available: {sorted(MODELS)}") from None
    if key not in _cache:
        _cache[key] = factory()
    return _cache[key]


def _ensure_sensevoice_files() -> tuple[Path, Path]:
    from huggingface_hub import snapshot_download

    SENSEVOICE_DIR.mkdir(parents=True, exist_ok=True)
    snapshot_download(
        repo_id=SENSEVOICE_REPO,
        allow_patterns=["model.int8.onnx", "tokens.txt"],
        local_dir=SENSEVOICE_DIR,
    )
    model, tokens = SENSEVOICE_DIR / "model.int8.onnx", SENSEVOICE_DIR / "tokens.txt"
    if not model.exists() or not tokens.exists():
        raise FileNotFoundError(f"SenseVoice files missing under {SENSEVOICE_DIR}")
    return model, tokens
