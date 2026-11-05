"""Describe the machine a result was measured on, without identifying the user."""

import platform
import subprocess
from importlib import metadata


def _sysctl(name: str) -> str:
    out = subprocess.run(["sysctl", "-n", name], check=True, capture_output=True, text=True)
    return out.stdout.strip()


def chip() -> str:
    """CPU brand string, for example "Apple M4 Pro"."""
    return _sysctl("machdep.cpu.brand_string")


def chip_slug(name: str | None = None) -> str:
    return (name or chip()).lower().replace(" ", "-")


def describe(packages: tuple[str, ...] = ()) -> dict[str, str | int | dict[str, str]]:
    """Chip, memory, OS and runtime versions. Never includes host or user names."""
    versions: dict[str, str] = {}
    for package in packages:
        try:
            versions[package] = metadata.version(package)
        except metadata.PackageNotFoundError:
            versions[package] = "unknown"
    return {
        "chip": chip(),
        "memory_gb": round(int(_sysctl("hw.memsize")) / 2**30),
        "macos": platform.mac_ver()[0],
        "python": platform.python_version(),
        "packages": versions,
    }
