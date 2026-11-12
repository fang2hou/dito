"""The run cache: how a stored probe run is named, checked for staleness and loaded.

A run is the output of one `intent_probe` pass over a split, stored as
`cache/intent/<chip>/<model key>/<variant>.<mode>/<split>.{jsonl,requests.jsonl,meta.json,bin}`.
The mode names how it was taken: `cached` (resident prompt prefix) or `full` (whole prompt), then
`+gen-letter` or `+gen-json` when the model also generated, then `+L<layers>` when hidden states
were kept (for example `cached+Lall`), then an optional tag.

A run is valid while the requests stored with it equal the current ones. They are built from the
case texts and the prompt only, so moving a data file does not invalidate a run.
"""

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np

from ..data import INTENTS, IntentCase, load_split
from ..locations import cache_dir
from ..prompts import VARIANTS

# The runs heads are fitted on: resident prefix, hidden states of every layer.
FIT_MODE = "cached+Lall"


def run_prefix(key: str, variant: str, mode: str, split: str, chip: str | None = None) -> Path:
    """Path of a run without a suffix: add `.jsonl`, `.meta.json`, `.bin` or `.requests.jsonl`."""
    return cache_dir(chip) / key / f"{variant}.{mode}" / split


def request_lines(variant: str, cases: list[IntentCase]) -> list[dict[str, Any]]:
    """The probe requests of `cases` under a prompt variant, as `intent_probe` reads them."""
    v = VARIANTS[variant]
    return [{"id": c.id, "options": list(v.labels), **v.build(c.text)} for c in cases]


def stale(path: Path, expected: list[dict[str, Any]], keys: tuple[str, ...] | None = None) -> bool:
    """Whether the requests stored with the run at `path` differ from `expected` (on `keys` only,
    when given): the cases or the prompt changed since the run."""
    stored_path = path.with_suffix(".requests.jsonl")
    if not stored_path.exists():
        return True
    stored = [json.loads(line) for line in stored_path.read_text(encoding="utf-8").splitlines()]
    if keys is not None:
        stored = [{k: r.get(k) for k in keys} for r in stored]
        expected = [{k: r.get(k) for k in keys} for r in expected]
    return stored != expected


def run_exists(key: str, variant: str, mode: str, split: str, chip: str | None = None) -> bool:
    return run_prefix(key, variant, mode, split, chip).with_suffix(".meta.json").exists()


@dataclass(slots=True)
class Run:
    cases: list[IntentCase]
    labels: np.ndarray  # gold intent index
    logits: np.ndarray  # [n, len(INTENTS)] option logits in INTENTS order
    elapsed: np.ndarray  # milliseconds per pass
    records: list[dict[str, Any]]
    meta: dict[str, Any]
    path: Path

    def hidden(self, layer: int, pool: str = "last") -> np.ndarray:
        """Residual stream entering `layer`: `last` at the answer position, `span` averaged over
        the instruction tokens."""
        layers = self.meta["layers"]
        shape = (len(self.cases), len(layers), 2, self.meta["n_embd"])
        data = np.memmap(self.path.with_suffix(".bin"), dtype="<f4", mode="r", shape=shape)
        return np.asarray(data[:, layers.index(layer), 0 if pool == "last" else 1, :], np.float64)


def load_run(
    key: str, variant: str, split: str, mode: str = "cached", chip: str | None = None
) -> Run:
    path = run_prefix(key, variant, mode, split, chip)
    cases = load_split(split)
    lines = path.with_suffix(".jsonl").read_text(encoding="utf-8").splitlines()
    records = [json.loads(line) for line in lines]
    if [r["id"] for r in records] != [c.id for c in cases]:
        msg = f"{path}: the cases changed since this run"
        raise ValueError(msg)
    keys = ("id", "user", "span") if "+gen-" in mode else ("id", "system", "user", "span")
    if stale(path, request_lines(variant, cases), keys):
        msg = f"{path}: the case texts or the prompt changed since this run"
        raise ValueError(msg)
    order = VARIANTS[variant].order
    column = [order.index(intent) for intent in INTENTS]
    return Run(
        cases,
        np.asarray([INTENTS.index(c.intent) for c in cases]),
        np.asarray([r["option_logits"] for r in records])[:, column],
        np.asarray([r["elapsed_ms"] for r in records]),
        records,
        json.loads(path.with_suffix(".meta.json").read_text(encoding="utf-8")),
        path,
    )
