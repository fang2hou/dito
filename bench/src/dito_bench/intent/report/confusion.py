"""Where the router confuses neighbouring intents: the boundary pairs of `data/intent/GUIDE.md`
and the full confusion matrix, from the selected head of a fit report on the test cases."""

from collections.abc import Callable, Sequence
from dataclasses import dataclass

import numpy as np

from ..data import INTENTS, IntentCase, load_cases
from ..models import MODEL_KEYS
from ..prompts import SHIPPED_VARIANT
from ..train.fit_report import read_report
from .tables import md


@dataclass(frozen=True, slots=True)
class Side:
    """Test cases that pick a gold set, and the intent that counts as a misread of them."""

    name: str
    gold: Callable[[IntentCase], bool]
    read_as: str


def _intent(name: str) -> Callable[[IntentCase], bool]:
    return lambda case: case.intent == name


def _shortened(case: IntentCase) -> bool:
    return case.intent == "rewrite" and "shorten" in case.actions


# Each pair is two sides; a side counts its cases that the router read as the other side's intent.
BOUNDARIES: tuple[tuple[Side, Side], ...] = (
    (Side("reply", _intent("reply"), "rewrite"), Side("rewrite", _intent("rewrite"), "reply")),
    (Side("reply", _intent("reply"), "ask"), Side("ask", _intent("ask"), "reply")),
    (Side("ask", _intent("ask"), "none"), Side("none", _intent("none"), "ask")),
    (
        Side("rewrite (shorten)", _shortened, "summarize"),
        Side("summarize", _intent("summarize"), "rewrite"),
    ),
    (Side("polish", _intent("polish"), "rewrite"), Side("rewrite", _intent("rewrite"), "polish")),
)


def boundary_counts(
    cases: Sequence[IntentCase], p: np.ndarray
) -> list[tuple[str, list[tuple[Side, int, int]]]]:
    """For each boundary pair its label and, per side, how many of the side's cases the router read
    as the other side's intent, and how many cases the side has."""
    predicted = [INTENTS[i] for i in p.argmax(axis=1)]
    out = []
    for left, right in BOUNDARIES:
        sides = []
        for side in (left, right):
            mine = [i for i, case in enumerate(cases) if side.gold(case)]
            sides.append((side, sum(predicted[i] == side.read_as for i in mine), len(mine)))
        out.append((f"{left.name} / {right.name}", sides))
    return out


def boundary_rows(cases: Sequence[IntentCase], p: np.ndarray) -> list[list[object]]:
    """One row per boundary pair: each side's cases, and how many of them were read as the other
    side's intent."""
    rows: list[list[object]] = []
    for label, sides in boundary_counts(cases, p):
        cells: list[object] = [label]
        for side, confused, total in sides:
            cells += [f"{side.name} read as {side.read_as}", f"{confused} of {total}"]
        rows.append(cells)
    return rows


def confusion_matrix(cases: Sequence[IntentCase], p: np.ndarray) -> np.ndarray:
    """Counts with gold intents as rows and predicted intents as columns."""
    matrix = np.zeros((len(INTENTS), len(INTENTS)), dtype=int)
    for case, row in zip(cases, p, strict=True):
        matrix[INTENTS.index(case.intent), row.argmax()] += 1
    return matrix


def confusion_markdown(
    models: Sequence[str] = MODEL_KEYS, variant: str = SHIPPED_VARIANT, chip: str | None = None
) -> str:
    """The boundary pairs and the confusion matrix of every model's selected head."""
    cases = load_cases()
    out = []
    for model in models:
        report = read_report(model, variant, chip)
        stored = report["test_probabilities"]
        if stored["ids"] != [c.id for c in cases]:
            msg = f"{model}: the test cases changed since this fit"
            raise ValueError(msg)
        p = np.asarray(stored["selected"])
        columns = ["boundary", "one side", "misread", "other side", "misread"]
        out.append(
            f"#### {model} {variant}, boundary pairs\n\n{md(columns, boundary_rows(cases, p))}"
        )
        matrix = confusion_matrix(cases, p)
        rows = [[gold, *matrix[i]] for i, gold in enumerate(INTENTS)]
        out.append(f"#### {model} {variant}, gold (rows) by predicted (columns)\n")
        out.append(md(["gold", *INTENTS], rows))
    return "\n\n".join(out)
