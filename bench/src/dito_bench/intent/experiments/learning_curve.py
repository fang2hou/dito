"""How much training data a head needs: test accuracy against the number of training cases.

The curve is stored inside the fit report, next to the fit it describes, so `intent fit` adds it
after fitting. Exporting heads does not need it.
"""

from typing import Any

import numpy as np

from ..data import INTENTS
from ..probe.runs import FIT_MODE, Run, load_run
from ..stats import metrics
from ..train.fit import Fit
from ..train.heads import Parts, Projected, features, groups_of

# Training-set sizes of the curve; the whole set is added as the last point.
CURVE_SIZES = (50, 100, 200, 400, 800)


def learning_curve(
    train: Run, test: Run, parts: Parts, sizes: tuple[int, ...]
) -> list[dict[str, Any]]:
    """Test accuracy of the head when only the first `n` training cases of a fixed shuffle are
    used."""
    x, tx = features(train, parts), features(test, parts)
    order = np.random.default_rng(11).permutation(len(train.cases))
    groups = groups_of(train.cases)
    out = []
    for n in sizes:
        keep = order[: min(n, len(order))]
        head = Projected(x[keep], train.labels[keep], [groups[i] for i in keep], len(INTENTS))
        out.append({"train": len(keep), **metrics(head.probabilities(tx), test.labels)})
    return out


def add_learning_curve(fit: Fit, chip: str | None = None, mode: str = FIT_MODE) -> None:
    """Add the curve of the head on both readings of the layer the fit chose to `fit.report`, after
    `slots`, where it has always been."""
    train, test = (load_run(fit.key, fit.variant, split, mode, chip) for split in ("train", "test"))
    layer = fit.report["chosen"]["layer"]
    n = len(train.cases)
    sizes = (*(size for size in CURVE_SIZES if size < n), n)
    curve = learning_curve(train, test, [(layer, "last"), (layer, "span")], sizes)
    report = dict(fit.report)
    fit.report.clear()
    for name, value in report.items():
        fit.report[name] = value
        if name == "slots":
            fit.report["learning_curve"] = curve
