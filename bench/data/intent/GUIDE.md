# Intent data guide

Context: Dito is a macOS dictation app. In command mode the user selects text in any app, presses a command shortcut and speaks (or types) an instruction about the selected text. The instruction is transcribed by ASR, then a small router must classify it into exactly one of eight intents. Each case is one transcribed instruction.

This guide defines both sets in this folder:

- `test.json`: the frozen test set, 600 cases, 200 per language (`zh`, `ja`, `en`). Reports only; nothing is fitted or chosen on it.
- `train/*.json`: the training set the heads are fitted on, 1684 cases written, 1679 after the load-time filter (see "Training set").

The labeling rules, fields, tags and quality bar below apply to both. Counts and coverage minimums differ and are given per set.

## Intents (exactly one gold label per case)

A user cares first about what the result is to the selected text: it changes that text, it is a new text made from it, or it is an answer about it. The intents follow that.

| Group | id | Meaning | The result | Typical phrasing |
| --- | --- | --- | --- | --- |
| Changes the selected text | `polish` | Fix it without changing purpose, register or length much: typos and ASR errors, punctuation, grammar, fluency, filler words; make it read naturally. Requests to check and fix (proofread, check grammar, fix typos) are polish | Replaces the selection | 整えて, 誤字を直して, 校正して, polish this, fix the grammar, clean it up |
| | `rewrite` | Change a property on purpose: length (expand, add detail, elaborate, shorten, trim), tone or register (more polite, casual, formal, friendlier, firmer, 敬語), format (bullet points, email, title), audience or perspective, or say it in other words (paraphrase). Making it shorter is rewrite unless the user asks for the gist | Replaces the selection | 言い換えて, もっと丁寧に, 箇条書きにして, make it longer, sound more professional, turn it into bullet points |
| | `translate` | Another language or written variety (for example Traditional Chinese). Translation requested together with other changes is still `translate`, unless a reply is asked for (rule 4) | Replaces the selection | 英語にして, 中国語に訳して, translate this into Japanese |
| New text made from the selected text | `summarize` | The gist: summary, key points, TL;DR, a one-line description | Shown for confirmation | 要点をまとめて, 要約して, summarize this, give me the TL;DR |
| | `reply` | The selected text is something the user received (a message, an email, a comment, a review) and the user wants a response to send back, or help with what to say | Shown for confirmation, never written over the selection | なんて返せばいいかな, reply saying Friday works, what should I say to this |
| An answer about the selected text | `ask` | A question that expects an answer, about the text or about anything else (what does this mean, is this correct, who wrote this) | Shown in a window | これ誰が書いたの, what does this mean, is this too formal |
| Control | `cancel` | The user calls the command off and asks for nothing else, including when they first asked for something and then retracted it | Nothing | やっぱりいいや, キャンセル, never mind, forget it |
| Not a command | `none` | An action Dito cannot do (send, save, search, open an app, delete, copy to Slack), chit-chat, dictated content that is not an instruction, garbled or unintelligible recognition output | Nothing | send this to Tom, おはよう, we meet at three tomorrow |

## Precedence rules (apply in order)

1. Self-correction: the request standing at the end wins. "make it longer, no wait, just polish it" is `polish`.
2. Negation removes an intent: "don't translate it, just polish it" is `polish`.
3. Full retraction with nothing left is `cancel`: "summarize this, actually never mind" is `cancel`.
4. Several requests at once: `reply` beats `translate`, `translate` beats `summarize`, `summarize` beats `rewrite`, `rewrite` beats `polish`. "polish it and then make it shorter" is `rewrite`. A reply is the thing the user will send, so what else is said shapes it: "summarize this and write back" and "reply in English" are both `reply`.
5. A question about how to respond is `reply`: the user wants words to send ("なんて返せばいいかな", "what should I say to this", "how do I answer this").
6. Any other question that expects an answer is `ask`, even when it names an intent word ("what does proofread mean", "is this too formal"). A request politely phrased as a question is the request ("could you shorten this").
7. An action Dito cannot do, chit-chat, dictated content and garbled output are `none`.

## Boundaries to get right

| Looks alike | Decide by | Examples |
| --- | --- | --- |
| `reply`, `rewrite` | Is the output a response to the selected text, or the selected text itself in another shape | "reply to this" is reply. "make it read like a reply" is rewrite (format) |
| `reply`, `ask` | Does the user want words to send, or to know something | "how should I answer this" is reply. "what does he mean by that" is ask |
| `reply`, `rewrite` (expand) | Who continues: the user answering someone, or the user's own text growing | "write a few more lines back" is reply. "add a bit more" is rewrite (expand) |
| `rewrite` (shorten), `summarize` | Same text made shorter in its own voice, or the gist | "cut it in half" is rewrite. "give me the gist" is summarize |
| `polish`, `rewrite` (formal, casual) | Fixing, or changing the register | "make it flow better" is polish. "make it more formal" is rewrite |
| `summarize`, `ask` | What the whole text says, or one thing about it | "what is this email mainly about" is summarize. "which day is the deadline he mentions" is ask |
| `ask`, `none` | A question, or something to do elsewhere | "is this word right" is ask. "send it to Tom" is none |

## Fields

A test case:

```json
{
  "id": "en-v2-001",
  "lang": "en",
  "text": "um so can you reply to this saying friday works and keep it short",
  "intent": "reply",
  "complexity": "compound",
  "tags": ["filler"],
  "card": {
    "object": "selection",
    "actions": ["accept"],
    "constraints": ["saying friday works", "keep it short"]
  },
  "note": ""
}
```

A training case has the same fields with `slots` in place of `card`, for example `"slots": {"op": "expand"}`.

- `id`: test cases `<lang>-v2-<NNN>`, 001 to 200, in order. Training cases `<lang>-t<part><NNN>`, for example `ja-ta001`, where `part` is the file letter.
- `lang`: `zh`, `ja` or `en`.
- `text`: the instruction exactly as ASR would output it. See "Spoken text" below.
- `intent`: one of the eight ids.
- `complexity`: `simple` (one short request), `compound` (constraints, several clauses, conditions, corrections), `long` (25+ words in en, 40+ characters in zh or ja, rambling or with context).
- `tags`: zero or more of the following.
  - `asr_error`: contains at least one realistic ASR error (homophone, wrong kanji conversion, wrong segmentation, misheard loanword). Describe each error in `note`, in English, as `wrong -> intended`.
  - `self_correction`: the user changes the request mid-sentence and the final intent differs from the first one mentioned.
  - `retraction`: the user withdraws a request (gold is usually `cancel`).
  - `negation`: an intent word appears negated.
  - `filler`: spoken fillers (えーと, あの, um, uh, like).
  - `code_mix`: words from another language (これを summarize して, can you polish これ).
  - `polite`: polite or indirect phrasing (お願いできますか, could you possibly).
  - `multi_request`: several requests resolved by rule 4.
  - `question`: phrased as a question.
- `note`: English, optional except for `asr_error` cases. Short. No separator characters such as middle dots or vertical bars.
- `card` (test cases): what the instruction says and nothing more. Together with `text` (the instruction) and `intent` (the goal) it is the whole instruction card.
  - `object`: `"selection"`, or the words of `text` that name a part of it ("二つ目の段落", "最後の一文", "the first sentence"), copied exactly.
  - `actions`: what to do, from the fixed lists below, in spoken order. Empty when the intent has none.
  - `constraints`: limits the user states (length, language, tone, things to keep or leave out, content to include), each copied exactly as one continuous piece of `text`. Empty when none is stated. Never a limit the user did not say.
  - After a self-correction or negation, the card holds only what still stands at the end.
  - A polish limited to some kinds of fixes ("誤字だけ", "just fix the typos") keeps that limit as a constraint.
  - For `format` the target form ("in email format", "markdown の表"), and for `audience` the target reader ("小学生にもわかるように", "for a new intern"), are constraints.
  - A tone given by comparison ("like I'm texting a friend", "友達に話すみたいに") is a constraint.
  - In a multi-request case, the requests that lost under rule 4 stay in `constraints` as spoken ("fix the typos first", "先に誤字を直して"): the tool still has to do them.
  - Context is not a constraint. Who the text is for or why ("since it's going to my boss", "お客さんからなんだけど") stays out of the card, unless it names the target reader of an `audience` rewrite.
- `slots` (training cases): the argument of the tool the intent owns, an object.
  - `translate`: `{"target": "<code>"}`, one of `en`, `ja`, `zh` (Simplified or unspecified Chinese), `zh-Hant` (Traditional), `ko`, `other` (any other language).
  - `rewrite`: `{"op": "<op>"}`, one of the rewrite actions below. When a case asks for two, use the one mentioned last.
  - All other intents: `{}`.

The slot labels of a test case come from its card: a rewrite's `op` is the last of its `actions` (so a rewrite with two actions is labeled by the later one, as in the training set), a translate's `target` is its one action.

### Actions by intent

| Intent | `actions` |
| --- | --- |
| `rewrite` | One or more of the seven ops below, every one the user asks for |
| `translate` | One target code: `en`, `ja`, `zh`, `zh-Hant`, `ko`, `other` |
| `reply` | One stance: `accept`, `decline`, `ask_back` (ask for more information), `thanks`, `given` (the user dictates what the reply says and it is none of the others), `unspecified` |
| All others | Empty |

Rewrite ops:

- `expand`: longer, more detail, elaborate, add examples.
- `shorten`: shorter, tighter, trim, fit a length limit (but not "give me the gist", which is `summarize`).
- `formal`: more formal, polite, professional, respectful (敬語, 正式, businesslike).
- `casual`: more casual, friendly, softer, warmer, spoken style.
- `format`: change the form (bullet list, numbered list, email, title, table, markdown).
- `paraphrase`: say it in other words with the same meaning and register.
- `audience`: adapt for a reader or perspective (for children, for non-experts, third person, customer voice).

For `rewrite` and `translate` the words that name the action are not repeated in `constraints`: "make it shorter" is the action `shorten` with no constraint, "cut it to under a hundred words" is `shorten` with the constraint "under a hundred words".

## Spoken text

The router reads what ASR heard, so every case must read like a transcript and not like a typed command.

- Fillers and false starts stay: えーと, あの, なんか, um, uh, like, you know.
- People repeat, restart and trail off: "これを、これ全部", "can you just, can you make this".
- Punctuation is what ASR gives: often none in short utterances, commas where the speaker paused, rarely a full stop. English may lack capitals mid-sentence but keeps "I".
- People point: "これ", "ここ", "this", "this bit". They rarely say "selected text".
- People give context while asking: who wrote it, who it is for, why ("お客さんからなんだけど", "it's from my landlord").
- ASR errors come from the sound: homophones and near homophones, wrong kanji for the same reading, misheard loanwords, merged or split words, dropped particles. They are real words or real word sequences, because ASR decodes with a language model: 最期 for 最後, "sails" for "sales", "core ian" for "Korean". A string that is not a word ("poleesh", "sumarize") is not an ASR error, and neither is a typo a keyboard would make.

## Test set

`test.json`. Each language has two blocks of 100 cases: 001 to 100 written by hand, 101 to 200 drafted by a second model from this guide and then reviewed and corrected by hand. Both blocks follow the counts and minimums below. The set is frozen: it is not edited, and a test (`tests/test_intent_data.py`) checks its distribution and its SHA-256.

Counts per block of 100 (a language has twice these: 28, 48, 20, 20, 32, 20, 16 and 16):

| intent | count |
| --- | ---: |
| polish | 14 |
| rewrite | 24 |
| translate | 10 |
| summarize | 10 |
| reply | 16 |
| ask | 10 |
| cancel | 8 |
| none | 8 |

Coverage minimums per language (tags overlap freely):

- `complexity`: about 40 simple, 50 compound, at least 10 long. Most real instructions are short.
- `filler`: at least 35. `asr_error`: at least 25, at least 10 of them on the word that carries the request, across every intent.
- `self_correction`: at least 12, including corrections into and out of `reply`. `negation`: at least 6. `retraction`: at least 5. `code_mix`: at least 8 in `zh` and `ja`, at least 4 in `en`, where speakers mix less. `question`: at least 14.
- `multi_request`: at least 12, at least 5 of them with `reply`.
- `rewrite`: `expand` at least 5, `shorten` at least 5, `formal` or `casual` at least 6, each other action at least 2, and at least 6 cases with two or more actions.
- `reply`: every stance at least twice, `unspecified` at least 4, at least 3 phrased as a question about how to respond (rule 5).
- `ask`: at least 6 about the text, at least 2 that name an intent word.
- Each row of the boundary table: at least 2 cases on each side, written as close to the other side as a defensible label allows.
- `constraints`: at least 30 cases with one or more, at least 6 with two or more. `object`: at least 8 cases naming a part.

## Training set

`train/*.json` hold the cases the heads are fitted on: the fields above with `slots`. The set is used to fit a small classifier, so a case must never be one of the test cases and must not be written from them: write fresh cases from your own imagination of how people speak to a dictation tool, without reading `test.json`, and do not reuse the example phrasings of this guide verbatim.

| Files | Cases | Source |
| --- | ---: | --- |
| `*-a.json`, `*-b.json` | 898 | Written first, as 150 cases per file and language under six intents (`ask` did not exist: a question that expects an answer was `none`), then relabeled to the eight intents here: its 43 questions that expect an answer are `ask`; everything else keeps its label. Two cases with no defensible label (`言い返して`, and a shorten request that reads as a summary) were removed |
| `*-c.json` | 480 (160 per language) | Written by hand for `reply` (64 per language) and the boundaries around it, then rewritten into spoken transcripts by a second model with labels and slots fixed, and reviewed by hand |
| `*-d.json` | 306 (102 per language) | Written by hand from the out-of-fold errors of the first fit: two requests in both spoken orders, replies with no word for replying, replies in a named language against translations, one's own reply draft to polish or rewrite, the gist of a whole text against one detail, short cancels, actions on a reply. Rewritten into transcripts by the second model under the rule that no word naming an intent is added or removed, and reviewed by hand |

Loading keeps 1679 of the 1684: `data.load_train` drops a training case whose words (letters and digits only, lowercased) equal a test case's or an earlier training case's, and one whose character bigrams overlap a test case of the same language by 75% or more (Jaccard). It drops 1 near-test case and 4 repeats; none equals a test case in other words.

Shapes the first-round files (`*-a`, `*-b`) were written to oversample, per file of 150 cases (tags overlap freely):

- Intents before the relabeling: polish 30, rewrite 40, translate 21, summarize 20, cancel 15, none 24.
- Rewrite: each of the seven ops at least 4 times. Translate: `en`, `ja`, `zh` each at least 3 times where it makes sense for the speaker's language, plus `zh-Hant`, `ko` and `other` at least twice each.
- `asr_error`: at least 55. At least 30 of them hit the intent word itself (the verb that carries the request), with many different errors, not the same one repeated. The rest hit other words. Every one needs a `wrong -> intended` note.
- `multi_request`: at least 25, covering every pair of intents in precedence rule 4, in both spoken orders ("translate then summarize", "summarize then translate").
- `self_correction`: at least 25, with varied first and final intents, including corrections that end in `none` or `cancel`.
- `negation`: at least 15. `retraction`: at least 10. `code_mix`: at least 12. `filler`: at least 25.
- `question`: at least 12, including questions that mention an intent word but expect an answer (then `ask`) and polite requests phrased as questions (then the request).
- `complexity`: about 50 simple, 65 compound, 35 long.
- `none`: unrelated actions, chit-chat, dictated content and garbled output, each at least 3 times.

## Quality bar

- Write as a native speaker actually talks to a dictation tool. Japanese must be natural Japanese, not translated Chinese; Chinese must be natural Mandarin (Simplified); English natural American or British speech.
- No two cases may be paraphrases of each other with the same label and tags; vary vocabulary, verbs, sentence shape and length.
- One defensible label under the rules. If two labelers could disagree after reading the rules, change the case.
- The card adds nothing: every constraint and every named part is in `text`, character for character.
- Do not mention real companies, real people or internal material. Generic names (Tom, 田中さん) are fine.
- Verify with a script before finishing. Test set: parses, 100 items per block, sequential ids, exact intent counts, the minimums above, no duplicate texts, every `asr_error` has a note, actions valid for the intent, every constraint and object part is a substring of `text`. Training files: parses, the file's item count, sequential ids, intent counts, the minimums that file was written to, no duplicate texts, every `asr_error` has a note, slots valid for the intent.
