"""The tables of the intent report, from the fit reports, the stored runs and `production.json`."""

from collections.abc import Sequence

from ..data import INTENTS, TAGS, load_cases, load_train
from ..models import MODEL_KEYS
from ..probe.runs import FIT_MODE, load_run
from ..prompts import FIRST_VARIANT, SHIPPED_VARIANT
from ..stats import mcnemar, softmax
from ..train.fit_report import intent_key, pick, read_report
from ..train.parity import read_production
from .confusion import confusion_markdown
from .tables import md, pct

STAGES = (
    ("direct readout, first prompt", FIRST_VARIANT, "direct"),
    ("direct readout, built prompt", SHIPPED_VARIANT, "direct"),
    ("head, first prompt", FIRST_VARIANT, "SELECTED"),
    ("head, built prompt", SHIPPED_VARIANT, "SELECTED"),
)


def summary_markdown(models: Sequence[str] = MODEL_KEYS, chip: str | None = None) -> str:
    reports = {
        (m, v): read_report(m, v, chip) for m in models for v in (FIRST_VARIANT, SHIPPED_VARIANT)
    }
    train, dropped = load_train()
    out = [f"train {len(train)} (dropped {dropped}), test {len(load_cases())}\n"]

    out.append("### headline\n")
    columns = ["model", "stage", "accuracy", "macro F1", "ECE", "auto share", "auto accuracy"]
    columns += ["auto wrong", "zh", "ja", "en"]
    rows = []
    for m in models:
        for label, variant, prefix in STAGES:
            r = pick(reports[m, variant], prefix)
            rows.append(
                [
                    m,
                    label,
                    pct(r["acc"]),
                    f"{r['f1']:.3f}",
                    f"{r['ece']:.3f}",
                    pct(r["auto"]),
                    pct(r["auto_acc"]),
                    r["auto_wrong"],
                    *(pct(r[k]) for k in ("zh", "ja", "en")),
                ]
            )
    out.append(md(columns, rows))

    out.append("\n### per intent (gold intent, test accuracy)\n")
    counts = {i: sum(c.intent == i for c in load_cases()) for i in INTENTS}
    rows = []
    for m in models:
        for label, variant, prefix in STAGES:
            r = pick(reports[m, variant], prefix)
            rows.append([m, label, *(pct(r[intent_key(i)]) for i in INTENTS)])
    out.append(md(["model", "stage", *(f"{i} ({counts[i]})" for i in INTENTS)], rows))

    out.append("\n### per tag (test accuracy)\n")
    tag_counts = {t: sum(t in c.tags for c in load_cases()) for t in TAGS}
    rows = []
    for m in models:
        for label, variant, prefix in STAGES:
            r = pick(reports[m, variant], prefix)
            rows.append([m, label, *(pct(r[t]) for t in TAGS)])
    out.append(md(["model", "stage", *(f"{t} ({tag_counts[t]})" for t in TAGS)], rows))

    out.append("\n### where neighbouring intents are confused (selected head, built prompt)\n")
    out.append(confusion_markdown(models, SHIPPED_VARIANT, chip))

    out.append("\n### what was selected\n")
    columns = ["model", "prompt", "selected by out-of-fold loss", "won", "lost", "McNemar p"]
    columns += ["intent head parts", "direct weight"]
    rows = []
    for m in models:
        for variant in (FIRST_VARIANT, SHIPPED_VARIANT):
            result = reports[m, variant]
            r, exported = pick(result, "SELECTED"), result["chosen"]["exported"]
            rows.append(
                [
                    m,
                    variant,
                    result["chosen"]["selected"],
                    r["won"],
                    r["lost"],
                    f"{r['p']:.4f}",
                    exported["parts"],
                    "" if exported["direct_weight"] is None else f"{exported['direct_weight']:g}",
                ]
            )
    out.append(md(columns, rows))

    out.append("\n### every candidate\n")
    columns = ["method", "out-of-fold loss", "accuracy", "auto share", "auto accuracy", "won"]
    columns += ["lost", "p"]
    for m in models:
        for variant in (FIRST_VARIANT, SHIPPED_VARIANT):
            rows = []
            for r in reports[m, variant]["rows"]:
                loss = f"{r['oof_nll']:.3f}" if "oof_nll" in r else ""
                won = [r["won"], r["lost"], f"{r['p']:.3f}"] if "won" in r else ["", "", ""]
                rows.append(
                    [r["method"], loss, pct(r["acc"]), pct(r["auto"]), pct(r["auto_acc"]), *won]
                )
            out.append(f"\n{m} {variant}\n\n{md(columns, rows)}")

    out.append(
        "\n### prompt effect on the direct readout (test), exact McNemar against the first prompt\n"
    )
    rows = []
    for m in models:
        a = load_run(m, FIRST_VARIANT, "test", FIT_MODE, chip)
        b = load_run(m, SHIPPED_VARIANT, "test", FIT_MODE, chip)
        ca = softmax(a.logits).argmax(axis=1) == a.labels
        cb = softmax(b.logits).argmax(axis=1) == b.labels
        lost, won, p = mcnemar(ca, cb)
        rows.append([m, pct(ca.mean()), pct(cb.mean()), won, lost, f"{p:.4f}"])
    out.append(md(["model", "first prompt", "built prompt", "won", "lost", "p"], rows))

    out.append("\n### slot heads\n")
    columns = ["model", "prompt", "task", "classes", "train", "test", "layer", "pool", "majority"]
    rows = []
    for m in models:
        for variant in (FIRST_VARIANT, SHIPPED_VARIANT):
            for s in reports[m, variant]["slots"]:
                rows.append(
                    [
                        m,
                        variant,
                        s["task"],
                        s["classes"],
                        s["train"],
                        s["test"],
                        s["layer"],
                        s["pool"],
                        pct(s["majority"]),
                        pct(s["acc"]),
                    ]
                )
    out.append(md([*columns, "accuracy"], rows))

    out.append("\n### learning curve (first prompt)\n")
    curves = {m: reports[m, FIRST_VARIANT]["learning_curve"] for m in models}
    rows = [
        [point["train"], *(pct(curves[m][i]["acc"]) for m in models)]
        for i, point in enumerate(curves[models[0]])
    ]
    out.append(md(["training cases", *models], rows))

    out.append("\n### best layer by pooling (first prompt, test accuracy)\n")
    columns = ["model", "layers", "best answer-position layer", "accuracy"]
    columns += ["best instruction-mean layer", "accuracy", "final layer, answer position"]
    rows = []
    for m in models:
        scan = reports[m, FIRST_VARIANT]["scan"]
        best = {
            pool: max((s for s in scan if s["pool"] == pool), key=lambda s: s["test_acc"])
            for pool in ("last", "span")
        }
        final = max((s for s in scan if s["pool"] == "last"), key=lambda s: s["layer"])
        rows.append(
            [
                m,
                final["layer"] + 1,
                best["last"]["layer"],
                pct(best["last"]["test_acc"]),
                best["span"]["layer"],
                pct(best["span"]["test_acc"]),
                pct(final["test_acc"]),
            ]
        )
    out.append(md(columns, rows))

    production = [r for r in read_production(chip) if r["model"] in models]
    if production:
        n_test = len(load_cases())
        out.append(
            f"\n### production path (`intent_decide`, the {n_test} test cases, shipped heads)\n"
        )
        columns = ["model", "prompt tokens", "p50 ms", "p95 ms", "max prob. difference"]
        columns += ["same top answer", "accuracy", "auto share", "auto accuracy", "auto wrong"]
        rows = [
            [
                r["model"],
                "" if r["prompt_tokens"] is None else r["prompt_tokens"],
                f"{r['p50_ms']:.0f}",
                f"{r['p95_ms']:.0f}",
                f"{r['max_abs_diff']:.4f}",
                pct(r["same_top"]),
                pct(r["accuracy"]),
                pct(r["auto"]),
                pct(r["auto_acc"]),
                r["auto_wrong"],
            ]
            for r in production
        ]
        out.append(md(columns, rows))
    return "\n".join(out)
