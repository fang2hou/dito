import hashlib
import json
from collections import Counter
from pathlib import Path

from dito_bench.intent.data import (
    CASES_PATH,
    INTENTS,
    LANGS,
    OPS,
    TARGETS,
    load_cases,
    load_train,
    normalize,
)

GUIDE_COUNTS = {
    "polish": 28,
    "rewrite": 48,
    "translate": 20,
    "summarize": 20,
    "reply": 32,
    "ask": 20,
    "cancel": 16,
    "none": 16,
}
TEST_SHA256 = "1bcb28079c1f0a2cdec2c3db89bc0d980d21d1d965b1e37f8a05ccd2a9e733e5"


def test_test_set_keeps_the_guide_distribution() -> None:
    cases = load_cases()
    counts = Counter((c.lang, c.intent) for c in cases)
    assert counts == Counter({(lang, i): GUIDE_COUNTS[i] for lang in LANGS for i in INTENTS})
    for lang in LANGS:
        ids = [c.id for c in cases if c.lang == lang]
        assert ids == [f"{lang}-v2-{n:03d}" for n in range(1, 201)]
    assert all(c.note for c in cases if "asr_error" in c.tags)
    assert hashlib.sha256(CASES_PATH.read_bytes()).hexdigest() == TEST_SHA256


def test_cards_give_the_slot_labels() -> None:
    cases = load_cases()
    rewrites = [c for c in cases if c.intent == "rewrite"]
    assert all(c.slots["op"] == c.actions[-1] and c.slots["op"] in OPS for c in rewrites)
    assert any(len(c.actions) > 1 for c in rewrites)
    assert all(c.slots["target"] in TARGETS for c in cases if c.intent == "translate")
    assert not any(c.slots for c in cases if c.intent not in ("rewrite", "translate"))


def _train_case(n: int, text: str, intent: str = "polish", lang: str = "en") -> dict:
    return {
        "id": f"{lang}-t{n:03d}",
        "lang": lang,
        "text": text,
        "intent": intent,
        "complexity": "simple",
        "tags": [],
        "slots": {"op": "expand"} if intent == "rewrite" else {},
        "note": "",
    }


def test_training_set_is_filtered_against_the_test_set(tmp_path: Path) -> None:
    test = [c for c in load_cases() if c.lang == "en"][:3]
    near = test[0].text + " please"
    cases = [
        _train_case(1, test[1].text.upper() + "!"),
        _train_case(2, near),
        _train_case(3, "fix the commas in this paragraph and nothing else"),
        _train_case(4, "Fix the commas in this paragraph, and nothing else."),
        _train_case(5, "make it a bit longer", "rewrite"),
    ]
    (tmp_path / "a.json").write_text(json.dumps(cases), encoding="utf-8")
    train, dropped = load_train(test, tmp_path)
    assert [c.id for c in train] == ["en-t003", "en-t005"]
    assert dropped == {"same_as_test": 1, "near_test": 1, "duplicate": 1}
    assert train[1].slots == {"op": "expand"}


def test_training_set_loads_with_valid_labels() -> None:
    train, _ = load_train()
    test_keys = {normalize(c.text) for c in load_cases()}
    assert not any(normalize(c.text) in test_keys for c in train)
    assert all(c.intent in INTENTS for c in train)
