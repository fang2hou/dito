"""Qwen3-ASR through the native MLX reimplementation (mlx-qwen3-asr)."""

import time
from collections.abc import Sequence
from pathlib import Path
from typing import ClassVar

from ..audio import duration_s
from ..weights import Weights
from .base import Transcription


class Qwen3AsrMlx:
    supports_hotwords: ClassVar[bool] = True

    def __init__(self, weights: Weights) -> None:
        from mlx_qwen3_asr import Session

        self._session = Session(model=weights.ref)

    def transcribe(
        self, audio: Path, hotwords: Sequence[str] = (), language: str | None = None
    ) -> Transcription:
        """Hotwords are injected as the system prompt context (decoding-level biasing)."""
        start = time.perf_counter()
        result = self._session.transcribe(str(audio), language=language, context=" ".join(hotwords))
        elapsed = time.perf_counter() - start
        return Transcription(
            text=result.text.strip(),
            language=result.language,
            audio_s=duration_s(audio),
            elapsed_s=elapsed,
        )
