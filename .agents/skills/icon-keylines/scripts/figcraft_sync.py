#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Carry a keylines plan into a .figcraft file through the figcraft-live control channel.

Master edits do not reach existing instances by themselves, so the work has three steps.
`sync` runs all of them; the single steps write batch files that `apply` sends to the running
FigCraft app.

  figcraft_sync.py sync      design.figcraft PLAN.json [--apply] [--project DIR]
  figcraft_sync.py master    NAME ICON.svg OUT.json --parent FRAME_ID [--x N --y N]

  figcraft_sync.py masters   BEFORE.json PLAN.json OUT.json [--prefix lucide/] [--skip NAME,..]
  figcraft_sync.py instances BEFORE.json PLAN.json OUT.json [--prefix lucide/]
  figcraft_sync.py repair    BEFORE.json AFTER.json OUT.json [--prefix lucide/]
  figcraft_sync.py apply     BATCH.json [--chunk 600] [--project DIR]

BEFORE.json and AFTER.json come from `figcraft-cli convert design.figcraft X.json`, taken
before step 1 and after step 2. The plan comes from `keylines.py plan --out`.

Exit codes: 0 ok, 2 usage or invalid input, 4 control channel failure.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

IDENTITY = [1, 0, 0, 1, 0, 0]
MCP = Path.home() / ".local/bin/figcraft-live-mcp"
TOL = 0.05


def die(msg: str, code: int = 2) -> None:
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(code)


def load(path: str) -> dict[int, dict]:
    try:
        return {
            int(k): v for k, v in json.loads(Path(path).read_text())["nodes"].items()
        }
    except (OSError, KeyError, ValueError) as exc:
        die(f"cannot read {path} as a figcraft-cli JSON export: {exc}")


def masters_of(nodes: dict, prefix: str) -> dict[str, int]:
    found = {
        v["name"][len(prefix) :]: k
        for k, v in nodes.items()
        if v["type"] == "component" and v.get("name", "").startswith(prefix)
    }
    if not found:
        die(f"no component named {prefix}* in the file; pass --prefix")
    return found


def page_xy(nodes: dict, i: int) -> tuple[float, float]:
    x = y = 0.0
    while i in nodes:
        t = nodes[i].get("transform", IDENTITY)
        x, y = x + t[4], y + t[5]
        i = nodes[i].get("parent")
    return x, y


def leaves(nodes: dict, i: int) -> list[int]:
    kids = nodes[i].get("children")
    return [leaf for c in kids for leaf in leaves(nodes, c)] if kids else [i]


def pairs(
    nodes: dict, master: int, inst: int, out: list | None = None
) -> list[tuple[int, int]]:
    out = [] if out is None else out
    out.append((master, inst))
    for a, b in zip(nodes[master].get("children", []), nodes[inst].get("children", [])):
        pairs(nodes, a, b, out)
    return out


def write(out: str, label: str, ops: list) -> None:
    Path(out).write_text(json.dumps({"label": label, "commands": ops}))
    print(f"{len(ops)} commands -> {out}", file=sys.stderr)


def changed_masters(nodes: dict, plan: dict, prefix: str) -> dict[int, dict]:
    ids = masters_of(nodes, prefix)
    return {
        ids[n]: v
        for n, v in plan.items()
        if n in ids and (v["s"] != 1 or v["tx"] or v["ty"])
    }


def cmd_masters(a: argparse.Namespace) -> None:
    nodes, plan = load(a.before), json.loads(Path(a.plan).read_text())
    skip = set(filter(None, a.skip.split(",")))
    ops = []
    for mid, v in changed_masters(nodes, plan, a.prefix).items():
        kids = nodes[mid]["children"]
        px, py = page_xy(nodes, mid)
        if v["s"] != 1 and nodes[mid]["name"][len(a.prefix) :] not in skip:
            ops.append(
                {
                    "command": "transform.scale",
                    "params": {
                        "ids": kids,
                        "factor": v["s"],
                        "cx": px + 12,
                        "cy": py + 12,
                    },
                }
            )
        if v["tx"] or v["ty"]:
            ops.append(
                {
                    "command": "transform.move",
                    "params": {"ids": kids, "dx": v["tx"], "dy": v["ty"]},
                }
            )
        # transform.scale scales strokes too (the K tool); the master stroke must stay at its original weight.
        ops += [
            {
                "command": "node.set",
                "params": {"ids": [n], "strokeWeight": nodes[n]["stroke"]["weight"]},
            }
            for n in leaves(nodes, mid)
            if nodes[n].get("strokes")
        ]
    write(a.out, "keylines-masters", ops)


def cmd_instances(a: argparse.Namespace) -> None:
    nodes, plan = load(a.before), json.loads(Path(a.plan).read_text())
    changed = changed_masters(nodes, plan, a.prefix)
    ops = []
    for nid, n in nodes.items():
        if n["type"] != "instance" or n.get("component") not in changed:
            continue
        ratio = n["width"] / nodes[n["component"]]["width"]
        for _, i in pairs(nodes, n["component"], nid)[1:]:
            ops += [
                {"command": "component.resetOverride", "params": {"id": i, "group": g}}
                for g in ("size", "position")
            ]
        weight = next(
            (
                nodes[x]["stroke"]["weight"]
                for x in leaves(nodes, n["component"])
                if nodes[x].get("strokes")
            ),
            None,
        )
        for leaf in leaves(nodes, nid):
            if leaf != nid and nodes[leaf].get("strokes") and weight:
                ops.append(
                    {
                        "command": "node.set",
                        "params": {
                            "ids": [leaf],
                            "strokeWeight": round(weight * ratio, 5),
                        },
                    }
                )
    write(a.out, "keylines-instances", ops)


def cmd_repair(a: argparse.Namespace) -> None:
    before, after = load(a.before), load(a.after)
    ids = set(masters_of(after, a.prefix).values())
    resized, restore, corners, resync = [], [], [], []
    for k, b in before.items():
        n = after.get(k)
        if not n or b == n or b["type"] != "instance":
            continue
        if (b["width"], b["height"]) != (n["width"], n["height"]):
            t = b.get("transform", IDENTITY)
            resized.append(
                {
                    "command": "node.set",
                    "params": {
                        "ids": [k],
                        "width": b["width"],
                        "height": b["height"],
                        "x": t[4],
                        "y": t[5],
                    },
                }
            )
        for i, (x, y) in enumerate(zip(b.get("effects", []), n.get("effects", []))):
            if x != y:
                restore.append(
                    {
                        "command": "effect.set",
                        "params": {"ids": [k], "index": i, "effect": x},
                    }
                )
        for field in set(n.get("bound", {})) - set(b.get("bound", {})):
            restore.append(
                {
                    "command": "variable.bind",
                    "params": {"ids": [k], "field": field, "variable": None},
                }
            )
        sb, sa = b.get("stroke"), n.get("stroke")
        if sb and sa and sb.get("weight") != sa.get("weight"):
            restore.append(
                {
                    "command": "node.set",
                    "params": {"ids": [k], "strokeWeight": sb["weight"]},
                }
            )
    for k, n in after.items():
        if n["type"] != "instance" or n.get("component") not in ids:
            continue
        _master, ratio = (
            after[n["component"]],
            n["width"] / after[n["component"]]["width"],
        )
        for m, i in pairs(after, n["component"], k)[1:]:
            mn, inn = after[m], after[i]
            if inn["type"] == "rectangle" and mn.get("corners"):
                corners.append(
                    {
                        "command": "node.set",
                        "params": {
                            "ids": [i],
                            "cornerRadius": round(mn["corners"]["radii"][0] * ratio, 5),
                        },
                    }
                )
            if (
                abs(inn["width"] - mn["width"] * ratio) > TOL
                or abs(inn["height"] - mn["height"] * ratio) > TOL
            ):
                resync += [
                    {
                        "command": "component.resetOverride",
                        "params": {"id": i, "group": g},
                    }
                    for g in ("size", "position")
                ]
                if inn.get("strokes"):
                    weight = mn["stroke"]["weight"]
                    resync.append(
                        {
                            "command": "node.set",
                            "params": {
                                "ids": [i],
                                "strokeWeight": round(weight * ratio, 5),
                            },
                        }
                    )
    # Resized instances rescale their contour, so every leaf stroke below them is set again.
    strokes = []
    for op in resized:
        k = op["params"]["ids"][0]
        for leaf in leaves(before, k):
            if leaf != k and before[leaf].get("strokes"):
                strokes.append(
                    {
                        "command": "node.set",
                        "params": {
                            "ids": [leaf],
                            "strokeWeight": before[leaf]["stroke"]["weight"],
                        },
                    }
                )
    print(
        f"resized {len(resized)}, restored {len(restore)}, corners {len(corners)}, resynced {len(resync)}",
        file=sys.stderr,
    )
    write(a.out, "keylines-repair", resized + strokes + restore + corners + resync)


def call(name: str, args: dict, project: str | None) -> dict:
    if not MCP.exists():
        die(
            f"{MCP} not found; install figcraft-live-mcp (see the figcraft-live skill)",
            4,
        )
    msgs = [
        {
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {"protocolVersion": "2025-06-18"},
        },
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": name, "arguments": args},
        },
    ]
    run = subprocess.run(
        check=False,
        args=[str(MCP)],
        input="\n".join(map(json.dumps, msgs)) + "\n",
        capture_output=True,
        text=True,
        cwd=project,
        timeout=300,
    )
    for line in run.stdout.splitlines():
        reply = json.loads(line)
        if reply.get("id") == 1:
            return json.loads(reply["result"]["content"][0]["text"])
    die(f"no reply from figcraft-live-mcp: {run.stderr.strip()[:300]}", 4)


def run_batches(ops: list, label: str, chunk: int, project: str | None) -> None:
    for start in range(0, len(ops), chunk):
        reply = call(
            "batch",
            {
                "label": f"{label}-{start // chunk}",
                "commands": ops[start : start + chunk],
            },
            project,
        )
        if not reply.get("ok"):
            die(
                f"chunk at command {start} failed and was rolled back: {reply.get('error')}. "
                "Start the app with figcraft-live `ensure` first if it is not running.",
                4,
            )
    print(
        f"applied {len(ops)} commands in {-(-len(ops) // chunk)} batches",
        file=sys.stderr,
    )


def read_batch(path: str) -> tuple[list, str]:
    data = json.loads(Path(path).read_text())
    return (
        (data["commands"], data.get("label", "keylines"))
        if isinstance(data, dict)
        else (data, "keylines")
    )


def cmd_apply(a: argparse.Namespace) -> None:
    ops, label = read_batch(a.batch)
    run_batches(ops, label, a.chunk, a.project)


def convert(design: Path, out: Path) -> None:
    run = subprocess.run(
        ["figcraft-cli", "convert", str(design), str(out)],
        capture_output=True,
        text=True,
        check=False,
    )
    if run.returncode:
        die(f"figcraft-cli convert failed: {run.stderr.strip()[:300]}", 4)


def save(design: Path, project: str | None) -> None:
    reply = call(
        "exec",
        {"command": "file.save", "params": {"path": str(design), "overwrite": True}},
        project,
    )
    if not reply.get("ok"):
        die(f"file.save failed: {reply.get('error')}", 4)


def cmd_sync(a: argparse.Namespace) -> None:
    """masters, instances and repair as one run. Without --apply it only counts the commands."""
    design, tmp = Path(a.design).resolve(), Path(a.workdir)
    tmp.mkdir(parents=True, exist_ok=True)
    before, after = tmp / "before.json", tmp / "after.json"
    convert(design, before)
    for name, fn in (("masters", cmd_masters), ("instances", cmd_instances)):
        ns = argparse.Namespace(
            before=str(before),
            plan=a.plan,
            out=str(tmp / f"{name}.json"),
            prefix=a.prefix,
            skip=a.skip,
        )
        fn(ns)
    if not a.apply:
        print(
            "dry run; pass --apply to edit the open FigCraft document and save it",
            file=sys.stderr,
        )
        return
    for name in ("masters", "instances"):
        ops, label = read_batch(str(tmp / f"{name}.json"))
        run_batches(ops, label, a.chunk, a.project)
    save(design, a.project)
    convert(design, after)
    cmd_repair(
        argparse.Namespace(
            before=str(before),
            after=str(after),
            out=str(tmp / "repair.json"),
            prefix=a.prefix,
        )
    )
    ops, label = read_batch(str(tmp / "repair.json"))
    run_batches(ops, label, a.chunk, a.project)
    save(design, a.project)
    print("saved; now run the project's design check", file=sys.stderr)


def svg_paths(svg: str) -> list[tuple[str, float, float]]:
    """(path data, x offset, y offset) for each shape. Supports path (plain or a translate-only
    matrix), line, polyline and circle; anything else is an error."""
    if re.search(r"<(rect|ellipse|polygon)\b", svg):
        die(
            "rect, ellipse and polygon are not supported; convert the shape to a path first"
        )
    out = []
    for m in re.finditer(r"<(path|line|polyline|circle)\b([^>]*)>", svg):
        kind, attrs = m.group(1), dict(re.findall(r'([\w-]+)="([^"]*)"', m.group(2)))
        dx = dy = 0.0
        if "transform" in attrs:
            t = re.fullmatch(
                r"matrix\(1 0 0 1 ([-\d.]+) ([-\d.]+)\)", attrs["transform"]
            )
            if not t:
                die(
                    f"unsupported transform {attrs['transform']!r}; only a translate-only matrix is supported"
                )
            dx, dy = float(t.group(1)), float(t.group(2))
        if kind == "path":
            if "d" not in attrs:
                continue
            if attrs["d"].strip() == "M0 0L24 0L24 24L0 24Z":
                continue  # the canvas clip rectangle of a design-tool export
            out.append((attrs["d"], dx, dy))
        elif kind == "line":
            x1, y1, x2, y2 = (attrs.get(k, "0") for k in ("x1", "y1", "x2", "y2"))
            out.append((f"M{x1} {y1}L{x2} {y2}", dx, dy))
        elif kind == "polyline":
            pts = re.findall(r"[-\d.]+", attrs["points"])
            out.append(
                (
                    "M"
                    + "L".join(f"{pts[i]} {pts[i + 1]}" for i in range(0, len(pts), 2)),
                    dx,
                    dy,
                )
            )
        else:
            cx, cy, r = (float(attrs.get(k, "0")) for k in ("cx", "cy", "r"))
            k = 0.5523 * r
            out.append(
                (
                    f"M{cx + r} {cy}C{cx + r} {cy + k} {cx + k} {cy + r} {cx} {cy + r}C{cx - k} {cy + r} {cx - r} {cy + k} {cx - r} {cy}C{cx - r} {cy - k} {cx - k} {cy - r} {cx} {cy - r}C{cx + k} {cy - r} {cx + r} {cy - k} {cx + r} {cy}",
                    dx,
                    dy,
                )
            )
    if not out:
        die("no path, line, polyline or circle found in the SVG")
    return out


def cmd_master(a: argparse.Namespace) -> None:
    """A batch that creates the `<prefix><name>` master from an SVG, matching the existing masters:
    24 pt clipping component without fill, one vector per shape, 2 pt round stroke bound to the
    shared stroke variable, scale constraints on both axes."""
    shapes = svg_paths(Path(a.svg).read_text())
    ops = [
        {
            "command": "node.create",
            "as": "master",
            "params": {
                "type": "component",
                "parent": a.parent,
                "name": f"{a.prefix}{a.name}",
                "width": 24,
                "height": 24,
                "x": a.x,
                "y": a.y,
                "props": {"clip": True, "fill": "none"},
            },
        }
    ]
    for i, (d, dx, dy) in enumerate(shapes):
        props = {
            "stroke": {"$var": a.stroke_var},
            "strokeWeight": 2,
            "strokeCap": "round",
            "strokeJoin": "round",
            "fill": "none",
            "constraints": {"horizontal": "scale", "vertical": "scale"},
        }
        ops.append(
            {
                "command": "vector.create",
                "params": {
                    "parent": {"$ref": "master"},
                    "path": d,
                    "x": dx,
                    "y": dy,
                    "name": "glyph" if len(shapes) == 1 else f"glyph-{i}",
                    "props": props,
                },
            }
        )
    write(a.out, f"keylines-new-master-{a.name}", ops)


def main() -> None:
    p = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = p.add_subparsers(dest="cmd", required=True)
    for name, fn, files in (
        ("masters", cmd_masters, ("before", "plan", "out")),
        ("instances", cmd_instances, ("before", "plan", "out")),
        ("repair", cmd_repair, ("before", "after", "out")),
    ):
        s = sub.add_parser(name)
        for f in files:
            s.add_argument(f)
        s.add_argument(
            "--prefix",
            default="lucide/",
            help="component name prefix of the icon masters [lucide/]",
        )
        if name == "masters":
            s.add_argument(
                "--skip",
                default="",
                help="comma-separated master names already scaled, to avoid scaling twice",
            )
        s.set_defaults(fn=fn)
    ap = sub.add_parser("apply")
    ap.add_argument("batch")
    ap.add_argument(
        "--chunk",
        type=int,
        default=600,
        help="commands per undo step; keeps each call under the 60 s UI limit [600]",
    )
    ap.add_argument(
        "--project",
        default=None,
        help="repository root, so the app resolves the shared instance",
    )
    ap.set_defaults(fn=cmd_apply)
    sy = sub.add_parser("sync", help="masters, instances and repair in one run")
    sy.add_argument("design", help="the .figcraft file open in FigCraft")
    sy.add_argument("plan")
    sy.add_argument(
        "--apply",
        action="store_true",
        help="edit the open document and save it; without it only count commands",
    )
    sy.add_argument("--prefix", default="lucide/")
    sy.add_argument("--skip", default="")
    sy.add_argument(
        "--workdir",
        default="/tmp/icon-keylines",
        help="where batches and exports are written [/tmp/icon-keylines]",
    )
    sy.add_argument("--chunk", type=int, default=600)
    sy.add_argument("--project", default=None)
    sy.set_defaults(fn=cmd_sync)
    mk = sub.add_parser(
        "master", help="write a batch that creates a new master from an SVG"
    )
    mk.add_argument("name")
    mk.add_argument("svg")
    mk.add_argument("out")
    mk.add_argument(
        "--parent",
        type=int,
        required=True,
        help="id of the frame that holds the other masters",
    )
    mk.add_argument("--prefix", default="lucide/")
    mk.add_argument("--x", type=float, default=0, help="page x inside the parent [0]")
    mk.add_argument("--y", type=float, default=0, help="page y inside the parent [0]")
    mk.add_argument(
        "--stroke-var",
        default="VariableID:7",
        help="stroke color variable used by the other masters [VariableID:7]",
    )
    mk.set_defaults(fn=cmd_master)
    a = p.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
