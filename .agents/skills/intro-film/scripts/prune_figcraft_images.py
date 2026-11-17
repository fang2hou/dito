#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# ///
"""Drop images that no layer references from a .figcraft file.

FigCraft keeps an image in the file after the last layer using it is gone, so a replaced placeholder stays in the
archive. A .figcraft file is a zip of document.json plus images/<hash>; this removes the unreferenced entries and
their imageIndex records and leaves everything else byte for byte.

    uv run --script prune_figcraft_images.py design/dito.figcraft [--apply]

Close or reload the file in FigCraft afterwards: an open tab that saves again writes the images back.
"""

import json
import shutil
import sys
import tempfile
import zipfile
from pathlib import Path


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    apply = "--apply" in sys.argv
    if len(args) != 1:
        print(__doc__)
        return 2
    path = Path(args[0])
    zin = zipfile.ZipFile(path)
    raw = zin.read("document.json").decode()
    doc = json.loads(raw)
    hashes = [e["hash"] for e in doc["imageIndex"]]
    dead = {h for h in hashes if raw.count(h) <= 1}
    sizes = {i.filename.split("/", 1)[1]: i.file_size for i in zin.infolist() if i.filename.startswith("images/")}
    print(f"{len(hashes)} images, {len(dead)} unreferenced, {sum(sizes.get(h, 0) for h in dead)} bytes")
    if not dead or not apply:
        return 0
    doc["imageIndex"] = [e for e in doc["imageIndex"] if e["hash"] not in dead]
    compact = "\n" not in raw[:200] and ", " not in raw[:200]
    out = json.dumps(doc, ensure_ascii=False, separators=(",", ":") if compact else None).encode()
    tmp = Path(tempfile.mkdtemp()) / path.name
    with zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED) as zout:
        for info in zin.infolist():
            if info.filename.startswith("images/") and info.filename.split("/", 1)[1] in dead:
                continue
            entry = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = info.external_attr
            zout.writestr(entry, out if info.filename == "document.json" else zin.read(info.filename))
    shutil.move(tmp, path)
    print(f"wrote {path} ({path.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
