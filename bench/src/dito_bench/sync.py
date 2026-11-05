"""Run only the benchmark cases whose stored result is missing or stale."""

from collections.abc import Callable, Sequence
from dataclasses import dataclass
from typing import Any

from . import env
from .store import ResultStore
from .weights import Weights


@dataclass(frozen=True, slots=True)
class Case[P]:
    """One unit of benchmark work. `fingerprint` covers every input that affects the output."""

    id: str
    fingerprint: str
    payload: P


def sync_store[M, P](
    store: ResultStore,
    *,
    weights: Weights,
    packages: tuple[str, ...],
    cases: Sequence[Case[P]],
    load: Callable[[], M],
    warmup: Callable[[M], None],
    execute: Callable[[M, P], dict[str, Any]],
    refresh: bool = False,
    log: Callable[[str], None] = print,
) -> int:
    """Execute stale or missing cases, saving after each one. Returns how many were run."""
    if store.invalidate_if_weights_changed(weights.identity()):
        log("  weights changed since the last run, discarding old records")
    todo = [c for c in cases if refresh or not store.is_fresh(c.id, c.fingerprint)]
    if not todo:
        log(f"  up to date ({len(cases)} cases)")
        return 0
    model = load()
    warmup(model)
    store.weights = weights.identity()
    store.environment = env.describe(packages)
    try:
        for case in todo:
            result = execute(model, case.payload)
            store.put(case.id, case.fingerprint, result)
            store.save()
            log(f"  ran {case.id}")
    finally:
        store.save()
    return len(todo)
