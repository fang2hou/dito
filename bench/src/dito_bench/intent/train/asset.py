"""The `.heads` asset the app bundles: fitted heads, and the GGUF and prompt they were fitted on.

Little-endian layout: the magic `DITOHEAD`, u32 format version (1), u32 header length N, N bytes of
UTF-8 JSON, then f32 payload. For each head in header order the payload holds `mean[dim]`,
`scale[dim]`, `weights[classes][dim]` and `bias[classes]`, where `dim` is `n_embd` times the number
of parts. A head computes `softmax((W (x - mean) / scale + b) / temperature)`.
"""

import hashlib
import json
import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np

from ...paths import REPO_ROOT
from ..probe.runs import Run
from ..stats import mix, softmax
from .heads import Head, Parts, features

MAGIC = b"DITOHEAD"
VERSION = 1
INTENT_HEAD = "intent"
# The committed build inputs of `dito-core`: a build never refits.
HEADS_DIR = REPO_ROOT / "crates/dito-core/src/intent/heads"
_CHUNK = 1 << 20


@dataclass(frozen=True, slots=True)
class DirectMix:
    """Weight of the head in the log-linear mix with the option-logit readout, whose logits are
    scaled by `temperature` first."""

    weight: float
    temperature: float


@dataclass(frozen=True, slots=True)
class NamedHead:
    name: str
    labels: tuple[str, ...]
    parts: Parts
    head: Head  # acts on the raw hidden states of `parts`, side by side


@dataclass(frozen=True, slots=True)
class HeadsAsset:
    model: str  # catalog id
    gguf_sha256: str
    prompt_sha256: str
    direct: DirectMix | None  # applies to the intent head only
    heads: list[NamedHead]

    def head(self, name: str) -> NamedHead:
        return next(h for h in self.heads if h.name == name)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(_CHUNK):
            digest.update(chunk)
    return digest.hexdigest()


def write_heads(path: Path, asset: HeadsAsset) -> int:
    """Write the asset and return its size in bytes."""
    header: dict[str, Any] = {
        "model": asset.model,
        "gguf_sha256": asset.gguf_sha256,
        "prompt_sha256": asset.prompt_sha256,
        "direct": None
        if asset.direct is None
        else {"weight": asset.direct.weight, "temperature": asset.direct.temperature},
        "heads": [
            {
                "name": h.name,
                "labels": list(h.labels),
                "parts": [list(p) for p in h.parts],
                "temperature": h.head.temperature,
                "dim": h.head.mean.shape[0],
            }
            for h in asset.heads
        ],
    }
    blob = json.dumps(header, ensure_ascii=False, separators=(",", ":")).encode()
    payload = b"".join(
        np.concatenate([h.head.mean, h.head.scale, h.head.weights.T.reshape(-1), h.head.bias])
        .astype("<f4")
        .tobytes()
        for h in asset.heads
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(MAGIC + struct.pack("<II", VERSION, len(blob)) + blob + payload)
    return path.stat().st_size


def read_heads(path: Path) -> HeadsAsset:
    raw = path.read_bytes()
    if raw[:8] != MAGIC:
        msg = f"{path}: not a heads asset"
        raise ValueError(msg)
    version, length = struct.unpack("<II", raw[8:16])
    if version != VERSION:
        msg = f"{path}: format version {version}, expected {VERSION}"
        raise ValueError(msg)
    header = json.loads(raw[16 : 16 + length].decode())
    payload = np.frombuffer(raw, dtype="<f4", offset=16 + length)
    heads, at = [], 0
    for spec in header["heads"]:
        dim, classes = spec["dim"], len(spec["labels"])
        mean, scale = payload[at : at + dim], payload[at + dim : at + 2 * dim]
        at += 2 * dim
        weights = payload[at : at + classes * dim].reshape(classes, dim)
        at += classes * dim
        bias = payload[at : at + classes]
        at += classes
        head = Head(
            mean.astype(np.float64),
            scale.astype(np.float64),
            weights.T.astype(np.float64),
            bias.astype(np.float64),
            spec["temperature"],
        )
        parts = [(int(layer), pool) for layer, pool in spec["parts"]]
        heads.append(NamedHead(spec["name"], tuple(spec["labels"]), parts, head))
    if at != len(payload):
        msg = f"{path}: payload holds {len(payload)} floats, the header accounts for {at}"
        raise ValueError(msg)
    direct = header["direct"]
    return HeadsAsset(
        header["model"],
        header["gguf_sha256"],
        header["prompt_sha256"],
        None if direct is None else DirectMix(direct["weight"], direct["temperature"]),
        heads,
    )


def intent_probabilities(asset: HeadsAsset, run: Run) -> np.ndarray:
    """The intent probabilities the app computes, from the stored hidden states and option logits
    of `run`: the intent head, mixed with the option-logit readout when the asset says so."""
    named = asset.head(INTENT_HEAD)
    p = named.head.probabilities(features(run, named.parts))
    if asset.direct is None:
        return p
    return mix(p, softmax(run.logits / asset.direct.temperature), asset.direct.weight)
