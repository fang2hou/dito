"""ASR benchmark: run missing cases into the result store, and report from the store."""

from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .. import env
from ..audio import sha256_file
from ..clips import Clip, ensure_clips, load_clips
from ..metrics import score
from ..store import ResultStore, fingerprint
from ..sync import Case, sync_store
from .base import AsrModel, Transcription
from .registry import AsrSpec, get_asr_spec

TASK = "asr"


@dataclass(frozen=True, slots=True)
class AsrInput:
    clip: Clip
    hotwords: tuple[str, ...]


def build_cases(spec: AsrSpec, clips: Sequence[Clip]) -> list[Case[AsrInput]]:
    """One plain case per clip, plus a hotword case when the model can use hotwords."""
    cases = []
    for clip in clips:
        variants = [()]
        if clip.hotwords and spec.supports_hotwords:
            variants.append(clip.hotwords)
        for hotwords in variants:
            case_id = f"{clip.name}+hotwords" if hotwords else clip.name
            cases.append(
                Case(
                    case_id,
                    _fingerprint(clip, hotwords),
                    AsrInput(clip, hotwords),
                )
            )
    return cases


def _fingerprint(clip: Clip, hotwords: tuple[str, ...]) -> str:
    return fingerprint(sha256_file(clip.path), " ".join(hotwords))


def _execute(model: AsrModel, item: AsrInput) -> dict[str, Any]:
    result = model.transcribe(item.clip.path, item.hotwords)
    return {
        "text": result.text,
        "language": result.language,
        "audio_s": round(result.audio_s, 3),
        "elapsed_s": round(result.elapsed_s, 3),
    }


def _warmup_with(clips: Sequence[Clip]) -> Callable[[AsrModel], None]:
    shortest = min(clips, key=lambda c: c.path.stat().st_size)

    def warmup(model: AsrModel) -> None:
        model.transcribe(shortest.path)

    return warmup


def run(
    keys: Sequence[str],
    *,
    refresh: bool = False,
    chip: str | None = None,
    log: Callable[[str], None] = print,
) -> None:
    clips = ensure_clips()
    chip_slug = env.chip_slug(chip)
    for key in keys:
        spec = get_asr_spec(key)
        log(f"[{key}] {spec.label} ({spec.runtime})")
        store = ResultStore.open(TASK, chip_slug, key)
        sync_store(
            store,
            weights=spec.weights,
            packages=spec.packages,
            cases=build_cases(spec, clips),
            load=lambda spec=spec: spec.load(spec.weights),
            warmup=_warmup_with(clips),
            execute=_execute,
            refresh=refresh,
            log=log,
        )


def report_rows(keys: Sequence[str], *, chip: str | None = None) -> tuple[list[dict], list[str]]:
    """Rows built purely from stored records; no model runs. Also returns warnings."""
    clips = {c.name: c for c in load_clips()}
    chip_slug = env.chip_slug(chip)
    rows: list[dict] = []
    warnings: list[str] = []
    for key in keys:
        get_asr_spec(key)  # fail early on an unknown key
        store = ResultStore.open(TASK, chip_slug, key)
        if not store.records:
            warnings.append(f"{key}: no results yet (run `dito-bench asr run -m {key}`)")
            continue
        stale = 0
        for case_id, record in store.records.items():
            clip = clips.get(case_id.split("+")[0])
            if clip is None:
                stale += 1
                continue
            result = record.result
            transcription = Transcription(
                result["text"], result["language"], result["audio_s"], result["elapsed_s"]
            )
            error = score(clip.text, transcription.text, clip.lang)
            hotwords = clip.hotwords if case_id.endswith("+hotwords") else ()
            is_stale = clip.path.exists() and not store.is_fresh(
                case_id, _fingerprint(clip, hotwords)
            )
            rows.append(
                {
                    "model": key,
                    "clip": case_id,
                    "stale": "yes" if is_stale else "",
                    "lang": clip.lang,
                    "detected": transcription.language,
                    "metric": error.metric,
                    "error": error.value,
                    "elapsed_s": transcription.elapsed_s,
                    "x_realtime": round(transcription.realtime_factor, 1),
                    "text": transcription.text,
                }
            )
        if stale:
            warnings.append(f"{key}: {stale} record(s) refer to clips that no longer exist")
    return rows, warnings


def summarize(rows: Sequence[dict]) -> list[dict]:
    """Mean error and speed per model and metric."""
    groups: dict[tuple[str, str], list[dict]] = {}
    for row in rows:
        groups.setdefault((row["model"], row["metric"]), []).append(row)
    return [
        {
            "model": model,
            "metric": metric,
            "mean_error": sum(r["error"] for r in group) / len(group),
            "mean_elapsed_s": sum(r["elapsed_s"] for r in group) / len(group),
            "mean_x_realtime": round(sum(r["x_realtime"] for r in group) / len(group), 1),
            "cases": len(group),
        }
        for (model, metric), group in groups.items()
    ]


def compare_file(
    audio: Path, keys: Sequence[str], hotwords: Sequence[str] = ()
) -> list[dict[str, Any]]:
    """Transcribe one arbitrary file with each model. Not stored: there is no ground truth."""
    rows = []
    for key in keys:
        spec = get_asr_spec(key)
        model = spec.load(spec.weights)
        result = model.transcribe(audio, hotwords if model.supports_hotwords else ())
        rows.append(
            {
                "model": key,
                "detected": result.language,
                "elapsed_s": round(result.elapsed_s, 3),
                "x_realtime": round(result.realtime_factor, 1),
                "text": result.text,
            }
        )
    return rows
