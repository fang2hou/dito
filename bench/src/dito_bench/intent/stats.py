"""Probability scores shared by every fit and table. numpy only."""

from math import comb

import numpy as np

# A decision is taken without asking when its top probability reaches this.
AUTO = 0.9


def softmax(scores: np.ndarray) -> np.ndarray:
    exp = np.exp(scores - scores.max(axis=1, keepdims=True))
    return exp / exp.sum(axis=1, keepdims=True)


def nll(p: np.ndarray, y: np.ndarray) -> float:
    return float(-np.log(np.clip(p[np.arange(len(y)), y], 1e-12, 1)).mean())


def ece(p: np.ndarray, y: np.ndarray, bins: int = 10) -> float:
    """Expected calibration error: the gap between confidence and accuracy, over equal-width
    confidence bins weighted by their share of cases."""
    conf, correct = p.max(axis=1), p.argmax(axis=1) == y
    index = np.minimum((conf * bins).astype(int), bins - 1)
    return float(
        sum(
            (index == b).mean() * abs(correct[index == b].mean() - conf[index == b].mean())
            for b in range(bins)
            if (index == b).any()
        )
    )


def macro_f1(pred: np.ndarray, y: np.ndarray, classes: int) -> float:
    scores = []
    for c in range(classes):
        tp = np.sum((pred == c) & (y == c))
        false = np.sum((pred == c) & (y != c)) + np.sum((pred != c) & (y == c))
        scores.append(0.0 if tp == 0 else 2 * tp / (2 * tp + false))
    return float(np.mean(scores))


def metrics(p: np.ndarray, y: np.ndarray) -> dict[str, float]:
    """Accuracy, macro F1, NLL and ECE, and the `auto` figures: `auto` is the share of cases whose
    top probability reaches `AUTO`, `auto_acc` their accuracy and `auto_wrong` how many of them
    are wrong."""
    pred, conf = p.argmax(axis=1), p.max(axis=1)
    auto = conf >= AUTO
    return {
        "acc": float((pred == y).mean()),
        "f1": macro_f1(pred, y, p.shape[1]),
        "nll": nll(p, y),
        "ece": ece(p, y),
        "auto": float(auto.mean()),
        "auto_acc": float((pred[auto] == y[auto]).mean()) if auto.any() else float("nan"),
        "auto_wrong": int((pred[auto] != y[auto]).sum()),
    }


def fit_temperature(scores: np.ndarray, y: np.ndarray) -> float:
    """The temperature that minimizes the NLL of `softmax(scores / T)`, by golden-section search
    on log T over [-3, 3]."""
    low, high, ratio = -3.0, 3.0, (5**0.5 - 1) / 2

    def loss(t: float) -> float:
        return nll(softmax(scores / np.exp(t)), y)

    for _ in range(60):
        a, b = high - ratio * (high - low), low + ratio * (high - low)
        if loss(a) < loss(b):
            high = b
        else:
            low = a
    return float(np.exp((low + high) / 2))


def mcnemar(a_correct: np.ndarray, b_correct: np.ndarray) -> tuple[int, int, float]:
    """Cases only A gets right, cases only B gets right, and the exact two-sided p-value."""
    only_a, only_b = int((a_correct & ~b_correct).sum()), int((~a_correct & b_correct).sum())
    n = only_a + only_b
    if n == 0:
        return 0, 0, 1.0
    tail = sum(comb(n, k) for k in range(min(only_a, only_b) + 1)) / 2**n
    return only_a, only_b, min(1.0, 2 * tail)


def mix(a: np.ndarray, b: np.ndarray, weight: float) -> np.ndarray:
    """Log-linear mix of two probability tables, `a ** weight * b ** (1 - weight)`, renormalized."""
    return softmax(
        weight * np.log(np.clip(a, 1e-9, 1)) + (1 - weight) * np.log(np.clip(b, 1e-9, 1))
    )
