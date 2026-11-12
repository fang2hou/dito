"""Round 1 against the current round: the same router prompt, heads fitted on the training set of
each round, scored on the same test cases.

Round 1 is stored compactly in `results/intent/<chip>/round1/<model>.json` (the selected head's
test probabilities and out-of-fold predictions); the current round is the fit report of the
shipped prompt.
"""

import json
from collections.abc import Sequence
from pathlib import Path
from typing import Any

import numpy as np

from ..data import INTENTS, load_cases, load_train
from ..locations import results_dir
from ..models import MODEL_KEYS
from ..prompts import SHIPPED_VARIANT
from ..report.confusion import boundary_counts
from ..report.tables import md, pct
from ..stats import mcnemar, metrics
from ..train.fit_report import read_report


def round1_path(model: str, chip: str | None = None) -> Path:
    return results_dir(chip) / "round1" / f"{model}.json"


def rounds_markdown(models: Sequence[str] = MODEL_KEYS, chip: str | None = None) -> str:
    """Per model: accuracy, auto share and auto accuracy, McNemar of the current round against
    round 1, then per intent and boundary pairs, then the out-of-fold errors of each round."""
    cases = load_cases()
    gold = np.asarray([INTENTS.index(c.intent) for c in cases])
    out = []
    head: list[list[object]] = []
    per_intent: list[list[object]] = []
    boundaries: list[list[object]] = []
    oof_rows: list[list[object]] = []
    for model in models:
        old = json.loads(round1_path(model, chip).read_text(encoding="utf-8"))
        new_report: dict[str, Any] = read_report(model, SHIPPED_VARIANT, chip)
        new = new_report["test_probabilities"]
        if old["test_ids"] != new["ids"] or new["ids"] != [c.id for c in cases]:
            msg = f"{model}: the test cases differ between the rounds"
            raise ValueError(msg)
        p_old, p_new = np.asarray(old["test_selected"]), np.asarray(new["selected"])
        ok_old, ok_new = p_old.argmax(axis=1) == gold, p_new.argmax(axis=1) == gold
        lost, won, p_value = mcnemar(ok_old, ok_new)
        for label, p, extra in (
            ("1", p_old, ["", "", ""]),
            ("2", p_new, [won, lost, f"{p_value:.4f}"]),
        ):
            m = metrics(p, gold)
            head.append(
                [
                    model,
                    label,
                    pct(m["acc"]),
                    pct(m["auto"]),
                    pct(m["auto_acc"]),
                    m["auto_wrong"],
                    *extra,
                ]
            )
        for intent_name in INTENTS:
            rows_i = np.flatnonzero(gold == INTENTS.index(intent_name))
            per_intent.append(
                [
                    model,
                    intent_name,
                    len(rows_i),
                    pct(ok_old[rows_i].mean()),
                    pct(ok_new[rows_i].mean()),
                    int((~ok_old[rows_i] & ok_new[rows_i]).sum()),
                    int((ok_old[rows_i] & ~ok_new[rows_i]).sum()),
                ]
            )
        for (label, sides_old), (_, sides_new) in zip(
            boundary_counts(cases, p_old), boundary_counts(cases, p_new), strict=True
        ):
            for (side, c_old, total), (_, c_new, _) in zip(sides_old, sides_new, strict=True):
                boundaries.append(
                    [model, label, f"{side.name} read as {side.read_as}", total, c_old, c_new]
                )
        train, _ = load_train()
        wrong_new = int(
            sum(
                predicted != INTENTS.index(c.intent)
                for c, predicted in zip(train, new_report["oof_selected"]["predicted"], strict=True)
            )
        )
        wrong_old = sum(g != q for g, q in zip(old["oof_gold"], old["oof_predicted"], strict=True))
        oof_rows.append(
            [
                model,
                f"{wrong_old} of {old['train']} ({pct(1 - wrong_old / old['train'])})",
                f"{wrong_new} of {len(train)} ({pct(1 - wrong_new / len(train))})",
            ]
        )
    columns = ["model", "round", "accuracy", "auto share", "auto accuracy", "auto wrong"]
    out.append(md([*columns, "won", "lost", "McNemar p"], head))
    out.append(
        "\n" + md(["model", "intent", "cases", "round 1", "round 2", "won", "lost"], per_intent)
    )
    out.append(
        "\n" + md(["model", "boundary", "misread side", "cases", "round 1", "round 2"], boundaries)
    )
    out.append(
        "\n"
        + md(
            ["model", "round 1 out-of-fold wrong (right %)", "round 2 out-of-fold wrong (right %)"],
            oof_rows,
        )
    )
    return "\n".join(out)
