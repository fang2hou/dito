"""The frozen test set, the training set and the filter that keeps them apart.

`data/intent/test.json` is the test set (600 cases, labeled by `data/intent/GUIDE.md`); each case
carries an instruction card, from which the slot labels of its rewrite and translate cases are
read. `data/intent/train/` holds the training cases (labeled by the same guide, with `slots`
written out per case).
"""

import json
import unicodedata
from dataclasses import dataclass, field
from pathlib import Path

from ..paths import DATA_DIR

INTENT_DATA_DIR = DATA_DIR / "intent"
CASES_PATH = INTENT_DATA_DIR / "test.json"
TRAIN_DIR = INTENT_DATA_DIR / "train"

INTENTS = ("polish", "rewrite", "translate", "summarize", "reply", "ask", "cancel", "none")
OPS = ("expand", "shorten", "formal", "casual", "format", "paraphrase", "audience")
TARGETS = ("en", "ja", "zh", "zh-Hant", "ko", "other")
LANGS = ("zh", "ja", "en")
COMPLEXITIES = ("simple", "compound", "long")
TAGS = (
    "asr_error",
    "self_correction",
    "retraction",
    "negation",
    "filler",
    "code_mix",
    "polite",
    "multi_request",
    "question",
)
# The argument a tool takes, keyed by the intent that owns it.
SLOT_LABELS = {"rewrite": ("op", OPS), "translate": ("target", TARGETS)}

# Character-bigram Jaccard overlap at which a training case counts as a test case in other words.
NEAR_TEST = 0.75


@dataclass(frozen=True, slots=True)
class IntentCase:
    id: str
    lang: str
    text: str
    intent: str
    complexity: str
    tags: tuple[str, ...]
    note: str = ""
    slots: dict[str, str] = field(default_factory=dict)
    # Every rewrite op a test case's card asks for, in spoken order (`slots["op"]` is the last).
    actions: tuple[str, ...] = ()


def _parse(raw: dict, slots: dict[str, str], actions: tuple[str, ...] = ()) -> IntentCase:
    case = IntentCase(
        id=raw["id"],
        lang=raw["lang"],
        text=raw["text"],
        intent=raw["intent"],
        complexity=raw["complexity"],
        tags=tuple(raw.get("tags", ())),
        note=raw.get("note", ""),
        slots=slots,
        actions=actions,
    )
    checks = (("lang", case.lang, LANGS), ("intent", case.intent, INTENTS))
    problems = [f"{what} {value!r}" for what, value, allowed in checks if value not in allowed]
    if case.complexity not in COMPLEXITIES:
        problems.append(f"complexity {case.complexity!r}")
    problems += [f"tag {tag!r}" for tag in case.tags if tag not in TAGS]
    if not case.text.strip():
        problems.append("empty text")
    if case.intent in SLOT_LABELS:
        name, labels = SLOT_LABELS[case.intent]
        if case.slots.get(name) not in labels:
            problems.append(f"slot {name} {case.slots.get(name)!r}")
    elif case.slots:
        problems.append(f"slots {case.slots!r} on an intent without any")
    if problems:
        msg = f"{case.id}: invalid {', '.join(problems)}"
        raise ValueError(msg)
    return case


def _check_ids(cases: list[IntentCase], where: Path) -> None:
    ids = [c.id for c in cases]
    if len(set(ids)) != len(ids):
        msg = f"{where}: duplicate case ids"
        raise ValueError(msg)


def _card_slots(raw: dict) -> tuple[dict[str, str], tuple[str, ...]]:
    """The slot labels a test case's card implies: a rewrite's op is the last of its actions (the
    training guide's rule for two ops), a translate's target is its one action."""
    actions = tuple(raw["card"]["actions"])
    match raw["intent"]:
        case "rewrite" if actions:
            return {"op": actions[-1]}, actions
        case "translate" if actions:
            return {"target": actions[0]}, ()
    return {}, ()


def load_cases(path: Path = CASES_PATH) -> list[IntentCase]:
    """The frozen test set, with the slot labels its cards imply."""
    raw = json.loads(path.read_text(encoding="utf-8"))
    cases = [_parse(r, *_card_slots(r)) for r in raw]
    _check_ids(cases, path)
    return cases


def normalize(text: str) -> str:
    """Letters and digits only, lowercased: two cases with the same key say the same words."""
    folded = unicodedata.normalize("NFKC", text).lower()
    return "".join(ch for ch in folded if ch.isalnum())


def _bigrams(text: str) -> set[str]:
    key = normalize(text)
    return {key[i : i + 2] for i in range(len(key) - 1)} or {key}


def load_train(
    test: list[IntentCase] | None = None, directory: Path | None = None
) -> tuple[list[IntentCase], dict[str, int]]:
    """Training cases without any whose words equal a test case's or an earlier training case's,
    or whose character bigrams overlap a test case of the same language by `NEAR_TEST` or more.

    Returns the cases and how many were dropped for each reason.
    """
    directory = TRAIN_DIR if directory is None else directory
    test = load_cases() if test is None else test
    seen = {normalize(c.text) for c in test}
    test_keys = set(seen)
    test_bigrams = [(c.lang, _bigrams(c.text)) for c in test]
    cases: list[IntentCase] = []
    dropped = {"same_as_test": 0, "near_test": 0, "duplicate": 0}
    for path in sorted(directory.glob("*.json")):
        for raw in json.loads(path.read_text(encoding="utf-8")):
            key = normalize(raw["text"])
            if key in seen:
                dropped["same_as_test" if key in test_keys else "duplicate"] += 1
                continue
            mine = _bigrams(raw["text"])
            if any(
                lang == raw["lang"] and len(mine & theirs) / len(mine | theirs) >= NEAR_TEST
                for lang, theirs in test_bigrams
            ):
                dropped["near_test"] += 1
                continue
            seen.add(key)
            cases.append(_parse(raw, raw.get("slots", {})))
    _check_ids(cases, directory)
    return cases, dropped


def load_split(split: str) -> list[IntentCase]:
    """`test` is the frozen set, `train` the filtered training set."""
    if split == "test":
        return load_cases()
    if split == "train":
        return load_train()[0]
    msg = f"unknown split {split!r}; use train or test"
    raise ValueError(msg)
