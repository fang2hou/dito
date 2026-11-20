"""The interface every polish runtime adapter implements."""

from dataclasses import dataclass
from typing import Protocol

from .prompts import Prompt

APP_SEED = 1234
APP_TEMPERATURE = 0.2
LEGACY_MAX_TOKENS = 300
RUNTIME_VERSION = "fresh-context-v1"


@dataclass(frozen=True, slots=True)
class SamplingProfile:
    name: str
    temperature: float
    seed: int | None
    max_tokens: int | None
    context_tokens: int | None

    def output_budget(self, prompt_tokens: int) -> int:
        if self.max_tokens is not None:
            return self.max_tokens
        return min(max(prompt_tokens * 3 // 2, 256), 4096)

    def context_budget(self, prompt_tokens: int) -> int:
        if self.context_tokens is not None:
            return self.context_tokens
        return max(prompt_tokens + self.output_budget(prompt_tokens), 1024)


APP_SAMPLING = SamplingProfile("app", APP_TEMPERATURE, APP_SEED, None, None)
LEGACY_SAMPLING = SamplingProfile("legacy", 0.0, None, LEGACY_MAX_TOKENS, 2048)
SAMPLING_PROFILES = {profile.name: profile for profile in (APP_SAMPLING, LEGACY_SAMPLING)}


@dataclass(frozen=True, slots=True)
class Polished:
    text: str
    elapsed_s: float
    tokens: int
    prompt_tokens: int
    max_tokens: int
    context_tokens: int

    @property
    def tokens_per_s(self) -> float:
        return self.tokens / self.elapsed_s if self.elapsed_s > 0 and self.tokens > 0 else 0.0


class Polisher(Protocol):
    def polish(self, prompt: Prompt, sampling: SamplingProfile) -> Polished: ...
