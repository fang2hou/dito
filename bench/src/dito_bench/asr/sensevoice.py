"""SenseVoice-Small int8 through sherpa-onnx, the engine Dito ships (CPU)."""

import time
from collections.abc import Sequence
from pathlib import Path
from typing import ClassVar

from ..audio import SAMPLE_RATE, load_16k_mono
from ..weights import Weights
from .base import Transcription


class SenseVoiceSherpa:
    supports_hotwords: ClassVar[bool] = False

    def __init__(self, weights: Weights) -> None:
        import sherpa_onnx

        directory = weights.fetch()
        self._recognizer = sherpa_onnx.OfflineRecognizer.from_sense_voice(
            model=str(directory / "model.int8.onnx"),
            tokens=str(directory / "tokens.txt"),
            num_threads=4,
            language="auto",
            use_itn=True,
        )

    def transcribe(self, audio: Path, hotwords: Sequence[str] = ()) -> Transcription:
        # sherpa-onnx has no decoding-level hotword biasing; its `hr_dict` is a post-processing
        # replacement, so hotwords are ignored (see `supports_hotwords`).
        samples = load_16k_mono(audio)
        stream = self._recognizer.create_stream()
        stream.accept_waveform(SAMPLE_RATE, samples)
        start = time.perf_counter()
        self._recognizer.decode_stream(stream)
        elapsed = time.perf_counter() - start
        result = stream.result
        language = (getattr(result, "lang", "") or "").strip("<|>") or None
        return Transcription(
            text=result.text.strip(),
            language=language,
            audio_s=len(samples) / SAMPLE_RATE,
            elapsed_s=elapsed,
        )
