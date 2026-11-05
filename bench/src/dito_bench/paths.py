"""Filesystem layout of the bench project."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DATA_DIR = ROOT / "data"
AUDIO_DIR = ROOT / "audio"
RECORDINGS_DIR = ROOT / "recordings"
MODELS_DIR = ROOT / "models"
RESULTS_DIR = ROOT / "results"
