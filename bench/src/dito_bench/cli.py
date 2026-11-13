"""Command line entry point: `dito-bench`."""

import argparse
import json
from collections.abc import Callable, Mapping, Sequence
from pathlib import Path
from typing import Any

from . import env, table
from .asr import benchmark as asr_benchmark
from .asr.registry import ASR_MODELS, default_asr_keys
from .clips import ensure_clips
from .polish import benchmark as polish_benchmark
from .polish.registry import POLISH_MODELS, default_polish_keys
from .store import ResultStore

ASR_COLUMNS = [
    "model",
    "clip",
    "stale",
    "detected",
    "metric",
    "error",
    "elapsed_s",
    "x_realtime",
    "text",
]
ASR_SUMMARY_COLUMNS = [
    "model",
    "metric",
    "mean_error",
    "mean_elapsed_s",
    "mean_x_realtime",
    "cases",
]
POLISH_COLUMNS = ["model", "case", "stale", "elapsed_s", "tok_s", "output"]


def _model_list(
    task: str, specs: Mapping[str, Any], defaults: Sequence[str], chip: str | None
) -> int:
    chip_slug = env.chip_slug(chip)
    rows = []
    for key, spec in specs.items():
        rows.append(
            {
                "key": key,
                "label": spec.label,
                "runtime": spec.runtime,
                "default": "yes" if key in defaults else "",
                "weights": "downloaded" if spec.weights.identity() else "missing",
                "recorded": len(ResultStore.open(task, chip_slug, key).records),
            }
        )
    print(table.render(rows, ["key", "label", "runtime", "default", "weights", "recorded"]))
    return 0


def _download(specs: Mapping[str, Any], defaults: Sequence[str], keys: Sequence[str] | None) -> int:
    for key in keys or defaults:
        if key not in specs:
            msg = f"unknown model {key!r}; available: {', '.join(sorted(specs))}"
            raise SystemExit(msg)
        print(f"[{key}] {specs[key].weights.repo}")
        print(f"  -> {specs[key].weights.fetch()}")
    return 0


def _emit(
    rows: Sequence[Mapping[str, Any]], columns: Sequence[str], args: argparse.Namespace
) -> None:
    if args.json:
        print(json.dumps(list(rows), ensure_ascii=False, indent=2))
    else:
        print(table.render(rows, columns, markdown=args.markdown))


def _add_model_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "-m", "--models", nargs="+", metavar="KEY", help="model keys (default: the default set)"
    )
    parser.add_argument(
        "--chip", help='chip name to use instead of this machine, e.g. "Apple M4 Pro"'
    )


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="dito-bench", description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    clips = commands.add_parser("clips", help="synthesize the benchmark clips with `say`")
    clips.add_argument("--force", action="store_true", help="regenerate clips that already exist")

    record = commands.add_parser("record", help="record from the microphone into recordings/")
    record.add_argument("--seconds", type=float, help="fixed duration; omit to stop with Enter")
    record.add_argument("--note", default="", help="free-text note stored in the JSON sidecar")

    for task in ("asr", "polish"):
        sub = commands.add_parser(task, help=f"{task} models").add_subparsers(
            dest="action", required=True
        )
        listing = sub.add_parser("list", help="show known models, weights and recorded results")
        listing.add_argument("--chip")
        download = sub.add_parser("download", help="download weights")
        download.add_argument("models", nargs="*", metavar="KEY", help="default: the default set")
        run = sub.add_parser("run", help="run only the cases without a fresh stored result")
        _add_model_args(run)
        run.add_argument("--refresh", action="store_true", help="rerun every case")
        report = sub.add_parser("report", help="print stored results without running anything")
        _add_model_args(report)
        report.add_argument("--markdown", action="store_true")
        report.add_argument("--json", action="store_true")
        if task == "asr":
            compare = sub.add_parser("compare", help="transcribe one file with several models")
            compare.add_argument("audio", type=Path)
            compare.add_argument("-m", "--models", nargs="+", metavar="KEY")
            compare.add_argument("--hotwords", nargs="+", default=[])

    _add_intent_commands(commands)
    return parser


def _add_intent_commands(commands: argparse._SubParsersAction[argparse.ArgumentParser]) -> None:
    from .intent.probe.runs import FIT_MODE
    from .intent.prompts import FIRST_VARIANT, SHIPPED_VARIANT

    intent = commands.add_parser("intent", help="voice command intent routing").add_subparsers(
        dest="action", required=True
    )

    def add(name: str, help_text: str) -> argparse.ArgumentParser:
        parser = intent.add_parser(name, help=help_text)
        _add_model_args(parser)
        return parser

    run = add("run", "probe the model on a split; runs that exist are skipped")
    run.add_argument("-v", "--variants", nargs="+", default=[FIRST_VARIANT], metavar="NAME")
    run.add_argument("-s", "--splits", nargs="+", default=["test"], choices=["train", "test"])
    run.add_argument("--layers", default="none", help="none, all or layer numbers like 14,15")
    run.add_argument("--full", action="store_true", help="decode the whole prompt every time")
    run.add_argument("--generate", choices=["letter", "json"], help="also generate the answer")
    run.add_argument("--tag", default="", help="suffix for the run name")
    run.add_argument("--gguf", type=Path, help="GGUF file to probe (one -m)")

    sweep = add("sweep", "direct readout by prompt variant (default model: qwen3.5-4b)")
    sweep.add_argument("-s", "--splits", nargs="+", default=["test"], choices=["train", "test"])
    sweep.add_argument("--mode", default="cached")

    fit = add("fit", "fit the heads on stored hidden states and write the fit reports")
    fit.add_argument("-v", "--variants", nargs="+", default=[FIRST_VARIANT, SHIPPED_VARIANT])
    fit.add_argument("--mode", default=FIT_MODE)

    add("export", "fit the shipped prompt and write the .heads asset the app bundles")

    # `-m` takes one key per use here, so the instructions after it are not read as keys.
    decide = intent.add_parser("decide", help="decide instructions through the app path")
    decide.add_argument("-m", "--models", action="append", metavar="KEY", help="repeat for several")
    decide.add_argument("--chip", help="chip name to use instead of this machine")
    decide.add_argument("texts", nargs="+", metavar="TEXT", help="instructions to decide")

    add("parity", "run the test cases through the app path and compare it with numpy")

    efficiency = add("efficiency", "readout versus generation: accuracy and time")
    efficiency.add_argument("-v", "--variant", default=FIRST_VARIANT)
    efficiency.add_argument("--json", action="store_true")

    add("order", "average the readouts of two option orders (default model: qwen3.5-4b)")

    errors = add("errors", "list the test cases a head gets wrong")
    errors.add_argument("-v", "--variant", default=SHIPPED_VARIANT)
    errors.add_argument("--parts", help="layer:pool list like 24:last,19:span (default: exported)")
    errors.add_argument("--mode", default=FIT_MODE)

    add("confusion", "boundary pairs and confusion matrix of the selected heads on the test cases")
    add("oof", "training cases the selected head gets wrong out-of-fold")
    add("rounds", "round 1 against the current round: accuracy, intents, boundaries, McNemar")
    add("summary", "print the tables of the report")
    add("charts", "write the SVG charts (needs the charts extra)")

    everything = add("all", "regenerate every artifact in the results folder")
    everything.add_argument("--sweep-model", default="qwen3.5-4b", metavar="KEY")


def _asr(args: argparse.Namespace) -> int:
    keys = args.models if getattr(args, "models", None) else default_asr_keys()
    match args.action:
        case "list":
            return _model_list("asr", ASR_MODELS, default_asr_keys(), args.chip)
        case "download":
            return _download(ASR_MODELS, default_asr_keys(), args.models)
        case "run":
            asr_benchmark.run(keys, refresh=args.refresh, chip=args.chip)
        case "report":
            rows, warnings = asr_benchmark.report_rows(keys, chip=args.chip)
            _emit(rows, ASR_COLUMNS, args)
            if not args.json:
                print()
                print(
                    table.render(
                        asr_benchmark.summarize(rows), ASR_SUMMARY_COLUMNS, markdown=args.markdown
                    )
                )
            for warning in warnings:
                print(f"warning: {warning}")
        case "compare":
            rows = asr_benchmark.compare_file(args.audio, keys, args.hotwords)
            print(table.render(rows, ["model", "detected", "elapsed_s", "x_realtime", "text"]))
    return 0


def _polish(args: argparse.Namespace) -> int:
    keys = args.models if getattr(args, "models", None) else default_polish_keys()
    match args.action:
        case "list":
            return _model_list("polish", POLISH_MODELS, default_polish_keys(), args.chip)
        case "download":
            return _download(POLISH_MODELS, default_polish_keys(), args.models)
        case "run":
            polish_benchmark.run(keys, refresh=args.refresh, chip=args.chip)
        case "report":
            rows, warnings = polish_benchmark.report_rows(keys, chip=args.chip)
            _emit(rows, POLISH_COLUMNS, args)
            for warning in warnings:
                print(f"warning: {warning}")
    return 0


def _intent(args: argparse.Namespace) -> int:
    try:
        return _intent_action(args)
    except FileNotFoundError as error:
        raise SystemExit(str(error)) from error


def _intent_action(args: argparse.Namespace) -> int:
    from .intent import pipeline
    from .intent.experiments import efficiency, errors, order, rounds, sweep
    from .intent.models import MODEL_KEYS, catalog_id
    from .intent.probe import runner
    from .intent.prompts import SHIPPED_VARIANT, VARIANTS
    from .intent.report import charts, confusion, oof, summary, tables
    from .intent.train import export, fit, fit_report, parity

    chip = args.chip
    # The prompt sweep and its option-order check were recorded for one model only.
    single = [sweep.SWEEP_MODEL]
    keys = args.models or (single if args.action in ("sweep", "order") else list(MODEL_KEYS))
    for key in keys:
        catalog_id(key)
    for variant in getattr(args, "variants", None) or [getattr(args, "variant", SHIPPED_VARIANT)]:
        if variant not in VARIANTS:
            msg = f"unknown variant {variant!r}; available: {', '.join(VARIANTS)}"
            raise SystemExit(msg)
    match args.action:
        case "run":
            runner.run_probes(
                keys,
                args.variants,
                args.splits,
                chip=chip,
                layers=args.layers,
                full=args.full,
                generate=args.generate,
                tag=args.tag,
                gguf=args.gguf,
                log=print,
            )
        case "sweep":
            for key in keys:
                print(sweep.sweep_markdown(key, args.splits, args.mode, chip))
        case "fit":
            for key in keys:
                for variant in args.variants:
                    result = pipeline.fit_with_curve(
                        key, variant, chip=chip, mode=args.mode, log=print
                    )
                    print(f"\n{tables.fit_markdown(result.report)}")
                    print(f"\nwrote {fit_report.write_report(result, chip)}")
        case "export":
            for key in keys:
                result = fit.fit_variant(key, SHIPPED_VARIANT, chip=chip, log=print)
                written = export.export_heads(result, chip)
                weight = written["direct_weight"]
                direct = "off" if weight is None else f"weight {weight}"
                print(
                    f"{written['path']}: {written['bytes']} bytes, "
                    f"intent parts {written['parts']}, direct {direct}"
                )
        case "decide":
            columns = ["model", "text", "intent", "confidence", "auto", "rewrite op", "ms"]
            print(table.render(parity.decide_rows(keys, args.texts), columns))
        case "parity":
            rows = parity.parity_rows(keys, chip=chip, log=print)
            print(table.render([_parity_display(r) for r in rows], list(_PARITY_COLUMNS.values())))
            print(f"wrote {parity.write_production(rows, chip)}")
        case "efficiency":
            rows = efficiency.rows(keys, args.variant, chip)
            if args.json:
                print(json.dumps(rows, indent=1))
            else:
                print(efficiency.efficiency_markdown(rows))
        case "order":
            for key in keys:
                print(order.order_markdown(key, chip=chip))
        case "errors":
            for key in keys:
                if args.parts:
                    parts = errors.parse_parts(args.parts)
                else:
                    report = fit_report.read_report(key, args.variant, chip)
                    parts = [(layer, pool) for layer, pool in report["chosen"]["exported"]["parts"]]
                print(errors.errors_markdown(key, args.variant, parts, args.mode, chip))
        case "confusion":
            print(confusion.confusion_markdown(keys, SHIPPED_VARIANT, chip))
        case "rounds":
            print(rounds.rounds_markdown(keys, chip))
        case "oof":
            print(oof.oof_markdown(keys, SHIPPED_VARIANT, chip))
        case "summary":
            print(summary.summary_markdown(keys, chip))
        case "charts":
            for path in charts.write_charts(efficiency.rows(keys, chip=chip), keys, chip):
                print(f"wrote {path}")
        case "all":
            pipeline.run_all(keys, args.sweep_model, chip, log=print)
    return 0


_PARITY_COLUMNS = {
    "model": "model",
    "max_abs_diff": "max prob. diff",
    "same_top": "same top",
    "accuracy": "accuracy",
    "auto": "auto share",
    "auto_acc": "auto accuracy",
    "auto_wrong": "auto wrong",
    "p50_ms": "p50 ms",
    "p95_ms": "p95 ms",
    "prompt_tokens": "prompt tokens",
}


def _parity_display(row: Mapping[str, Any]) -> dict[str, Any]:
    shown: dict[str, Any] = {}
    for field, label in _PARITY_COLUMNS.items():
        value = row[field]
        if field == "max_abs_diff":
            value = f"{value:.5f}"
        elif field in ("p50_ms", "p95_ms"):
            value = f"{value:.1f}"
        shown[label] = value
    return shown


def _record(args: argparse.Namespace) -> int:
    from . import recorder

    if args.seconds:
        recorder.record(args.seconds, args.note)
    else:
        recorder.record_until_enter(args.note)
    return 0


def main(argv: Sequence[str] | None = None) -> int:
    args = _build_parser().parse_args(argv)
    handlers: dict[str, Callable[[argparse.Namespace], int]] = {
        "asr": _asr,
        "polish": _polish,
        "intent": _intent,
        "record": _record,
    }
    if args.command == "clips":
        for clip in ensure_clips(force=args.force):
            print(f"{clip.name:<16} [{clip.lang:<5}] {clip.text}")
        return 0
    return handlers[args.command](args)
