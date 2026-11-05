"""The interface every speech recognition adapter implements."""

from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import ClassVar, Protocol


@dataclass(frozen=True, slots=True)
class Transcription:
    text: str
    language: str | None
    audio_s: float
    elapsed_s: float

    @property
    def realtime_factor(self) -> float:
        """Audio seconds processed per compute second; higher is faster."""
        return self.audio_s / self.elapsed_s if self.elapsed_s > 0 else float("inf")


class AsrModel(Protocol):
    """A loaded model. Adapters load lazily and stay resident, so timings exclude loading."""

    supports_hotwords: ClassVar[bool]

    def transcribe(self, audio: Path, hotwords: Sequence[str] = ()) -> Transcription: ...
