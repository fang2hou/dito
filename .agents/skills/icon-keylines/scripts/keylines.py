#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["resvg-py==0.5.0", "pillow==13.0.0", "numpy==2.5.4", "scipy==1.18.1"]
# ///
"""Normalize the optical size of a set of 24 pt outline SVG icons (keylines).

  keylines.py plan  DIR [--out plan.json]      measure every icon, print the plan
  keylines.py bake  DIR --plan plan.json [--write]   apply the plan to the SVGs
  keylines.py sheet DIR OUT.png                contact sheet inside circles

Rule (all lengths in pt on the 24 pt canvas, stroke-inclusive):
  marks   keep their own box (no scaling, no shift)
  bodies  scale about the canvas center until the convex hull of the ink is
          about --area pt^2, at most --cap pt on a side, never so far that a
          gap between strokes drops under --gap pt (density floor)
  all     translate so the mean of bbox center and ink centroid sits on the
          canvas center; shifts under --min-shift are dropped, the rest capped
          at --max-shift. The stroke width never scales.

Exit codes: 0 ok, 2 usage or invalid input, 3 nothing to do (no SVG found).
"""

from __future__ import annotations

import argparse
import fnmatch
import io
import json
import re
import sys
from pathlib import Path

import numpy as np
import resvg_py
from PIL import Image, ImageDraw
from scipy import ndimage
from scipy.spatial import ConvexHull

SS = 16  # supersampling per pt: 1/16 pt resolution keeps hull and gap errors under 0.1 pt
CANVAS = 24
DEFAULT_MARKS = "x,plus,minus,check,chevron-*,arrow-*,ellipsis*"
TIGHT_KEEP = 0.9  # a tight icon (gap already under the floor) is only shrunk when it is more than 10 percent over target
TOL = 0.015  # scale within 1.5 percent of 1 is rounded to 1, so reruns stay idempotent


def die(msg: str, code: int = 2) -> None:
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(code)


def render(svg: str) -> np.ndarray:
    try:
        png = bytes(
            resvg_py.svg_to_bytes(svg_string=svg, width=CANVAS * SS, height=CANVAS * SS)
        )
    except ValueError as exc:
        die(f"cannot render SVG: {exc}")
    return np.array(Image.open(io.BytesIO(png)).convert("RGBA"))[:, :, 3] / 255.0


def ink_stats(a: np.ndarray) -> dict:
    ys, xs = np.nonzero(a > 0.5)
    if len(xs) == 0:
        die(
            "icon renders empty; check currentColor/stroke attributes. Try replacing currentColor with #000."
        )
    x0, x1, y0, y1 = (
        xs.min() / SS,
        (xs.max() + 1) / SS,
        ys.min() / SS,
        (ys.max() + 1) / SS,
    )
    mass = a.sum()
    return {
        "x0": x0,
        "x1": x1,
        "y0": y0,
        "y1": y1,
        "w": x1 - x0,
        "h": y1 - y0,
        "cx": (a.sum(0) * np.arange(a.shape[1])).sum() / mass / SS,
        "cy": (a.sum(1) * np.arange(a.shape[0])).sum() / mass / SS,
        "hull": ConvexHull(np.stack([xs, ys], 1) / SS).volume,
    }


def topology(mask: np.ndarray) -> tuple[int, int]:
    return ndimage.label(mask)[1], ndimage.label(~np.pad(mask, 1))[1]


def merged_topology(a: np.ndarray, gap: float) -> tuple[int, int]:
    """Topology after growing the ink by gap/2: a gap narrower than `gap` closes."""
    k = max(1, round(gap / 2 * SS))
    yy, xx = np.mgrid[-k : k + 1, -k : k + 1]
    return topology(
        ndimage.binary_dilation(a > 0.5, structure=(xx**2 + yy**2) <= k * k)
    )


def tint(svg: str) -> str:
    return svg.replace("currentColor", "#000")


def wrap(svg: str, s: float, tx: float, ty: float) -> str:
    """Scale about the canvas center and keep the effective stroke width. Used for
    measuring and as the fallback when an SVG cannot be baked."""
    m = re.match(r"(<svg[^>]*>)(.*)(</svg>)", svg.strip(), re.DOTALL)
    if not m:
        die("not an <svg> document")
    head, body, tail = m.groups()
    body = re.sub(
        r'stroke-width="([\d.]+)"',
        lambda g: f'stroke-width="{float(g.group(1)) / s:.5f}"',
        body,
    )
    c = CANVAS / 2
    return f'{head}<g transform="translate({c + tx:.4f} {c + ty:.4f}) scale({s:.5f}) translate({-c} {-c})">{body}</g>{tail}'


def fmt(v: float) -> str:
    out = f"{v:.3f}".rstrip("0").rstrip(".")
    return "0" if out in ("-0", "") else out


def bake(svg: str, s: float, tx: float, ty: float) -> str | None:
    """Scale path coordinates and move the matrix origin. Works for design-tool exports
    that use only <path transform="matrix(a b c d e f)" d="M/L/C/Z ...">; returns None otherwise."""
    if re.search(r"<(rect|circle|ellipse|line|polyline|polygon)\b", svg):
        return None
    paths = re.findall(r"<path\b[^>]*>", svg)
    if any(
        'transform="matrix(' not in p
        or re.search(r' d="[^"]*[^MLCZ0-9 .\-,"][^"]*"', p)
        for p in paths
        if "clip" not in p and ' d="M0 0L24 0L24 24L0 24Z"' not in p
    ):
        return None
    c = CANVAS / 2

    def path(m: re.Match) -> str:
        a, b, cc, d, e, f = (float(v) for v in m.group(1).split())
        mat = " ".join(
            fmt(v) for v in (a, b, cc, d, c + s * (e - c) + tx, c + s * (f - c) + ty)
        )
        scaled = "".join(
            cmd + re.sub(r"-?\d+(?:\.\d+)?", lambda g: fmt(float(g.group(0)) * s), args)
            for cmd, args in re.findall(r"([MLCZ])([^MLCZ]*)", m.group(2))
        )
        return f'<path transform="matrix({mat})" d="{scaled}"'

    return re.sub(r'<path transform="matrix\(([^)]*)\)" d="([^"]*)"', path, svg)


def plan_icon(name: str, svg: str, a: argparse.Namespace) -> dict:
    st = ink_stats(render(tint(svg)))
    is_mark = any(fnmatch.fnmatch(name, p) for p in a.marks.split(","))
    s, reason = 1.0, "mark"
    if not is_mark:
        s_area = (a.area / st["hull"]) ** 0.5
        s_cap = a.cap / max(st["w"], st["h"])
        s, reason = min(s_area, s_cap), "area" if s_area <= s_cap else "cap"
        plain = render(tint(svg))
        base = merged_topology(plain, a.gap)
        tight = base != topology(plain > 0.5)  # a gap is already under the floor
        if s < 1 and tight and s >= TIGHT_KEEP:
            s, reason = (
                1.0,
                reason + "+floor",
            )  # sits at its floor from an earlier run; shrinking again would only add noise
        elif s < 1 and not tight:

            def ok(v: float) -> bool:
                return merged_topology(render(wrap(tint(svg), v, 0, 0)), a.gap) == base

            if not ok(s):
                lo, hi = s, 1.0
                for _ in range(12):
                    mid = (lo + hi) / 2
                    lo, hi = (lo, mid) if ok(mid) else (mid, hi)
                s, reason = round(hi + 0.005, 3), reason + "+floor"
        if abs(s - 1) <= TOL or (
            0.92 <= s <= 1.08 and "floor" not in reason and a.band
        ):
            s = 1.0
        s = round(s, 3)
    st2 = ink_stats(render(wrap(tint(svg), s, 0, 0)))
    tx = ty = 0.0
    if not is_mark and not any(
        fnmatch.fnmatch(name, p) for p in a.no_shift.split(",") if p
    ):
        ox, oy = (
            ((st2["x0"] + st2["x1"]) / 2 + st2["cx"]) / 2,
            ((st2["y0"] + st2["y1"]) / 2 + st2["cy"]) / 2,
        )
        tx, ty = (
            float(np.clip(round((CANVAS / 2 - o) * 4) / 4, -a.max_shift, a.max_shift))
            for o in (ox, oy)
        )
        tx, ty = (0.0 if abs(v) < a.min_shift else v for v in (tx, ty))
    return {
        "s": s,
        "tx": tx,
        "ty": ty,
        "reason": reason,
        "w": round(st["w"], 1),
        "h": round(st["h"], 1),
        "hull": round(st["hull"]),
    }


def icons_in(directory: str) -> list[Path]:
    files = sorted(Path(directory).glob("*.svg"))
    if not files:
        die(f"no *.svg files in {directory}", 3)
    return files


def cmd_plan(a: argparse.Namespace) -> None:
    plan = {f.stem: plan_icon(f.stem, f.read_text(), a) for f in icons_in(a.dir)}
    print(f"{'name':<22}{'s':>6}{'tx':>6}{'ty':>6}  {'reason':<12}{'now w x h':>12}")
    for k, v in plan.items():
        print(
            f"{k:<22}{v['s']:>6.3f}{v['tx']:>6.2f}{v['ty']:>6.2f}  {v['reason']:<12}{v['w']:>6.1f} x {v['h']:<5.1f}"
        )
    changed = sum(1 for v in plan.values() if v["s"] != 1 or v["tx"] or v["ty"])
    print(f"\n{changed} of {len(plan)} icons change", file=sys.stderr)
    if a.out:
        Path(a.out).write_text(json.dumps(plan, indent=1))


def cmd_bake(a: argparse.Namespace) -> None:
    plan = json.loads(Path(a.plan).read_text())
    for f in icons_in(a.dir):
        v = plan.get(f.stem)
        if not v or (v["s"] == 1 and not v["tx"] and not v["ty"]):
            continue
        svg = f.read_text()
        out, how = bake(svg, v["s"], v["tx"], v["ty"]), "baked"
        if out is None:
            out, how = (
                wrap(svg, v["s"], v["tx"], v["ty"]),
                "wrapped (stroke-width compensated, not baked)",
            )
        before, after = ink_stats(render(tint(svg))), ink_stats(render(tint(out)))
        print(
            f"{f.stem:<22}{how}: {before['w']:.1f}x{before['h']:.1f} -> {after['w']:.1f}x{after['h']:.1f}"
        )
        if a.write:
            f.write_text(out)
    if not a.write:
        print("dry run; pass --write to modify files", file=sys.stderr)


def cmd_sheet(a: argparse.Namespace) -> None:
    files = icons_in(a.dir)
    cols, px = 8, 6
    cell = int(a.circle * 1.45) * px
    im = Image.new(
        "RGBA", (cols * cell, ((len(files) + cols - 1) // cols) * cell), "white"
    )
    d = ImageDraw.Draw(im)
    for i, f in enumerate(files):
        svg = tint(f.read_text())
        g = Image.open(
            io.BytesIO(
                bytes(
                    resvg_py.svg_to_bytes(
                        svg_string=svg, width=a.glyph * px, height=a.glyph * px
                    )
                )
            )
        ).convert("RGBA")
        cx, cy = (i % cols) * cell + cell // 2, (i // cols) * cell + cell // 2
        r = a.circle * px // 2
        d.ellipse((cx - r, cy - r, cx + r, cy + r), outline="#d0ccc4", width=px // 2)
        im.alpha_composite(g, (cx - a.glyph * px // 2, cy - a.glyph * px // 2))
        d.text((cx - cell // 2 + 6, cy + cell // 2 - 22), f.stem, fill="#6a6455")
    im.convert("RGB").save(a.out)
    print(f"wrote {a.out}", file=sys.stderr)


def main() -> None:
    p = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = p.add_subparsers(dest="cmd", required=True)
    pl = sub.add_parser("plan", help="measure icons and write the scale/shift plan")
    pl.add_argument("dir")
    pl.add_argument("--out", help="write the plan as JSON")
    pl.add_argument(
        "--area",
        type=float,
        default=254,
        help="target convex-hull area in pt^2 (circle 18, square 16) [254]",
    )
    pl.add_argument(
        "--cap",
        type=float,
        default=18,
        help="max ink side in pt for the area step [18]",
    )
    pl.add_argument(
        "--gap",
        type=float,
        default=1.5,
        help="density floor: smallest allowed gap between strokes, pt [1.5]",
    )
    pl.add_argument(
        "--marks",
        default=DEFAULT_MARKS,
        help=f"comma-separated name globs kept as-is [{DEFAULT_MARKS}]",
    )
    pl.add_argument(
        "--no-shift",
        default="",
        help="comma-separated name globs that keep their position, e.g. an approved offset",
    )
    pl.add_argument(
        "--min-shift",
        type=float,
        default=0.5,
        help="drop optical shifts below this, pt [0.5]",
    )
    pl.add_argument(
        "--max-shift",
        type=float,
        default=1.0,
        help="cap optical shifts at this, pt [1]",
    )
    pl.add_argument(
        "--no-band",
        dest="band",
        action="store_false",
        help="disable the 8 percent keep-as-is band",
    )
    pl.set_defaults(fn=cmd_plan)
    bk = sub.add_parser("bake", help="apply a plan to the SVG files")
    bk.add_argument("dir")
    bk.add_argument("--plan", required=True)
    bk.add_argument(
        "--write", action="store_true", help="modify files; without it only report"
    )
    bk.set_defaults(fn=cmd_bake)
    sh = sub.add_parser("sheet", help="render every icon inside a circle for review")
    sh.add_argument("dir")
    sh.add_argument("out")
    sh.add_argument("--circle", type=int, default=36, help="circle diameter in pt [36]")
    sh.add_argument("--glyph", type=int, default=20, help="glyph canvas in pt [20]")
    sh.set_defaults(fn=cmd_sheet)
    a = p.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
