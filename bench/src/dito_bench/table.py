"""Plain-text and Markdown tables that stay aligned with CJK text."""

import unicodedata
from collections.abc import Mapping, Sequence
from typing import Any


def _width(text: str) -> int:
    return sum(2 if unicodedata.east_asian_width(ch) in "WF" else 1 for ch in text)


def _clip(text: str, limit: int) -> str:
    if _width(text) <= limit:
        return text
    out, used = "", 0
    for ch in text:
        step = _width(ch)
        if used + step > limit - 1:
            break
        out, used = out + ch, used + step
    return out + "…"


def _cell(value: object) -> str:
    if isinstance(value, float):
        return f"{value:.3f}"
    return "" if value is None else str(value).replace("\n", " ")


def render(
    rows: Sequence[Mapping[str, Any]],
    columns: Sequence[str],
    *,
    markdown: bool = False,
    max_width: int = 48,
) -> str:
    cells = [[_clip(_cell(row.get(c)), max_width) for c in columns] for row in rows]
    widths = [max([_width(c), *(_width(r[i]) for r in cells)]) for i, c in enumerate(columns)]

    def line(values: Sequence[str]) -> str:
        padded = [v + " " * (w - _width(v)) for v, w in zip(values, widths, strict=True)]
        return ("| " + " | ".join(padded) + " |") if markdown else "  ".join(padded).rstrip()

    header = [line(list(columns))]
    header.append(
        "| " + " | ".join("-" * w for w in widths) + " |"
        if markdown
        else "  ".join("-" * w for w in widths)
    )
    return "\n".join([*header, *(line(r) for r in cells)])
