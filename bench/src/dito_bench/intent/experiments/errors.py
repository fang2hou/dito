"""List the test cases a head gets wrong, and the confident ones among them."""

import numpy as np

from ..data import INTENTS
from ..probe.runs import FIT_MODE, load_run
from ..report.tables import md
from ..train.heads import Parts, Projected, features, groups_of


def parse_parts(text: str) -> Parts:
    """`24:last,19:span` as [(24, "last"), (19, "span")]."""
    parts = []
    for item in text.split(","):
        layer, pool = item.split(":")
        parts.append((int(layer), pool))
    return parts


def errors_markdown(
    key: str, variant: str, parts: Parts, mode: str = FIT_MODE, chip: str | None = None
) -> str:
    """The test cases a head fitted on `parts` of the training run gets wrong."""
    train, test = (load_run(key, variant, s, mode, chip) for s in ("train", "test"))
    head = Projected(features(train, parts), train.labels, groups_of(train.cases), len(INTENTS))
    p = head.probabilities(features(test, parts))
    wrong = np.flatnonzero(p.argmax(axis=1) != test.labels)
    rows = []
    for i in wrong:
        c = test.cases[i]
        predicted = INTENTS[p[i].argmax()]
        rows.append(
            [c.id, c.intent, predicted, f"{p[i].max():.2f}", ", ".join(c.tags), c.text[:60]]
        )
    table = md(["id", "gold", "predicted", "p", "tags", "text"], rows)
    return f"{len(wrong)} wrong of {len(test.cases)}\n\n{table}"
