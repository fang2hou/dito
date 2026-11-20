"""GGUF models through llama.cpp with Metal, the same runtime family Dito uses."""

import time
from typing import cast

from ..weights import Weights
from .base import Polished, SamplingProfile
from .prompts import Prompt

EMPTY_THINK_BLOCK = "<think>\n\n</think>\n\n"
CONTEXT_BATCH = 512


def render_chatml(prompt: Prompt) -> str:
    turns = "".join(
        f"<|im_start|>{message['role']}\n{message['content']}<|im_end|>\n"
        for message in prompt.messages()
    )
    return f"{turns}<|im_start|>assistant\n{EMPTY_THINK_BLOCK}"


class LlamaPolisher:
    def __init__(self, weights: Weights) -> None:
        directory = weights.fetch()
        ggufs = sorted(directory.glob("*.gguf"))
        if not ggufs:
            msg = f"no GGUF file under {directory}"
            raise FileNotFoundError(msg)
        self._model_path = str(ggufs[0])

    def polish(self, prompt: Prompt, sampling: SamplingProfile) -> Polished:
        from llama_cpp import Llama
        from llama_cpp.llama_types import CreateCompletionResponse

        rendered = render_chatml(prompt)
        # llama.cpp's Python wrapper owns the context and the sampler together. A fresh wrapper
        # matches Dito's fresh per-request context without mutating internal live context state.
        tokenizer = Llama(
            model_path=self._model_path,
            n_ctx=CONTEXT_BATCH,
            n_batch=CONTEXT_BATCH,
            n_ubatch=CONTEXT_BATCH,
            n_gpu_layers=-1,
            verbose=False,
        )
        prompt_tokens = len(tokenizer.tokenize(rendered.encode(), add_bos=True, special=True))
        tokenizer.close()
        max_tokens = sampling.output_budget(prompt_tokens)
        context_tokens = sampling.context_budget(prompt_tokens)
        llm = Llama(
            model_path=self._model_path,
            n_ctx=context_tokens,
            n_batch=min(CONTEXT_BATCH, context_tokens),
            n_ubatch=min(CONTEXT_BATCH, context_tokens),
            n_gpu_layers=-1,
            verbose=False,
        )
        try:
            start = time.perf_counter()
            out = llm.create_completion(
                rendered,
                temperature=sampling.temperature,
                top_k=0,
                top_p=1.0,
                min_p=0.0,
                typical_p=1.0,
                repeat_penalty=1.0,
                presence_penalty=0.0,
                seed=sampling.seed,
                max_tokens=max_tokens,
                stop=["<|im_end|>"],
                stream=False,
            )
            out = cast(CreateCompletionResponse, out)
            elapsed = time.perf_counter() - start
            return Polished(
                text=out["choices"][0]["text"].strip(),
                elapsed_s=elapsed,
                tokens=out.get("usage", {}).get("completion_tokens", 0),
                prompt_tokens=prompt_tokens,
                max_tokens=max_tokens,
                context_tokens=context_tokens,
            )
        finally:
            llm.close()
