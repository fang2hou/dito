# ADR-0012: Fix model behavior with structure, not longer prompts

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Speech models echo their hotword list on silence, and polish models mix reference content into the output or answer dictated questions. Adding instructions to the prompts didn't stop either.

## Decision

Model behavior is controlled through structure: follow each model's own prompt format, fence reference content (wordbook words, style samples) in escaped tags, gate silent clips before any engine, and detect known failure shapes (such as a hotword echo) and re-run without the trigger.

The polish prompt follows the same rule (2026-11-12, measured in `bench/POLISH_PROMPT.md`):

- One fixed scaffold in English states the task for every transcript language, in named sections (`#Background#`, `#Purpose#`, `#Constraints#`, `#Style#`, `#Vocabulary#`, `#Outputs#`). It never changes with the language, so no code decides how the task is worded.
- The transcript is the whole user turn, between two `===` lines. A fence inside the transcript is dropped.
- Three demonstration turns precede the transcript, one each in Chinese, Japanese and English: a dictated request to translate, a correction far from what it corrects, and a question. They are real user and assistant turns, not text inside the system prompt.
- The user's rules are the only part that follows the language. They are layered under `#Style#` and can shape wording, never the task.

The command tools (rewrite, translate, summarize, reply, answer) follow the same scaffold: the same sections, the selected text between `===` lines, three demonstration turns per tool, and the instruction as the last block of the final user turn. Only the purpose, the language-type rule, the extra constraints, the output rule and the demonstrations differ by tool (`bench/POLISH_PROMPT.md`).

Meeting paragraphs use a fixed English cleanup scaffold regardless of interface or transcript language, a fenced current paragraph, and three demonstration turns in Chinese, Japanese and English. Vocabulary and prior paragraphs are escaped read-only references, separate from the current speech. Questions and instructions in speech stay in the transcript instead of becoming requests to the editor.

Live meeting notes (2026-11-27) use two fixed English requests: the current topic from the last minute of the transcript, and a memo of the whole meeting rewritten from the previous memo and the speech after it. Hints run about every 30 seconds while new speech arrives and the memo is due every five minutes of recording time. Memo input comes from the previous memo and the transcript after its last successful watermark. Each has three demonstration turns in Chinese, Japanese and English. The meeting's language is decided by code from the transcript (`meeting_notes::lang`), never by the model or the interface language, and goes in the user turn as data: a language tag and a real line of the transcript as a sample, naming no language when the transcript cannot tell it reliably and no language is settled or set by the user. Speaker names follow the meeting's language. The transcript is the fenced part, the previous hint and memo are escaped references, and each reply is a line format read back by `meeting_notes::prompt`; a reply that does not parse, a memo without its END line, or a reply that repeats the scaffold leaves the previous notes in place instead of showing something invented.

Meeting titles use a fixed English system prompt and a fenced transcript. The display language is context, and the title follows the main language spoken in the transcript. The editable title prompt controls style and format.

## Alternatives Considered

### Longer instructions

- Pros: Easy
- Cons: Didn't work; grows with every model
- Why not chosen: Rejected

## Consequences

Each new model needs its own format and failure checks; heuristics have limits and are documented next to the code.

## Pitfalls

Moves that look reasonable but go against this decision:

- Fixing hallucinations by adding instructions or by deleting hotwords from transcripts.
- Removing the silent-clip gate.
- Putting chat instructions into a speech model's audio turn.
- Wording the task once per language, or choosing the scaffold from the transcript's script. A stray kana in a Chinese sentence then turns the whole prompt Japanese and the model translates.
- Sending the transcript as a bare user turn, or writing the demonstrations into the system prompt, where a small model copies them.
- Demonstrations that lean to one language: a small model answers in the language most of them use.

## Review Triggers

Models with reliable built-in voice detection and instruction following replace the current ones.

## Code

`crates/dito-runtimes/src/llama.rs` (`asr_prompt`, `is_hotword_echo`), `crates/dito-core/src/prompts.rs`, `crates/dito-core/src/command/`, `crates/dito-core/src/audio.rs`.

## Application writing styles

Select app-specific writing rules by the frontmost application's bundle identifier captured at recording start. Keep common rules per language and keep optional writing examples inside escaped reference tags. The rules of a language are chosen by the language the speech engine reported for the transcript; only when it reported none does the script of the text decide. A copied style owns independent instructions and examples for every language; it never shares mutable state with its source. Unknown or removed application styles fall back to Other. Settings preview explicitly uses its selected text language and never sends the result to another app.
