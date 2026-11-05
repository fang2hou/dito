"""Whisper through mlx-whisper (Apple GPU)."""

import time
from collections.abc import Sequence
from pathlib import Path
from typing import ClassVar

from ..audio import SAMPLE_RATE, load_16k_mono
from ..weights import Weights
from .base import Transcription


class WhisperMlx:
    supports_hotwords: ClassVar[bool] = True

    def __init__(self, weights: Weights) -> None:
        self._repo = weights.ref

    def transcribe(self, audio: Path, hotwords: Sequence[str] = ()) -> Transcription:
        """Hotwords become `initial_prompt`, a soft bias rather than decoding-level biasing."""
        import mlx_whisper

        # Pass samples instead of a path: mlx-whisper's file loader shells out to ffmpeg.
        samples = load_16k_mono(audio)
        initial_prompt = " ".join(hotwords) or None
        start = time.perf_counter()
        out = mlx_whisper.transcribe(
            samples, path_or_hf_repo=self._repo, verbose=None, initial_prompt=initial_prompt
        )
        elapsed = time.perf_counter() - start
        return Transcription(
            text=out["text"].strip(),
            language=out.get("language"),
            audio_s=len(samples) / SAMPLE_RATE,
            elapsed_s=elapsed,
        )
