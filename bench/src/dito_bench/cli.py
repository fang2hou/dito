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
    return parser


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
        "record": _record,
    }
    if args.command == "clips":
        for clip in ensure_clips(force=args.force):
            print(f"{clip.name:<16} [{clip.lang:<5}] {clip.text}")
        return 0
    return handlers[args.command](args)
