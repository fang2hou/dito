"""Synthetic benchmark clips.

The clip definitions and their ground truth live in `data/clips.json`. The audio is
synthesized with the macOS `say` command, so the whole benchmark runs without a microphone.
"""

import json
import subprocess
from dataclasses import dataclass
from pathlib import Path

from .audio import SAMPLE_RATE
from .paths import AUDIO_DIR, DATA_DIR


@dataclass(frozen=True, slots=True)
class Clip:
    name: str
    voice: str
    lang: str
    text: str
    hotwords: tuple[str, ...] = ()

    @property
    def path(self) -> Path:
        return AUDIO_DIR / f"{self.name}.wav"


def load_clips() -> list[Clip]:
    raw = json.loads((DATA_DIR / "clips.json").read_text(encoding="utf-8"))
    return [
        Clip(
            name=item["name"],
            voice=item["voice"],
            lang=item["lang"],
            text=item["text"],
            hotwords=tuple(item.get("hotwords", ())),
        )
        for item in raw
    ]


def synthesize(clip: Clip) -> None:
    subprocess.run(
        [
            "say",
            "-v",
            clip.voice,
            "-o",
            str(clip.path),
            f"--data-format=LEI16@{SAMPLE_RATE}",
            clip.text,
        ],
        check=True,
        capture_output=True,
        text=True,
    )


def ensure_clips(*, force: bool = False) -> list[Clip]:
    """Synthesize any missing clip audio (or all of it with `force`) and return the clips."""
    AUDIO_DIR.mkdir(parents=True, exist_ok=True)
    clips = load_clips()
    for clip in clips:
        if force or not clip.path.exists():
            synthesize(clip)
    return clips
