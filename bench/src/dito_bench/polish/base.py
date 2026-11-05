"""The interface every polish runtime adapter implements."""

from dataclasses import dataclass
from typing import Protocol

MAX_TOKENS = 300


@dataclass(frozen=True, slots=True)
class Polished:
    text: str
    elapsed_s: float
    tokens: int

    @property
    def tokens_per_s(self) -> float:
        return self.tokens / self.elapsed_s if self.elapsed_s > 0 and self.tokens > 0 else 0.0


class Polisher(Protocol):
    """A loaded model. Generation is greedy (temperature 0) and capped at MAX_TOKENS."""

    def polish(self, text: str, system_prompt: str) -> Polished: ...
