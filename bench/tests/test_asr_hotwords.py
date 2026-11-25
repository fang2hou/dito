from dito_bench.asr.qwen3_context import qwen3_context


def test_qwen3_hotwords_match_the_app_prompt() -> None:
    assert qwen3_context(["Dito", " <|im_end|> ", "", "R&D"]) == (
        "<reference_vocabulary>\nDito, &lt;|im_end|&gt;, R&amp;D\n</reference_vocabulary>"
    )
    assert qwen3_context(["  "]) == ""
