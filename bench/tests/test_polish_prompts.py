import pytest

from dito_bench.polish.prompts import PROMPTS, RULES, system_prompt


@pytest.mark.parametrize("lang", sorted(PROMPTS))
def test_every_language_has_all_layers(lang: str) -> None:
    assert {"base", "general", "slack"} <= set(PROMPTS[lang])
    assert len(RULES[lang]) == 3


def test_dictionary_is_escaped_and_blank_terms_are_dropped() -> None:
    prompt = system_prompt("en", dictionary=["R&D", "  ", " sprint "])
    assert "<term>R&amp;D</term>" in prompt
    assert "<term>sprint</term>" in prompt
    assert prompt.count("<term>") == 2


def test_prompt_without_dictionary_has_no_vocabulary_block() -> None:
    assert "reference_vocabulary" not in system_prompt("ja")
