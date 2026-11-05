"""Audio helpers shared by the models, the clip set and the recorder."""

import hashlib
from pathlib import Path

import numpy as np
import soundfile as sf

SAMPLE_RATE = 16_000


def duration_s(path: Path) -> float:
    info = sf.info(path)
    return info.frames / info.samplerate


def load_16k_mono(path: Path) -> np.ndarray:
    """Read a WAV file as float32 mono samples; every model here expects 16 kHz input."""
    samples, rate = sf.read(path, dtype="float32", always_2d=True)
    if rate != SAMPLE_RATE:
        msg = f"{path}: expected {SAMPLE_RATE} Hz audio, got {rate} Hz"
        raise ValueError(msg)
    return samples.mean(axis=1) if samples.shape[1] > 1 else samples[:, 0]


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()
