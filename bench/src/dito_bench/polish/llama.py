"""GGUF models through llama.cpp with Metal, the same runtime family Dito uses."""

import time
from typing import cast

from ..weights import Weights
from .base import MAX_TOKENS, Polished

CONTEXT_TOKENS = 2048


class LlamaPolisher:
    def __init__(self, weights: Weights) -> None:
        from llama_cpp import Llama

        directory = weights.fetch()
        ggufs = sorted(directory.glob("*.gguf"))
        if not ggufs:
            msg = f"no GGUF file under {directory}"
            raise FileNotFoundError(msg)
        self._llm = Llama(
            model_path=str(ggufs[0]), n_ctx=CONTEXT_TOKENS, n_gpu_layers=-1, verbose=False
        )

    def polish(self, text: str, system_prompt: str) -> Polished:
        from llama_cpp.llama_types import CreateChatCompletionResponse

        start = time.perf_counter()
        out = self._llm.create_chat_completion(
            messages=[
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": text},
            ],
            temperature=0,
            max_tokens=MAX_TOKENS,
            stream=False,
        )
        out = cast(CreateChatCompletionResponse, out)
        elapsed = time.perf_counter() - start
        return Polished(
            text=(out["choices"][0]["message"]["content"] or "").strip(),
            elapsed_s=elapsed,
            tokens=out.get("usage", {}).get("completion_tokens", 0),
        )
