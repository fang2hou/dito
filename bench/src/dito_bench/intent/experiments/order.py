"""Option-order debiasing: average the readouts of the same prompt with the options in two orders
(two passes), after Zheng et al. (2024)."""

import numpy as np

from ..probe.runs import load_run
from ..prompts import FIRST_VARIANT, SHIPPED_VARIANT
from ..report.tables import md
from ..stats import mcnemar, metrics, softmax

PAIRS = (
    (FIRST_VARIANT, f"{FIRST_VARIANT}_rev"),
    ("rules", "rules_rev"),
    (SHIPPED_VARIANT, f"{SHIPPED_VARIANT}_rev"),
)
COLUMNS = ["prompt", "forward", "reversed", "both orders", "won", "lost", "p", "auto", "auto_acc"]


def order_markdown(key: str, split: str = "test", chip: str | None = None) -> str:
    """Per prompt: accuracy with the options forward, reversed, and both readouts averaged in log
    space, with cases won and lost by averaging against the forward order alone."""
    rows = []
    for forward, backward in PAIRS:
        a = load_run(key, forward, split, chip=chip)
        b = load_run(key, backward, split, chip=chip)
        pa, pb = softmax(a.logits), softmax(b.logits)
        both = softmax((np.log(pa) + np.log(pb)) / 2)
        y = a.labels
        lost, won, p = mcnemar(pa.argmax(axis=1) == y, both.argmax(axis=1) == y)
        m = metrics(both, y)
        rows.append(
            [
                forward,
                f"{metrics(pa, y)['acc']:.3f}",
                f"{metrics(pb, y)['acc']:.3f}",
                f"{m['acc']:.3f}",
                won,
                lost,
                f"{p:.3f}",
                f"{m['auto']:.3f}",
                f"{m['auto_acc']:.3f}",
            ]
        )
    return md(COLUMNS, rows)
