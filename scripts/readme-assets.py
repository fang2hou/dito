"""Regenerate the README animations in assets/readme from the running app.

    mise run readme:assets

Builds are not started here: build the screenshot binary first with
`cargo build --profile local -p dito-app --no-default-features --features mock,screenshots`.
Needs ImageMagick (`magick`) and `img2webp` (`brew install imagemagick webp`).

Run it on a Retina display: offscreen windows render at the display's scale, so a 1x display makes soft images. The
animations are 1440 px wide for the 720 pt slot in the README, with continuous (Apple style) rounded corners and the
best WebP quality that keeps a file under the size limit (lossless when it fits).
"""

import math

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
MAX_WIDTH = 1440
CORNER = 44
SMOOTHING = 0.6
MAX_BYTES = 9 * 1024 * 1024
QUALITIES = [None, 98, 95, 92, 89, 85, 80]  # None is lossless
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


def squircle_path(w: float, h: float, r: float, smoothing: float) -> str:
    """A rounded rectangle whose corners ease into the edges (continuous corners), as an SVG path."""
    p = min((1 + smoothing) * r, min(w, h) / 2)
    smoothing = min(smoothing, p / r - 1)
    arc = 90 * (1 - smoothing)
    arc_len = math.sin(math.radians(arc / 2)) * r * math.sqrt(2)
    p34 = r * math.tan(math.radians((90 - arc) / 4))
    beta = 45 * smoothing
    c = p34 * math.cos(math.radians(beta))
    d = c * math.tan(math.radians(beta))
    b = (p - arc_len - c - d) / 3
    a = 2 * b

    def f(*v: float) -> str:
        return " ".join(f"{x:.3f}" for x in v)

    return (
        f"M {w - p:.3f} 0 c {f(a, 0, a + b, 0, a + b + c, d)} a {r} {r} 0 0 1 {f(arc_len, arc_len)} c {f(d, c, d, b + c, d, a + b + c)} "
        f"L {w:.3f} {h - p:.3f} c {f(0, a, 0, a + b, -d, a + b + c)} a {r} {r} 0 0 1 {f(-arc_len, arc_len)} c {f(-c, d, -(b + c), d, -(a + b + c), d)} "
        f"L {p:.3f} {h} c {f(-a, 0, -(a + b), 0, -(a + b + c), -d)} a {r} {r} 0 0 1 {f(-arc_len, -arc_len)} c {f(-d, -c, -d, -(b + c), -d, -(a + b + c))} "
        f"L 0 {p:.3f} c {f(0, -a, 0, -(a + b), d, -(a + b + c))} a {r} {r} 0 0 1 {f(arc_len, -arc_len)} c {f(c, -d, b + c, -d, a + b + c, -d)} Z"
    )


def corner_mask(width: int, height: int, work: Path) -> Path:
    """The alpha mask for a frame of this size, drawn at 4x and scaled down for smooth edges."""
    path = work / f"mask-{width}x{height}.png"
    if not path.exists():
        k = 4
        draw = f"path '{squircle_path(width * k, height * k, CORNER * k * width / MAX_WIDTH, SMOOTHING)}'"
        subprocess.run(
            ["magick", "-size", f"{width * k}x{height * k}", "xc:black", "-fill", "white", "-draw", draw, "-filter", "Lanczos", "-resize", f"{width}x{height}!", str(path)],
            check=True,
        )
    return path


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
    durations = []
    for i, (ms, path) in enumerate(frames):
        if i + 1 < len(frames):
            durations.append(min(max(frames[i + 1][0] - ms, MIN_MS), MAX_MS))
        else:
            durations.append(TAIL_MS)
    first = subprocess.run(
        ["magick", "identify", "-format", "%w %h", str(frames[0][1])], check=True, capture_output=True, text=True
    ).stdout.split()
    width = min(int(first[0]), MAX_WIDTH)
    height = round(int(first[1]) * width / int(first[0]))
    mask = corner_mask(width, height, work)
    for i, (_, path) in enumerate(frames):
        subprocess.run(
            [
                "magick", str(path), "-filter", "Lanczos", "-resize", f"{width}x{height}!",
                "(", str(mask), "-alpha", "off", ")", "-compose", "CopyOpacity", "-composite", str(sized / f"{i:04}.png"),
            ],
            check=True,
        )
    for quality in QUALITIES:
        args = ["img2webp", "-loop", "0", "-m", "6"] + ([] if quality is None else ["-lossy", "-q", str(quality)])
        for i, duration in enumerate(durations):
            args += ["-d", str(duration), str(sized / f"{i:04}.png")]
        subprocess.run([*args, "-o", str(target)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if target.stat().st_size <= MAX_BYTES:
            break
    print(
        f"{target.relative_to(ROOT)}: {len(frames)} frames, {width}x{height}, {'lossless' if quality is None else f'q{quality}'}, {target.stat().st_size // 1024} KiB"
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
