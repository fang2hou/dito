"""Polish benchmark: run missing cases into the result store, and report from the store."""

import json
import re
import unicodedata
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from decimal import Decimal
from typing import Any

from .. import env
from ..paths import DATA_DIR
from ..store import ResultStore, fingerprint
from ..sync import Case, sync_store
from .base import APP_SAMPLING, RUNTIME_VERSION, SAMPLING_PROFILES, Polished, Polisher
from .prompts import Prompt, build
from .registry import get_polish_spec

TASK = "polish"


@dataclass(frozen=True, slots=True)
class PolishCase:
    id: str
    source_id: str
    lang: str
    note: str
    text: str
    target: str = "general"
    sampling: str = "legacy"
    group: str = "baseline"
    kind: str = "control"
    wordbook: tuple[str, ...] = ()
    forbidden: tuple[str, ...] = ()
    required: tuple[str, ...] = ()
    unchanged: bool = False
    features: Mapping[str, Any] | None = None


def _case_variants(item: dict[str, Any]) -> list[PolishCase]:
    targets = item.get("targets", ["general"])
    samplings = item.get("samplings", ["legacy"])
    qualified = "targets" in item or "samplings" in item
    variants = []
    for target in targets:
        for sampling in samplings:
            case_id = f"{item['id']}--{target}--{sampling}" if qualified else item["id"]
            variants.append(
                PolishCase(
                    id=case_id,
                    source_id=item["id"],
                    lang=item["lang"],
                    note=item["note"],
                    text=item["text"],
                    target=target,
                    sampling=sampling,
                    group=item.get("group", "baseline"),
                    kind=item.get("kind", "control"),
                    wordbook=tuple(item.get("wordbook", ())),
                    forbidden=tuple(item.get("forbidden", ())),
                    required=tuple(item.get("required", ())),
                    unchanged=item.get("unchanged", False),
                    features=item.get("features"),
                )
            )
    return variants


def load_cases() -> list[PolishCase]:
    raw = json.loads((DATA_DIR / "polish_cases.json").read_text(encoding="utf-8"))
    return [case for item in raw for case in _case_variants(item)]


def _prompt(case: PolishCase) -> Prompt:
    return build(case.lang, case.text, case.target, list(case.wordbook))


def build_cases(cases: Sequence[PolishCase]) -> list[Case[PolishCase]]:
    # Target and sampling are part of the result identity, even when the transcript is shared.
    return [
        Case(
            c.id,
            fingerprint(
                repr(_prompt(c).messages()),
                RUNTIME_VERSION,
                f"target={c.target}",
                repr(SAMPLING_PROFILES[c.sampling]),
            ),
            c,
        )
        for c in cases
    ]


def _normalize_control(text: str) -> str:
    return "".join(
        char
        for char in text
        if not char.isspace() and not unicodedata.category(char).startswith("P")
    )


def _fact_form(text: str) -> str:
    text = unicodedata.normalize("NFKC", text).casefold()
    text = text.replace("percent", "%").replace("mins", "min")
    text = text.replace("nov ", "november ")
    text = text.replace("minutes", "min").replace("hours", "h").replace("hour", "h")
    text = text.replace("days", "d").replace("day", "d")
    text = text.replace("million dollars", "m").replace("thousand dollars", "k")
    text = re.sub(r"(\d+)\s*点\s*(\d+)\s*分", r"\1:\2", text)
    text = text.replace("ヶ", "か").replace("ケ", "か")
    return "".join(text.replace(":", "").replace("-", "").replace(",", "").split())


MONEY_PATTERN = re.compile(
    r"(?<!\d)(\d+(?:\.\d+)?)\s*(million|thousand)?\s+dollars?\b"
    r"|\$(\d[\d,]*(?:\.\d+)?)(m|k)?\b",
    re.I,
)


def _money_match_amount(match: re.Match[str]) -> Decimal:
    number = Decimal(match.group(1) or match.group(3).replace(",", ""))
    scale = match.group(2) or match.group(4)
    if scale and scale.casefold() in ("million", "m"):
        number *= 1_000_000
    elif scale and scale.casefold() in ("thousand", "k"):
        number *= 1_000
    return number


def _money_amount(text: str) -> Decimal | None:
    raw = unicodedata.normalize("NFKC", text)
    match = MONEY_PATTERN.search(raw)
    return _money_match_amount(match) if match else None


def _contains(text: str, expected: str) -> bool:
    expected_count = re.fullmatch(r"(\d+) (rooms|people)", expected)
    if expected_count:
        count, noun = expected_count.groups()
        alternate = "total" if noun in ("rooms", "people") else noun
        return bool(re.search(rf"(?<!\d){count}\s*(?:{noun}|{alternate})(?!\w)", text, re.I))
    expected_money = _money_amount(expected)
    if expected_money is not None:
        raw = unicodedata.normalize("NFKC", text)
        return any(
            _money_match_amount(match) == expected_money for match in MONEY_PATTERN.finditer(raw)
        )
    return _fact_form(expected) in _fact_form(text)


def score_output(case: PolishCase, output: str) -> dict[str, Any]:
    missing = [fact for fact in case.required if not _contains(output, fact)]
    forbidden = [term for term in case.forbidden if _contains(output, term)]
    unchanged = _normalize_control(output) == _normalize_control(case.text)
    input_chars = len(case.text)
    output_chars = len(output)
    return {
        "unchanged": unchanged,
        "verbatim_unchanged": output.strip() == case.text.strip(),
        "missing": missing,
        "forbidden": forbidden,
        "input_chars": input_chars,
        "output_chars": output_chars,
        "length_ratio": round(output_chars / input_chars, 4) if input_chars else 0.0,
    }


def _execute(model: Polisher, case: PolishCase) -> dict[str, Any]:
    result: Polished = model.polish(_prompt(case), SAMPLING_PROFILES[case.sampling])
    return {
        "text": result.text,
        "elapsed_s": round(result.elapsed_s, 3),
        "tokens": result.tokens,
        "prompt_tokens": result.prompt_tokens,
        "max_tokens": result.max_tokens,
        "context_tokens": result.context_tokens,
        "quality": score_output(case, result.text),
    }


def _warmup(model: Polisher) -> None:
    model.polish(build("ja", "テストです"), APP_SAMPLING)


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
        cases_by_id = {case.id: case.payload for case in cases}
        for case_id, record in store.records.items():
            case = cases_by_id.get(case_id)
            if case is not None:
                record.result["quality"] = score_output(case, record.result["text"])
        store.save()
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
