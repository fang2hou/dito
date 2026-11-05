"""Persistent benchmark results, one JSON file per task, chip and model.

    results/<task>/<chip>/<model>.json

A record is *fresh* when its fingerprint (the inputs that determine the output) matches the
current one, so a run only has to execute cases that are new or stale. The files are plain
JSON with stable ordering and are meant to be committed. Only synthetic inputs are
recorded, never personal recordings.
"""

import datetime as dt
import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .paths import RESULTS_DIR

SCHEMA_VERSION = 1


def fingerprint(*parts: str) -> str:
    """Stable short hash of everything that determines a case's output."""
    digest = hashlib.sha256()
    for part in parts:
        digest.update(part.encode())
        digest.update(b"\0")
    return digest.hexdigest()[:16]


@dataclass(slots=True)
class Record:
    fingerprint: str
    recorded_at: str
    result: dict[str, Any]


@dataclass(slots=True)
class ResultStore:
    path: Path
    weights: str | None = None
    environment: dict[str, Any] = field(default_factory=dict)
    records: dict[str, Record] = field(default_factory=dict)

    @classmethod
    def open(
        cls, task: str, chip_slug: str, model_key: str, root: Path = RESULTS_DIR
    ) -> ResultStore:
        path = root / task / chip_slug / f"{model_key}.json"
        if not path.exists():
            return cls(path)
        raw = json.loads(path.read_text(encoding="utf-8"))
        if raw.get("schema") != SCHEMA_VERSION:
            msg = f"{path}: unsupported schema {raw.get('schema')!r}, expected {SCHEMA_VERSION}"
            raise ValueError(msg)
        return cls(
            path,
            weights=raw.get("weights"),
            environment=raw.get("environment", {}),
            records={
                case_id: Record(**record) for case_id, record in raw.get("records", {}).items()
            },
        )

    def is_fresh(self, case_id: str, case_fingerprint: str) -> bool:
        record = self.records.get(case_id)
        return record is not None and record.fingerprint == case_fingerprint

    def invalidate_if_weights_changed(self, current: str | None) -> bool:
        """Drop every record when the weights on disk differ from the recorded ones.

        An unknown current identity (weights not downloaded) keeps the records: the stored
        results are the only information available.
        """
        if current is None or self.weights is None or current == self.weights:
            return False
        self.records.clear()
        return True

    def put(self, case_id: str, case_fingerprint: str, result: dict[str, Any]) -> None:
        now = dt.datetime.now(dt.UTC).isoformat(timespec="seconds")
        self.records[case_id] = Record(case_fingerprint, now, result)

    def save(self) -> None:
        payload = {
            "schema": SCHEMA_VERSION,
            "weights": self.weights,
            "environment": self.environment,
            "records": {
                case_id: {
                    "fingerprint": record.fingerprint,
                    "recorded_at": record.recorded_at,
                    "result": record.result,
                }
                for case_id, record in sorted(self.records.items())
            },
        }
        self.path.parent.mkdir(parents=True, exist_ok=True)
        temp = self.path.with_suffix(".json.tmp")
        temp.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        temp.replace(self.path)
