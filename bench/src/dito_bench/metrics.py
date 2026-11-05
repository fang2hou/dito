"""Error metrics: WER for English, whitespace-insensitive CER for everything else."""

import re
from dataclasses import dataclass

from jiwer import cer, wer


@dataclass(frozen=True, slots=True)
class Score:
    metric: str
    value: float


def score(reference: str, hypothesis: str, lang: str) -> Score:
    if lang == "en":
        return Score("WER", wer(reference.lower(), hypothesis.lower()))
    return Score("CER", cer(_strip_spaces(reference), _strip_spaces(hypothesis)))


def _strip_spaces(text: str) -> str:
    return re.sub(r"\s+", "", text)
