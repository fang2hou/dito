"""The training cases the selected head gets wrong out-of-fold, so the data can be improved: every
case is scored by a head that never saw it, and the misses are listed by gold and predicted
intent."""

from collections import Counter
from collections.abc import Sequence

from ..data import INTENTS, TAGS, load_train
from ..models import MODEL_KEYS
from ..prompts import SHIPPED_VARIANT
from ..train.fit_report import read_report
from .tables import md, pct


def oof_markdown(
    models: Sequence[str] = MODEL_KEYS, variant: str = SHIPPED_VARIANT, chip: str | None = None
) -> str:
    train, _ = load_train()
    out = []
    for model in models:
        stored = read_report(model, variant, chip)["oof_selected"]
        if stored["ids"] != [c.id for c in train]:
            msg = f"{model}: the training cases changed since this fit"
            raise ValueError(msg)
        wrong = [
            (case, INTENTS[stored["predicted"][i]], stored["confidence"][i], stored["gold"][i])
            for i, case in enumerate(train)
            if INTENTS[stored["predicted"][i]] != case.intent
        ]
        out.append(
            f"## {model} {variant}: {len(wrong)} of {len(train)} wrong out-of-fold "
            f"({pct(1 - len(wrong) / len(train))} accurate)\n"
        )
        pairs = Counter((case.intent, predicted) for case, predicted, _, _ in wrong)
        rows = [[gold, predicted, n] for (gold, predicted), n in pairs.most_common()]
        out.append(md(["gold", "predicted", "cases"], rows))
        per_tag = []
        for tag in TAGS:
            tagged = [c for c in train if tag in c.tags]
            missed = sum(tag in case.tags for case, *_ in wrong)
            per_tag.append([tag, len(tagged), missed, pct(missed / len(tagged)) if tagged else ""])
        out.append("\n" + md(["tag", "cases", "wrong", "wrong share"], per_tag))
        rows = [
            [c.id, c.intent, predicted, f"{gold_p:.2f}", f"{conf:.2f}", ", ".join(c.tags), c.text]
            for c, predicted, conf, gold_p in sorted(wrong, key=lambda w: (w[0].intent, w[1]))
        ]
        out.append(
            "\n" + md(["id", "gold", "predicted", "p(gold)", "p(top)", "tags", "text"], rows) + "\n"
        )
    return "\n".join(out)
