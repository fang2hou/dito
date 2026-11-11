#!/usr/bin/env python3
"""Check that every `design:<id>` reference resolves, and that design/sections.toml matches the saved design.

Reads design/dito.figcraft as a zip and looks for layer names only; needs no FigCraft tooling. Local use only.
"""
import re
import subprocess
import sys
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# Wider than a valid id on purpose: `design:settings.prompts` must fail as a whole instead of resolving as `settings`.
REF = re.compile(r"design:([A-Za-z0-9_][A-Za-z0-9_./:-]*[A-Za-z0-9_])")
SOURCE_SUFFIXES = {".rs", ".md", ".py", ".sh", ".toml"}


def flatten(table: dict, prefix: str = "") -> dict[str, dict]:
    """Nested TOML tables become `a/b/c` ids; a table is a section when it has a `board`."""
    found = {}
    for key, value in table.items():
        if not isinstance(value, dict):
            continue
        sid = f"{prefix}/{key}" if prefix else key
        if "board" in value:
            found[sid] = value
        found.update(flatten(value, sid))
    return found


def layer_names(figcraft: Path) -> set[str]:
    with zipfile.ZipFile(figcraft) as z:
        text = z.read("document.json").decode()
    return set(re.findall(r'"name":\s*"([^"]+)"', text))


def tracked_files() -> list[Path]:
    listed = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, text=True, check=True).stdout
    paths = (ROOT / name for name in listed.split("\0") if name)
    return [path for path in paths if path.suffix in SOURCE_SUFFIXES and path.is_file()]


def main() -> int:
    sections = flatten(tomllib.loads((ROOT / "design/sections.toml").read_text()))
    names = layer_names(ROOT / "design/dito.figcraft")
    errors = []

    for sid, s in sections.items():
        if not s.get("layer"):
            continue
        number = s.get("item") or s["board"]
        if f"{number} {s['layer']}" not in names:
            errors.append(f"sections.toml: `{sid}` expects a layer named `{number} {s['layer']}`, not found in the design")

    used = set()
    for path in tracked_files():
        if path.name == "check-design-refs.py":
            continue
        for lineno, line in enumerate(path.read_text(errors="ignore").splitlines(), 1):
            for sid in REF.findall(line):
                used.add(sid)
                if sid not in sections:
                    errors.append(f"{path.relative_to(ROOT)}:{lineno}: unknown design id `{sid}`")

    for sid in sorted(set(sections) - used):
        print(f"note: `{sid}` is not referenced anywhere")
    for e in errors:
        print(f"error: {e}")
    print(f"{len(sections)} sections, {len(used)} referenced, {len(errors)} errors")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
