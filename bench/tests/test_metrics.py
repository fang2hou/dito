from dito_bench.metrics import score


def test_english_uses_case_insensitive_wer() -> None:
    result = score("Ship it tomorrow", "ship it tomorrow", "en")
    assert (result.metric, result.value) == ("WER", 0.0)


def test_cjk_uses_cer_ignoring_whitespace() -> None:
    result = score("明日の 会議室", "明日の会議室", "ja")
    assert (result.metric, result.value) == ("CER", 0.0)


def test_mixed_clip_counts_character_errors() -> None:
    result = score("先做quick prototype", "先做quick prototyp", "mixed")
    assert result.metric == "CER"
    assert 0 < result.value < 0.1
