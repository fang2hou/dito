"""Local ASR playground: record, save, and compare open-source ASR models on macOS."""

from .models import MODELS, load_model
from .recorder import latest_recording, list_input_devices, record, record_until_enter
from .testset import ensure_testset

__all__ = [
    "MODELS",
    "compare",
    "ensure_testset",
    "latest_recording",
    "list_input_devices",
    "load_model",
    "record",
    "record_until_enter",
    "run_benchmark",
]


def __getattr__(name: str):
    # Imported lazily so `python -m asr_lab.bench` does not double-import the
    # module (and notebooks that only record do not pay for pandas/jiwer).
    if name in {"compare", "run_benchmark"}:
        from . import bench

        return getattr(bench, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
