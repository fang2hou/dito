"""Polish benchmark: run missing cases into the result store, and report from the store."""

import json
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from typing import Any

from .. import env
from ..paths import DATA_DIR
from ..store import ResultStore, fingerprint
from ..sync import Case, sync_store
from .base import MAX_TOKENS, Polished, Polisher
from .prompts import system_prompt
from .registry import get_polish_spec

TASK = "polish"
STYLE = "general"


@dataclass(frozen=True, slots=True)
class PolishCase:
    id: str
    lang: str
    note: str
    text: str
    dictionary: tuple[str, ...] = ()


def load_cases() -> list[PolishCase]:
    raw = json.loads((DATA_DIR / "polish_cases.json").read_text(encoding="utf-8"))
    return [
        PolishCase(
            id=item["id"],
            lang=item["lang"],
            note=item["note"],
            text=item["text"],
            dictionary=tuple(item.get("dictionary", ())),
        )
        for item in raw
    ]


def _prompt(case: PolishCase) -> str:
    return system_prompt(case.lang, STYLE, list(case.dictionary))


def build_cases(cases: Sequence[PolishCase]) -> list[Case[PolishCase]]:
    # The fingerprint covers the exact prompt, so a prompt edit makes stored results stale.
    return [
        Case(c.id, fingerprint(_prompt(c), c.text, f"max_tokens={MAX_TOKENS}"), c) for c in cases
    ]


def _execute(model: Polisher, case: PolishCase) -> dict[str, Any]:
    result: Polished = model.polish(case.text, _prompt(case))
    return {
        "text": result.text,
        "elapsed_s": round(result.elapsed_s, 3),
        "tokens": result.tokens,
    }


def _warmup(model: Polisher) -> None:
    model.polish("テストです", system_prompt("ja"))


def run(
    keys: Sequence[str],
    *,
    refresh: bool = False,
    chip: str | None = None,
    log: Callable[[str], None] = print,
) -> None:
    cases = build_cases(load_cases())
    chip_slug = env.chip_slug(chip)
    for key in keys:
        spec = get_polish_spec(key)
        log(f"[{key}] {spec.label} ({spec.runtime})")
        store = ResultStore.open(TASK, chip_slug, key)
        sync_store(
            store,
            weights=spec.weights,
            packages=spec.packages,
            cases=cases,
            load=lambda spec=spec: spec.load(spec.weights),
            warmup=_warmup,
            execute=_execute,
            refresh=refresh,
            log=log,
        )


def report_rows(keys: Sequence[str], *, chip: str | None = None) -> tuple[list[dict], list[str]]:
    """Rows built purely from stored records; no model runs. Also returns warnings."""
    cases = {c.id: c for c in load_cases()}
    chip_slug = env.chip_slug(chip)
    rows: list[dict] = []
    warnings: list[str] = []
    for key in keys:
        store = ResultStore.open(TASK, chip_slug, key)
        if not store.records:
            warnings.append(f"{key}: no results yet (run `dito-bench polish run -m {key}`)")
            continue
        for case_id, record in store.records.items():
            case = cases.get(case_id)
            if case is None:
                warnings.append(f"{key}: record {case_id!r} refers to a case that no longer exists")
                continue
            stale = not store.is_fresh(case_id, build_cases([case])[0].fingerprint)
            result = record.result
            elapsed = result["elapsed_s"]
            rows.append(
                {
                    "model": key,
                    "case": case_id,
                    "stale": "yes" if stale else "",
                    "elapsed_s": elapsed,
                    "tok_s": round(result["tokens"] / elapsed, 1) if elapsed > 0 else 0.0,
                    "output": result["text"],
                }
            )
    return rows, warnings
