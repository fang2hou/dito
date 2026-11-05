"""Microphone recording helpers for the notebooks.

Everything is captured as 16 kHz mono float32 and saved under `recordings/` as a WAV file
plus a JSON sidecar with the capture metadata. Recordings are personal data: the directory
is git-ignored and results from recordings are never stored.
"""

import datetime as dt
import json
from pathlib import Path

import numpy as np
import sounddevice as sd
import soundfile as sf

from .audio import SAMPLE_RATE
from .paths import RECORDINGS_DIR


def list_input_devices() -> str:
    """One line per capture device, marking the system default."""
    default_input = sd.default.device[0]
    lines = [
        f"[{i}] {d['name']} ({d['max_input_channels']} in)"
        + ("  <- default" if i == default_input else "")
        for i, d in enumerate(sd.query_devices())
        if d["max_input_channels"] > 0
    ]
    return "\n".join(lines) or "No input device found."


def _save(samples: np.ndarray, note: str) -> Path:
    RECORDINGS_DIR.mkdir(parents=True, exist_ok=True)
    now = dt.datetime.now().astimezone()
    wav = RECORDINGS_DIR / f"{now:%Y%m%d-%H%M%S}.wav"
    sf.write(wav, samples, SAMPLE_RATE, subtype="PCM_16")
    sidecar = {
        "note": note,
        "duration_s": round(len(samples) / SAMPLE_RATE, 3),
        "sample_rate": SAMPLE_RATE,
        "created_at": now.isoformat(timespec="seconds"),
    }
    wav.with_suffix(".json").write_text(
        json.dumps(sidecar, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    return wav


def record(seconds: float, note: str = "") -> Path:
    """Record a fixed duration from the default microphone and return the WAV path."""
    print(f"Recording {seconds:.1f} s ...")
    data = sd.rec(int(seconds * SAMPLE_RATE), samplerate=SAMPLE_RATE, channels=1, dtype="float32")
    sd.wait()
    wav = _save(data[:, 0], note)
    print(f"Saved {wav.name} ({len(data) / SAMPLE_RATE:.1f} s)")
    return wav


def record_until_enter(note: str = "") -> Path:
    """Record until Enter is pressed and return the WAV path."""
    chunks: list[np.ndarray] = []

    def on_audio(indata: np.ndarray, _frames: int, _time: object, status: sd.CallbackFlags) -> None:
        if status:
            print(status)
        chunks.append(indata.copy())

    with sd.InputStream(samplerate=SAMPLE_RATE, channels=1, dtype="float32", callback=on_audio):
        input("Recording ... press Enter to stop\n")
    samples = np.concatenate(chunks)[:, 0] if chunks else np.zeros(0, dtype="float32")
    wav = _save(samples, note)
    print(f"Saved {wav.name} ({len(samples) / SAMPLE_RATE:.1f} s)")
    return wav


def latest_recording() -> Path | None:
    """The newest WAV in `recordings/`, or None."""
    wavs = sorted(RECORDINGS_DIR.glob("*.wav"))
    return wavs[-1] if wavs else None
