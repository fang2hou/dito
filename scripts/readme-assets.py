"""Regenerate the README animations in assets/readme from the running app.

    mise run readme:assets

Builds are not started here: build the screenshot binary first with
`cargo build --profile local -p dito-app --no-default-features --features mock,screenshots`.
Needs ImageMagick (`magick`) and `img2webp` (`brew install imagemagick webp`).
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "target" / "local" / "dito"
OUT = ROOT / "assets" / "readme"
LANGS = ["ja", "en", "zh", "ko"]
SCENES = ["dictation", "command", "meeting", "chat"]
THEME = "dark"
MAX_WIDTH = 800
MIN_MS, MAX_MS, TAIL_MS = 40, 600, 1800
FRAME = re.compile(r"^(?P<scene>[a-z]+)-(?P<ms>\d{6})(?P<tag>-[a-z]+)?\.png$")


def film(lang: str, work: Path) -> Path:
    shots = work / f"shots-{lang}"
    shots.mkdir()
    env = {
        **os.environ,
        "DITO_SHOTS_ONLY": "readmefilm",
        "DITO_FILM_LANG": lang,
        "DITO_FILM_THEME": THEME,
    }
    subprocess.run(
        [
            str(BINARY),
            "--mock",
            "--log-sink",
            "--data-dir",
            str(work / f"data-{lang}"),
            "--screenshots",
            str(shots),
        ],
        env=env,
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return shots


def frames_of(shots: Path, scene: str) -> list[tuple[int, Path]]:
    found = []
    for path in sorted(shots.glob(f"{scene}-*.png")):
        match = FRAME.match(path.name)
        if match and match["tag"] in (None, "-rec", "-fin"):
            found.append((int(match["ms"]), path))
    return found


def assemble(frames: list[tuple[int, Path]], target: Path, work: Path) -> None:
    sized = work / f"sized-{target.stem}"
    sized.mkdir()
    args = ["img2webp", "-loop", "0", "-lossy", "-q", "78", "-m", "6"]
    for i, (ms, path) in enumerate(frames):
        if i + 1 < len(frames):
            duration = min(max(frames[i + 1][0] - ms, MIN_MS), MAX_MS)
        else:
            duration = TAIL_MS
        small = sized / f"{i:04}.png"
        subprocess.run(
            ["magick", str(path), "-resize", f"{MAX_WIDTH}x>", str(small)], check=True
        )
        args += ["-d", str(duration), str(small)]
    subprocess.run(
        [*args, "-o", str(target)],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    print(
        f"{target.relative_to(ROOT)}: {len(frames)} frames, {target.stat().st_size // 1024} KiB"
    )


def main() -> int:
    if not BINARY.exists():
        print(
            f"missing {BINARY}; see the docstring for the build command",
            file=sys.stderr,
        )
        return 1
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        for lang in LANGS:
            shots = film(lang, work)
            for scene in SCENES:
                frames = frames_of(shots, scene)
                if frames:
                    assemble(frames, OUT / f"{scene}-{lang}.webp", work)
        shutil.rmtree(work, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
