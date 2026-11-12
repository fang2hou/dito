import numpy as np

from dito_bench.intent.stats import ece, fit_temperature, softmax


def test_ece_is_the_gap_between_confidence_and_accuracy() -> None:
    probabilities = np.array([[0.8, 0.2]] * 10)
    labels = np.array([0] * 8 + [1] * 2)
    assert ece(probabilities, labels) == 0.0
    labels = np.array([0] * 5 + [1] * 5)
    assert abs(ece(probabilities, labels) - 0.3) < 1e-9


def test_temperature_undoes_overconfident_scores() -> None:
    rng = np.random.default_rng(0)
    true_scores = rng.normal(size=(4000, 4)) * 2
    probabilities = softmax(true_scores)
    labels = np.array([rng.choice(4, p=p) for p in probabilities])
    assert abs(fit_temperature(true_scores * 3, labels) - 3) < 0.3
