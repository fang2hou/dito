"""Linear heads on hidden states: logistic regression, its cross-validated settings, the
principal-direction reduction in front of it, and the second heads for rewrite and translate
arguments. numpy only, so a fitted head folds into one affine map a Rust program applies with one
dot product."""

from dataclasses import dataclass
from typing import Any

import numpy as np

from ..data import SLOT_LABELS, IntentCase
from ..probe.runs import Run
from ..stats import fit_temperature, metrics, nll, softmax

type Parts = list[tuple[int, str]]

RANK = 256
L2_GRID = (1e-3, 1e-2, 1e-1, 1.0)
FOLDS = 5


@dataclass(frozen=True, slots=True)
class Head:
    mean: np.ndarray
    scale: np.ndarray
    weights: np.ndarray  # [d, classes]
    bias: np.ndarray
    temperature: float = 1.0

    def scores(self, x: np.ndarray) -> np.ndarray:
        return ((x - self.mean) / self.scale) @ self.weights + self.bias

    def probabilities(self, x: np.ndarray) -> np.ndarray:
        return softmax(self.scores(x) / self.temperature)


def features(run: Run, parts: Parts) -> np.ndarray:
    """The hidden states of the given (layer, pool) parts, side by side."""
    return np.concatenate([run.hidden(layer, pool) for layer, pool in parts], axis=1)


def fit_logistic(x: np.ndarray, y: np.ndarray, classes: int, l2: float, steps: int = 300) -> Head:
    """Softmax regression: full-batch Adam, standardized inputs, class-balanced loss."""
    mean, scale = x.mean(axis=0), x.std(axis=0) + 1e-6
    z = (x - mean) / scale
    onehot = np.eye(classes)[y]
    counts = np.maximum(onehot.sum(axis=0), 1)
    sample_w = (len(y) / (classes * counts))[y][:, None]
    w, b = np.zeros((z.shape[1], classes)), np.zeros(classes)
    mw, vw, mb, vb = np.zeros_like(w), np.zeros_like(w), np.zeros_like(b), np.zeros_like(b)
    for step in range(1, steps + 1):
        residual = (softmax(z @ w + b) - onehot) * sample_w
        gw, gb = z.T @ residual / len(y) + l2 * w, residual.mean(axis=0)
        for param, grad, m, v in ((w, gw, mw, vw), (b, gb, mb, vb)):
            m *= 0.9
            m += 0.1 * grad
            v *= 0.999
            v += 0.001 * grad**2
            param -= 0.05 * (m / (1 - 0.9**step)) / (np.sqrt(v / (1 - 0.999**step)) + 1e-8)
    return Head(mean, scale, w, b)


def folds(groups: list[str], k: int, seed: int = 7) -> list[np.ndarray]:
    """`k` folds that spread every group over the folds as evenly as its size allows."""
    rng = np.random.default_rng(seed)
    assignment, offset = np.empty(len(groups), dtype=int), 0
    for group in sorted(set(groups)):
        members = rng.permutation([i for i, g in enumerate(groups) if g == group])
        assignment[members] = (offset + np.arange(len(members))) % k
        offset += len(members)
    return [np.flatnonzero(assignment == f) for f in range(k)]


def groups_of(cases: list[IntentCase]) -> list[str]:
    return [f"{c.lang}:{c.intent}" for c in cases]


def fit_head(
    x: np.ndarray, y: np.ndarray, groups: list[str], classes: int
) -> tuple[Head, float, np.ndarray]:
    """Choose L2 and the temperature by 5-fold cross-validation inside the training data, then
    refit on all of it. Returns the head, the chosen L2 and the out-of-fold training
    probabilities (each case scored by a head that never saw it)."""
    split = folds(groups, FOLDS)
    best = None
    for l2 in L2_GRID:
        scores = np.zeros((len(y), classes))
        for held in split:
            rest = np.setdiff1d(np.arange(len(y)), held)
            scores[held] = fit_logistic(x[rest], y[rest], classes, l2).scores(x[held])
        temperature = fit_temperature(scores, y)
        loss = nll(softmax(scores / temperature), y)
        if best is None or loss < best[0]:
            best = (loss, l2, temperature, softmax(scores / temperature))
    assert best is not None
    _, l2, temperature, oof = best
    head = fit_logistic(x, y, classes, l2)
    return Head(head.mean, head.scale, head.weights, head.bias, temperature), l2, oof


def reduce(train: np.ndarray, rank: int = RANK) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Standardize, then keep the top principal directions of the training matrix. Returns
    (mean, scale, projection).

    With hundreds of cases and thousands of dimensions a linear head memorizes the training set
    unless it is limited. The rank cap bounds the directions the head can fit to `rank` and to a
    quarter of the cases.
    """
    mean, scale = train.mean(axis=0), train.std(axis=0) + 1e-6
    _, _, vt = np.linalg.svd((train - mean) / scale, full_matrices=False)
    return mean, scale, vt[: min(rank, len(train) // 4)].T


class Projected:
    """A head fitted in the reduced space, foldable into one affine map on the raw hidden state."""

    def __init__(self, x: np.ndarray, y: np.ndarray, groups: list[str], classes: int) -> None:
        self.mean, self.scale, self.projection = reduce(x)
        self.head, self.l2, self.oof = fit_head(self._z(x), y, groups, classes)

    def _z(self, x: np.ndarray) -> np.ndarray:
        return ((x - self.mean) / self.scale) @ self.projection

    def probabilities(self, x: np.ndarray) -> np.ndarray:
        return self.head.probabilities(self._z(x))

    def folded(self) -> Head:
        inner = self.head
        weights = self.projection @ (inner.weights / inner.scale[:, None])
        bias = inner.bias - (inner.mean / inner.scale) @ inner.weights
        return Head(self.mean, self.scale, weights, bias, inner.temperature)


@dataclass(slots=True)
class SlotResult:
    """The head for the argument of one tool, fitted on that intent's cases only."""

    task: str
    parts: Parts
    head: Projected
    report: dict[str, Any]


def slot_task(
    train: Run, test: Run, intent: str, labels: tuple[str, ...], candidates: Parts
) -> SlotResult:
    """A second head on the same pass: the argument of the chosen tool. It picks its own layer and
    pooling among `candidates` by out-of-fold training loss."""
    key = SLOT_LABELS[intent][0]

    def subset(run: Run, layer: int, pool: str) -> tuple[np.ndarray, np.ndarray, list[str]]:
        index = np.asarray([i for i, c in enumerate(run.cases) if c.intent == intent], dtype=int)
        y = np.asarray([labels.index(run.cases[i].slots[key]) for i in index])
        groups = [f"{run.cases[i].lang}:{run.cases[i].slots[key]}" for i in index]
        return run.hidden(layer, pool)[index], y, groups

    fitted = []
    for layer, pool in candidates:
        x, y, groups = subset(train, layer, pool)
        head = Projected(x, y, groups, len(labels))
        fitted.append((nll(head.oof, y), layer, pool, head, y))
    _, layer, pool, head, y = min(fitted, key=lambda f: f[0])
    tx, ty, _ = subset(test, layer, pool)
    majority = float((ty == np.bincount(y, minlength=len(labels)).argmax()).mean())
    report = {
        "task": f"{intent}.{key}",
        "classes": len(labels),
        "train": len(y),
        "test": len(ty),
        "layer": layer,
        "pool": pool,
        "majority": majority,
        **metrics(head.probabilities(tx), ty),
    }
    return SlotResult(f"{intent}.{key}", [(layer, pool)], head, report)
