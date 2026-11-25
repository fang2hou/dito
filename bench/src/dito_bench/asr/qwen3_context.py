"""The hotword context Qwen3-ASR reads in its system turn, built as the app does (qwen_text.rs)."""

from collections.abc import Sequence


def qwen3_context(hotwords: Sequence[str]) -> str:
    words = [
        word.strip().replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
        for word in hotwords
    ]
    words = [word for word in words if word]
    if not words:
        return ""
    return "<reference_vocabulary>\n" + ", ".join(words) + "\n</reference_vocabulary>"
