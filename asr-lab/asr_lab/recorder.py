"""Microphone recording helpers for the notebook playground.

Everything is captured as 16 kHz mono float32 — the sample rate every model in
the lab expects — and saved under recordings/ as a WAV file plus a JSON sidecar
holding capture metadata.
"""

from __future__ import annotations

import datetime as dt
import json

import numpy as np
import sounddevice as sd
import soundfile as sf

from .paths import RECORDINGS_DIR

SAMPLE_RATE = 16_000


def list_input_devices() -> str:
    """One line per capture device, marking the system default."""
    default_in = sd.default.device[0]
    lines = []
    for i, d in enumerate(sd.query_devices()):
        if d["max_input_channels"] > 0:
            mark = "  <- default" if i == default_in else ""
            lines.append(f"[{i}] {d['name']} ({d['max_input_channels']} in){mark}")
    return "\n".join(lines) or "No input device found."


def _save(recording: np.ndarray, note: str):
    RECORDINGS_DIR.mkdir(parents=True, exist_ok=True)
    stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    wav = RECORDINGS_DIR / f"{stamp}.wav"
    sf.write(wav, recording, SAMPLE_RATE, subtype="PCM_16")
    sidecar = {
        "note": note,
        "duration_s": round(len(recording) / SAMPLE_RATE, 3),
        "sample_rate": SAMPLE_RATE,
        "created_at": dt.datetime.now().isoformat(timespec="seconds"),
    }
    wav.with_suffix(".json").write_text(
        json.dumps(sidecar, ensure_ascii=False, indent=2)
    )
    return wav


def record(seconds: float, note: str = ""):
    """Record a fixed duration from the default microphone; returns the WAV path."""
    print(f"● Recording {seconds:.1f}s ...")
    data = sd.rec(
        int(seconds * SAMPLE_RATE), samplerate=SAMPLE_RATE, channels=1, dtype="float32"
    )
    sd.wait()
    wav = _save(data.squeeze(axis=1), note)
    print(f"✔ Saved {wav.name} ({len(data) / SAMPLE_RATE:.1f}s)")
    return wav


def record_until_enter(note: str = ""):
    """Record until Enter is pressed in the cell; returns the WAV path."""
    chunks: list[np.ndarray] = []

    def on_audio(indata, _frames, _time, status):
        if status:
            print(status)
        chunks.append(indata.copy())

    with sd.InputStream(
        samplerate=SAMPLE_RATE, channels=1, dtype="float32", callback=on_audio
    ):
        input("● Recording ... press Enter to stop\n")
    data = (
        np.concatenate(chunks).squeeze(axis=1)
        if chunks
        else np.zeros(0, dtype="float32")
    )
    wav = _save(data, note)
    print(f"✔ Saved {wav.name} ({len(data) / SAMPLE_RATE:.1f}s)")
    return wav


def latest_recording():
    """Newest WAV in recordings/, or None."""
    if not RECORDINGS_DIR.exists():
        return None
    wavs = sorted(RECORDINGS_DIR.glob("*.wav"))
    return wavs[-1] if wavs else None
