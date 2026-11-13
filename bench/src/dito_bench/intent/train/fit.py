"""Fit decision heads on the training split and score them on the frozen test split.

Everything a head needs is chosen on training data only (layer, pooling, L2, temperature, mixing
weight), by out-of-fold loss: the loss of each training case under heads that never saw it. The
test split is read once per configuration to report it.

Two pools are read from the hidden states: `last`, the residual stream at the answer position,
and `span`, its mean over the instruction tokens.

The selected head is what `export.py` writes for the app. The rest of the fit report (baselines,
layer scan, out-of-fold and test probabilities) is stored by `fit_report.py` for the tables.
"""

from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

import numpy as np

from ..data import INTENTS, LANGS, SLOT_LABELS, TAGS
from ..probe.runs import FIT_MODE, Run, load_run
from ..stats import fit_temperature, mcnemar, metrics, mix, nll, softmax
from .asset import DirectMix, NamedHead
from .fit_report import intent_key
from .heads import Parts, Projected, features, fit_head, groups_of, slot_task

POOLS = ("last", "span")
SELECTED = "SELECTED by out-of-fold loss: "
SLOT_HEADS = tuple((intent, labels) for intent, (_, labels) in SLOT_LABELS.items())
# Heads of these slot tasks go into the app; `translate.target` does not (123 training cases do
# not make it reliable, and the translate tool reads the instruction itself).
EXPORTED_SLOTS = ("rewrite.op",)


@dataclass(slots=True)
class Candidate:
    """A way to turn the stored features into probabilities, with the out-of-fold training loss
    that is the only thing the final choice may look at. `head` is None for the mix of the head
    with the option logits."""

    name: str
    p: np.ndarray
    loss: float
    oof: np.ndarray
    head: Projected | None = None
    parts: Parts | None = None


@dataclass(slots=True)
class Fit:
    """The report of one model and prompt variant, and the heads it would ship."""

    key: str
    variant: str
    report: dict[str, Any]
    intent: NamedHead
    direct: DirectMix | None
    slots: list[NamedHead]


def breakdown(run: Run, correct: np.ndarray) -> dict[str, float]:
    """Accuracy per language, per tag and per gold intent (NaN for a slice without cases)."""

    def share(mask: list[bool]) -> float:
        picked = correct[np.asarray(mask, dtype=bool)]
        return float(picked.mean()) if len(picked) else float("nan")

    out = {lang: share([c.lang == lang for c in run.cases]) for lang in LANGS}
    for tag in TAGS:
        out[tag] = share([tag in c.tags for c in run.cases])
    for intent in INTENTS:
        out[intent_key(intent)] = share([c.intent == intent for c in run.cases])
    return out


def row(
    name: str,
    p: np.ndarray,
    run: Run,
    baseline: np.ndarray | None = None,
    **extra: float,
) -> dict[str, Any]:
    """Test scores of one method, with cases won and lost against `baseline` (a boolean vector of
    correct answers) and the exact McNemar p-value when given."""
    correct = p.argmax(axis=1) == run.labels
    out: dict[str, Any] = {
        "method": name,
        **metrics(p, run.labels),
        **breakdown(run, correct),
        **extra,
    }
    if baseline is not None:
        lost, won, pvalue = mcnemar(baseline, correct)
        out |= {"won": won, "lost": lost, "p": pvalue}
    return out


def _scan(train: Run, test: Run, groups: list[str]) -> list[dict[str, Any]]:
    """One head per layer and pooling, each with its out-of-fold loss and its test accuracy."""
    scan = []
    for layer in train.meta["layers"]:
        for pool in POOLS:
            head = Projected(train.hidden(layer, pool), train.labels, groups, len(INTENTS))
            p = head.probabilities(test.hidden(layer, pool))
            scan.append(
                {
                    "layer": layer,
                    "pool": pool,
                    "oof_nll": nll(head.oof, train.labels),
                    "oof_acc": float((head.oof.argmax(axis=1) == train.labels).mean()),
                    "test_acc": float((p.argmax(axis=1) == test.labels).mean()),
                    "head": head,
                    "p": p,
                }
            )
    return scan


def _candidates(
    train: Run, test: Run, groups: list[str], scan: list[dict[str, Any]], temperature: float
) -> tuple[list[Candidate], dict[str, Any], float]:
    """Every head candidate. Returns them, the best single-layer scan entry and the weight of the
    mixed candidate."""
    y = train.labels
    layers = train.meta["layers"]
    last_layer = max(layers)
    best = min(scan, key=lambda s: s["oof_nll"])
    best_of = {
        pool: min((s for s in scan if s["pool"] == pool), key=lambda s: s["oof_nll"])
        for pool in POOLS
    }

    out = [
        Candidate(
            f"hidden head, {pool}, layer {s['layer']}",
            s["p"],
            s["oof_nll"],
            s["head"].oof,
            s["head"],
            [(s["layer"], pool)],
        )
        for pool, s in best_of.items()
    ]
    final = next(s for s in scan if s["layer"] == last_layer and s["pool"] == "last")
    out.append(
        Candidate(
            f"hidden head, last, final layer {last_layer}",
            final["p"],
            final["oof_nll"],
            final["head"].oof,
            final["head"],
            [(last_layer, "last")],
        )
    )
    for name, parts in (
        (
            f"head on last + span, layer {best['layer']}",
            [(best["layer"], "last"), (best["layer"], "span")],
        ),
        (
            f"head on last@{best_of['last']['layer']} + span@{best_of['span']['layer']}",
            [(best_of["last"]["layer"], "last"), (best_of["span"]["layer"], "span")],
        ),
        (
            f"head on layer {best['layer']} + final layer",
            [(best["layer"], best["pool"]), (last_layer, "last")],
        ),
    ):
        head = Projected(features(train, parts), y, groups, len(INTENTS))
        out.append(
            Candidate(
                name,
                head.probabilities(features(test, parts)),
                nll(head.oof, y),
                head.oof,
                head,
                parts,
            )
        )

    # Two readouts of one pass, mixed in log space: the best single head and the option logits
    # scaled by the fitted temperature.
    calibrated_train = softmax(train.logits / temperature)
    weight = min(
        np.linspace(0, 1, 11), key=lambda w: nll(mix(best["head"].oof, calibrated_train, w), y)
    )
    weight = round(float(weight), 6)
    mixed = mix(best["p"], softmax(test.logits / temperature), weight)
    oof = mix(best["head"].oof, calibrated_train, weight)
    out.append(Candidate(f"head + direct (weight {weight:.1f})", mixed, nll(oof, y), oof))
    return out, best, weight


def fit_variant(
    key: str,
    variant: str,
    *,
    chip: str | None = None,
    mode: str = FIT_MODE,
    log: Callable[[str], None] = print,
) -> Fit:
    """Fit every candidate on the training run of a model and variant, score them on the test run,
    and pick the one with the lowest out-of-fold loss."""
    train, test = (load_run(key, variant, s, mode, chip) for s in ("train", "test"))
    y, ty, groups = train.labels, test.labels, groups_of(train.cases)
    direct_train, direct = softmax(train.logits), softmax(test.logits)
    base_correct = direct.argmax(axis=1) == ty
    rows = [row("direct", direct, test)]

    temperature = fit_temperature(train.logits, y)
    rows.append(
        row(
            "direct + temperature",
            softmax(test.logits / temperature),
            test,
            base_correct,
            T=temperature,
        )
    )
    # Batch calibration (Zhou et al., 2024): remove the mean log-probability of unlabeled inputs.
    prior = np.log(direct_train).mean(axis=0)
    bc_t = fit_temperature(np.log(direct_train) - prior, y)
    rows.append(
        row(
            "direct + batch calibration",
            softmax((np.log(direct) - prior) / bc_t),
            test,
            base_correct,
        )
    )
    logit_head, _, _ = fit_head(train.logits, y, groups, len(INTENTS))
    rows.append(
        row(
            f"logit head ({len(INTENTS)} inputs)",
            logit_head.probabilities(test.logits),
            test,
            base_correct,
        )
    )

    log(f"{key} {variant}: scanning {len(train.meta['layers'])} layers")
    scan = _scan(train, test, groups)
    candidates, best, weight = _candidates(train, test, groups, scan, temperature)
    rows.extend(row(c.name, c.p, test, base_correct, oof_nll=c.loss) for c in candidates)
    chosen = min(candidates, key=lambda c: c.loss)
    rows.append(row(SELECTED + chosen.name, chosen.p, test, base_correct, oof_nll=chosen.loss))

    # What the app loads. The mixed candidate is the best single head plus the option-logit
    # readout; every other candidate is a head alone.
    if chosen.head is None:
        exported = (best["head"], [(best["layer"], best["pool"])])
        direct_mix = DirectMix(weight, temperature)
    else:
        assert chosen.parts is not None
        exported, direct_mix = (chosen.head, chosen.parts), None
    intent = NamedHead("intent", INTENTS, exported[1], exported[0].folded())

    best_span = min((s for s in scan if s["pool"] == "span"), key=lambda s: s["oof_nll"])
    last_layer = max(train.meta["layers"])
    slot_candidates: Parts = [
        (best["layer"], "last"),
        (best["layer"], "span"),
        (best_span["layer"], "span"),
        (last_layer, "last"),
    ]
    slots = [
        slot_task(train, test, intent_name, labels, slot_candidates)
        for intent_name, labels in SLOT_HEADS
    ]

    report = {
        "model": key,
        "variant": variant,
        "train": len(y),
        "test": len(ty),
        "rows": rows,
        "scan": [{k: v for k, v in s.items() if k not in ("head", "p")} for s in scan],
        "chosen": {
            "layer": best["layer"],
            "pool": best["pool"],
            "mix_weight": weight,
            "selected": chosen.name,
            "exported": {
                "parts": [list(p) for p in exported[1]],
                "direct_weight": None if direct_mix is None else direct_mix.weight,
            },
        },
        "slots": [s.report for s in slots],
        "oof_selected": {
            "ids": [c.id for c in train.cases],
            "predicted": chosen.oof.argmax(axis=1).tolist(),
            "confidence": chosen.oof.max(axis=1).round(4).tolist(),
            "gold": chosen.oof[np.arange(len(y)), y].round(4).tolist(),
        },
        "test_probabilities": {
            "ids": [c.id for c in test.cases],
            "head": best["p"].round(4).tolist(),
            "selected": chosen.p.round(4).tolist(),
            "direct": direct.round(4).tolist(),
            "mixed": next(c.p for c in candidates if c.head is None).round(4).tolist(),
        },
    }
    shipped_slots = [
        NamedHead(s.task, labels, s.parts, s.head.folded())
        for s, (_, labels) in zip(slots, SLOT_HEADS, strict=True)
        if s.task in EXPORTED_SLOTS
    ]
    return Fit(key, variant, report, intent, direct_mix, shipped_slots)
