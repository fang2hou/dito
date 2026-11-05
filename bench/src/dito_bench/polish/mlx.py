"""The same prompts through mlx-lm, the Apple Silicon native runtime."""

import time

from ..weights import Weights
from .base import MAX_TOKENS, Polished


class MlxPolisher:
    def __init__(self, weights: Weights) -> None:
        import mlx_lm

        loaded = mlx_lm.load(str(weights.fetch()))
        self._model, self._tokenizer = loaded[0], loaded[1]

    def _render_prompt(self, messages: list[dict[str, str]]) -> str:
        # Qwen3.5 chat templates default to thinking mode; polishing needs the direct answer.
        try:
            return self._tokenizer.apply_chat_template(
                messages, add_generation_prompt=True, tokenize=False, enable_thinking=False
            )
        except TypeError:
            return self._tokenizer.apply_chat_template(
                messages, add_generation_prompt=True, tokenize=False
            )

    def polish(self, text: str, system_prompt: str) -> Polished:
        import mlx_lm

        prompt = self._render_prompt(
            [{"role": "system", "content": system_prompt}, {"role": "user", "content": text}]
        )
        start = time.perf_counter()
        out = mlx_lm.generate(
            self._model, self._tokenizer, prompt=prompt, max_tokens=MAX_TOKENS, verbose=False
        )
        elapsed = time.perf_counter() - start
        tokens = len(self._tokenizer.encode(out))
        # Drop a reasoning preamble in case the model emits one anyway.
        if "</think>" in out:
            out = out.rsplit("</think>", 1)[1]
        return Polished(text=out.strip(), elapsed_s=elapsed, tokens=tokens)
