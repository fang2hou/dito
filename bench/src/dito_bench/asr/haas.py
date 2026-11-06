"""Community Haas-ja mixed INT4/INT8 encoder through sherpa-onnx (CPU)."""

import time
from collections.abc import Sequence
from pathlib import Path
from typing import ClassVar

from ..audio import SAMPLE_RATE, load_16k_mono
from ..weights import Weights
from .base import Transcription


class HaasJaSherpa:
    supports_hotwords: ClassVar[bool] = False

    def __init__(self, weights: Weights, *, num_threads: int = 4) -> None:
        import sherpa_onnx

        directory = weights.fetch()
        # The conversion packs KDA state into the existing NeMo TDT interface.
        self._recognizer = sherpa_onnx.OfflineRecognizer.from_transducer(
            encoder=str(directory / "encoder.quantized.onnx"),
            decoder=str(directory / "decoder.onnx"),
            joiner=str(directory / "joiner.onnx"),
            tokens=str(directory / "tokens.txt"),
            num_threads=num_threads,
            sample_rate=SAMPLE_RATE,
            feature_dim=128,
            model_type="nemo_transducer",
            provider="cpu",
        )

    def transcribe(self, audio: Path, hotwords: Sequence[str] = ()) -> Transcription:
        samples = load_16k_mono(audio)
        stream = self._recognizer.create_stream()
        stream.accept_waveform(SAMPLE_RATE, samples)
        start = time.perf_counter()
        self._recognizer.decode_stream(stream)
        elapsed = time.perf_counter() - start
        return Transcription(
            text=stream.result.text.strip(),
            language="ja",
            audio_s=len(samples) / SAMPLE_RATE,
            elapsed_s=elapsed,
        )
