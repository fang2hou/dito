from pathlib import Path

import pytest

from dito_bench.store import SCHEMA_VERSION, ResultStore, fingerprint
from dito_bench.sync import Case, sync_store
from dito_bench.weights import Weights


def open_store(root: Path) -> ResultStore:
    return ResultStore.open("asr", "apple-m4-pro", "model-a", root)


def test_round_trip_keeps_records_sorted_and_unicode(tmp_path: Path) -> None:
    store = open_store(tmp_path)
    store.weights = "hf:repo@abc"
    store.put("zh_01", "f1", {"text": "会议室"})
    store.put("en_01", "f2", {"text": "QA"})
    store.save()

    reloaded = open_store(tmp_path)
    assert reloaded.weights == "hf:repo@abc"
    assert list(reloaded.records) == ["en_01", "zh_01"]
    assert reloaded.records["zh_01"].result == {"text": "会议室"}
    assert "会议室" in store.path.read_text(encoding="utf-8")


def test_freshness_requires_matching_fingerprint(tmp_path: Path) -> None:
    store = open_store(tmp_path)
    store.put("zh_01", fingerprint("audio-v1", ""), {})
    assert store.is_fresh("zh_01", fingerprint("audio-v1", ""))
    assert not store.is_fresh("zh_01", fingerprint("audio-v2", ""))
    assert not store.is_fresh("missing", fingerprint("audio-v1", ""))


def test_fingerprint_separates_parts() -> None:
    assert fingerprint("ab", "c") != fingerprint("a", "bc")


def test_weights_change_drops_records_but_unknown_identity_keeps_them(tmp_path: Path) -> None:
    store = open_store(tmp_path)
    store.weights = "hf:repo@old"
    store.put("zh_01", "f", {})

    assert not store.invalidate_if_weights_changed(None)
    assert not store.invalidate_if_weights_changed("hf:repo@old")
    assert store.records

    assert store.invalidate_if_weights_changed("hf:repo@new")
    assert not store.records


def test_unknown_schema_is_rejected(tmp_path: Path) -> None:
    store = open_store(tmp_path)
    store.save()
    store.path.write_text(f'{{"schema": {SCHEMA_VERSION + 1}}}', encoding="utf-8")
    with pytest.raises(ValueError, match="unsupported schema"):
        open_store(tmp_path)


def test_sync_runs_only_missing_and_stale_cases(tmp_path: Path) -> None:
    store = open_store(tmp_path)
    store.put("a", "fp-a", {"n": 1})
    store.put("b", "old", {"n": 1})
    cases = [Case("a", "fp-a", "A"), Case("b", "fp-b", "B"), Case("c", "fp-c", "C")]
    executed: list[str] = []

    def execute(_model: None, payload: str) -> dict[str, int]:
        executed.append(payload)
        return {"n": 2}

    ran = sync_store(
        store,
        weights=Weights("example/none", tmp_path / "empty"),
        packages=(),
        cases=cases,
        load=lambda: None,
        warmup=lambda _model: None,
        execute=execute,
        log=lambda _message: None,
    )

    assert ran == 2
    assert executed == ["B", "C"]
    assert store.records["a"].result == {"n": 1}
    assert open_store(tmp_path).records["c"].result == {"n": 2}


def test_sync_skips_loading_when_everything_is_fresh(tmp_path: Path) -> None:
    store = open_store(tmp_path)
    store.put("a", "fp-a", {})

    def load() -> None:
        raise AssertionError("model must not load when nothing is stale")

    ran = sync_store(
        store,
        weights=Weights("example/none", tmp_path / "empty"),
        packages=(),
        cases=[Case("a", "fp-a", None)],
        load=load,
        warmup=lambda _model: None,
        execute=lambda _model, _payload: {},
        log=lambda _message: None,
    )
    assert ran == 0
