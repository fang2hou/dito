"""The same prompts through mlx-lm, the Apple Silicon native runtime."""

import time

from ..weights import Weights
from .base import Polished, SamplingProfile
from .prompts import Prompt


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

    def polish(self, prompt: Prompt, sampling: SamplingProfile) -> Polished:
        import mlx.core as mx
        import mlx_lm
        from mlx_lm.sample_utils import make_sampler

        rendered = self._render_prompt(prompt.messages())
        prompt_tokens = len(self._tokenizer.encode(rendered))
        max_tokens = sampling.output_budget(prompt_tokens)
        if sampling.seed is not None:
            mx.random.seed(sampling.seed)
        start = time.perf_counter()
        out = mlx_lm.generate(
            self._model,
            self._tokenizer,
            prompt=rendered,
            max_tokens=max_tokens,
            sampler=make_sampler(temp=sampling.temperature, top_p=1.0, top_k=0, min_p=0.0),
            verbose=False,
        )
        elapsed = time.perf_counter() - start
        tokens = len(self._tokenizer.encode(out))
        if "</think>" in out:
            out = out.rsplit("</think>", 1)[1]
        return Polished(
            text=out.strip(),
            elapsed_s=elapsed,
            tokens=tokens,
            prompt_tokens=prompt_tokens,
            max_tokens=max_tokens,
            context_tokens=prompt_tokens + max_tokens,
        )
