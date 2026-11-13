"""Router prompt variants. Each one is a set of switches over the same parts, so a result can be
attributed to one construction choice.

The shipped variant's system text is `crates/dito-core/src/intent/router_system.txt`; the app
builds the rest of the prompt (the echoed instruction) around it.
"""

import hashlib
from dataclasses import dataclass, replace

from ..paths import REPO_ROOT
from .data import INTENTS

FIRST_VARIANT = "base"
SHIPPED_VARIANT = "rules_kw_fewshot_echo"
ROUTER_SYSTEM_PATH = REPO_ROOT / "crates/dito-core/src/intent/router_system.txt"

INTRO = (
    "You route spoken commands for a dictation app. The user selected some text in another app "
    "and then gave an instruction about it, inside <instruction>. The instruction comes from "
    "speech recognition, so it may contain misheard words, filler words and self-corrections. "
)
BASE_RULE = (
    "When the user corrects themselves, only the final request counts; a negated request does "
    "not count. Choose the one option that matches what the user finally wants."
)
RULES = (
    "Decide in this order:\n"
    '1. Self-correction ("no wait", "actually", 不对, やっぱり): only the last request counts.\n'
    "2. A negated request (don't, 不用, しなくていい) does not count.\n"
    "3. If the user withdraws everything and asks for nothing else, it is cancel.\n"
    "4. Several requests that all stand: reply wins over translate, translate over summarize, "
    "summarize over rewrite, rewrite over polish. A reply is what the user will send, so "
    '"summarize this and write back" and "reply in English" are both reply.\n'
    "5. A question about how to respond (what should I say to this, 怎么回, なんて返す) is reply: "
    "the user wants words to send.\n"
    "6. Any other question that expects an answer is ask, even when it names an option word "
    '("what does proofread mean", "is this too formal"). A request politely phrased as a '
    'question ("could you shorten this") is still the request.\n'
    "7. An action the app cannot do (send, save, search, open, delete), chit-chat, dictated "
    "content and garbled speech are none.\n"
    "8. A word may be misheard: judge it by its sound and the rest of the sentence.\n"
    "Close calls:\n"
    "- reply or rewrite: a response to the received text is reply; the selected text itself in "
    "another shape is rewrite. Answering someone at more length is reply, making the user's own "
    "text longer is rewrite.\n"
    "- reply or ask: words to send back is reply, something the user wants to know is ask.\n"
    "- rewrite or summarize: the same text made shorter in its own voice is rewrite, the gist is "
    "summarize.\n"
    "- polish or rewrite: fixing is polish, changing the register or form is rewrite.\n"
    "- summarize or ask: what the whole text says is summarize, one detail about it is ask.\n"
    "- ask or none: a question is ask, something to do elsewhere is none.\n"
    "Choose the one option that matches what the user finally wants."
)

DESCRIPTIONS = {
    "polish": "polish: fix errors, punctuation, grammar and flow without changing meaning, tone or length",
    "rewrite": (
        "rewrite: change length, tone, formality, format, audience or wording of the text on "
        "purpose (expand, shorten, more polite or casual, bullet points, paraphrase)"
    ),
    "translate": "translate: put the text into another language or written variety",
    "summarize": "summarize: give the gist, key points or a one-line summary of the text",
    "reply": (
        "reply: the text is something the user received (a message, email, comment or review) "
        "and the user wants a response to send back, or help with what to say"
    ),
    "ask": "ask: a question that expects an answer, about the text or about anything else",
    "cancel": "cancel: the user calls it off and wants nothing done",
    "none": (
        "none: an action the app cannot do (send, save, search, open, delete), chit-chat, "
        "dictated content or unintelligible speech"
    ),
}
KEYWORDS = {
    "polish": "proofread, fix typos; 润色, 校对, 改错别字, 捋顺; 整える, 校正, 推敲, 添削, 誤字を直す",
    "rewrite": (
        "rephrase, expand, shorten, formal, casual, bullets; 改写, 扩写, 缩短, 精简, 换个说法, "
        "正式一点; 書き換え, 言い換え, 短く, 詳しく, 丁寧に, 敬語, 箇条書き"
    ),
    "translate": "into English, in Japanese; 翻译, 译成, 转成繁体; 翻訳, 訳す, 英語にして",
    "summarize": "recap, gist, key points, TL;DR; 总结, 概括, 摘要, 要点; 要約, まとめ, 要点",
    "reply": (
        "reply, write back, respond, say no politely; 回复, 回一下, 回信, 怎么回, 婉拒; "
        "返信, 返事, 返して, 断る, なんて返す"
    ),
    "ask": (
        "what does this mean, is this right, why, when; 什么意思, 对不对, 为什么, 哪天; "
        "どういう意味, 合ってる, なぜ, いつ"
    ),
    "cancel": "never mind, forget it; 算了, 不用了, 取消; やっぱりいい, キャンセル, 取り消し",
    "none": (
        "send it, save, search, open, good morning; 发给, 保存, 搜一下, 打开, 早上好; "
        "送って, 保存して, 調べて, 開いて, おはよう"
    ),
}
COMPACT = {
    "polish": "polish (fix errors, keep meaning, tone and length)",
    "rewrite": "rewrite (change length, tone, format, audience or wording)",
    "translate": "translate",
    "summarize": "summarize",
    "reply": "reply (a response to the text, to send back)",
    "ask": "ask (a question expecting an answer)",
    "cancel": "cancel (nothing to do)",
    "none": "none (other action, chit-chat, dictated content, noise)",
}

# Written for the prompt; none of these is a test or training case.
FEWSHOT = (
    ("这段有点啰嗦，帮我压短点再顺一顺", "rewrite"),
    ("翻成英文吧，哦不对，只要个要点就行", "summarize"),
    ("客户这条帮我回一下，就说周四可以", "reply"),
    ("这里面提到的负责人是哪位", "ask"),
    ("不用翻译了，直接润色", "polish"),
    ("帮我发给小李", "none"),
    ("この返事、なんて書けばいいかな", "reply"),
    ("えーと、これ、あ、やっぱり要約して", "summarize"),
    ("このぶん、けいごにしてもらえる", "rewrite"),
    ("ここの数字って合ってる", "ask"),
    ("おはよう、今日はいい天気だね", "none"),
    ("tidy this up and then put it in spanish", "translate"),
    ("just write back saying I'll be there", "reply"),
    ("what's the deadline he mentions", "ask"),
    ("this needs a proof reed", "polish"),
    ("what's the gist of this email", "summarize"),
    ("scratch that, forget it", "cancel"),
    ("I'll meet you at the station at six", "none"),
)


@dataclass(frozen=True, slots=True)
class Variant:
    name: str
    rules: bool = False
    keywords: bool = False
    compact: bool = False
    fewshot: bool = False
    echo: bool = False
    cue: bool = False
    options_last: bool = False
    reverse: bool = False
    digits: bool = False

    @property
    def labels(self) -> tuple[str, ...]:
        return tuple("12345678") if self.digits else tuple("ABCDEFGH")

    @property
    def order(self) -> tuple[str, ...]:
        """Intent shown at each label position."""
        return tuple(reversed(INTENTS)) if self.reverse else INTENTS

    def _options(self) -> str:
        lines = []
        for label, intent in zip(self.labels, self.order, strict=True):
            text = COMPACT[intent] if self.compact else DESCRIPTIONS[intent]
            if self.keywords:
                text += f" [{KEYWORDS[intent]}]"
            lines.append(f"{label}: {text}")
        return "\n".join(lines)

    def _label_of(self, intent: str) -> str:
        return self.labels[self.order.index(intent)]

    def build(self, text: str) -> dict[str, str]:
        """The request fields of one instruction: `system`, `user`, `assistant_prefix` and the
        escaped instruction `span` the probe pools over."""
        escaped = text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
        instruction = f"<instruction>{escaped}</instruction>"
        unit = "number" if self.digits else "letter"
        system = INTRO + (RULES if self.rules else BASE_RULE)
        if self.fewshot:
            shots = "\n".join(
                f"<instruction>{t}</instruction> {self._label_of(i)}" for t, i in FEWSHOT
            )
            system += f"\n\nExamples:\n{shots}"
        answer = f"Answer with the option {unit} only."
        if self.options_last:
            user = f"{instruction}\n\nOptions:\n{self._options()}\n\n{answer}"
        else:
            system += f"\n\n{self._options()}\n\n{answer}"
            user = instruction
        if self.echo:
            user += f"\n\nRead the instruction once more, then decide:\n{instruction}"
        prefix = 'The final request is option "' if self.cue else ""
        return {"system": system, "user": user, "assistant_prefix": prefix, "span": escaped}


_BASE = Variant("base")
_RULES = Variant("rules", rules=True)
VARIANTS = {
    v.name: v
    for v in (
        _BASE,
        replace(_BASE, name="base_rev", reverse=True),
        replace(_BASE, name="base_digits", digits=True),
        replace(_BASE, name="base_cue", cue=True),
        replace(_BASE, name="base_echo", echo=True),
        replace(_BASE, name="base_optlast", options_last=True),
        replace(_BASE, name="base_kw", keywords=True),
        replace(_BASE, name="base_fewshot", fewshot=True),
        replace(_BASE, name="compact", compact=True),
        _RULES,
        replace(_RULES, name="rules_rev", reverse=True),
        replace(_RULES, name="rules_kw", keywords=True),
        replace(_RULES, name="rules_echo", echo=True),
        replace(_RULES, name="rules_fewshot", fewshot=True),
        replace(_RULES, name="rules_kw_echo", keywords=True, echo=True),
        replace(_RULES, name="rules_kw_fewshot", keywords=True, fewshot=True),
        replace(_RULES, name="rules_kw_fewshot_echo", keywords=True, fewshot=True, echo=True),
        replace(
            _RULES,
            name="rules_kw_fewshot_echo_rev",
            keywords=True,
            fewshot=True,
            echo=True,
            reverse=True,
        ),
    )
}


def prompt_sha256() -> str:
    """Hash of the shipped prompt as the app builds it: the content of `router_system.txt` and the
    user turn around a NUL placeholder, joined by a unit separator. A heads asset fitted on another
    prompt is refused by comparing this value."""
    system = ROUTER_SYSTEM_PATH.read_text(encoding="utf-8")
    user = VARIANTS[SHIPPED_VARIANT].build("\x00")["user"]
    return hashlib.sha256(f"{system}\x1f{user}".encode()).hexdigest()
