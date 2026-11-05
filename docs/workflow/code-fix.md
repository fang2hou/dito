# Workflow: Fix Code

Use this for bugs and wrong behavior. The aim is to fix the cause and leave the code harder to break the same way.

## 1. Locate the problem

- Reproduce it, preferably with `cargo run -p dito-app -- --mock --log-sink` so nothing is typed into other apps.
- Confirm where it happens from evidence, not guesses: find the code by the identifiers involved (element ids, `Tr::` keys, command names, log messages) and check the trace output for the text it prints.
- Read the relevant ADRs in [docs/engineering/adr](../engineering/adr) before changing anything they cover. Several macOS workarounds look removable but are deliberate.

## 2. Find the root cause

- Ask why the code allowed this. Look for a better organization that makes the bug impossible: clearer state ownership, one owner per piece of state, a type instead of a flag, a shared helper instead of a copy.
- Check the language and framework best practice for the situation (Rust, GPUI, gpui-kit, Tokio) against the versions in `Cargo.lock`, and prefer it over a local patch.
- Don't silence the symptom: no ignored errors, no disabled lints, no weakened tests.

## 3. Improve proactively

- Fix the cause, then look for the same pattern elsewhere and fix it there too, within reason.
- If the fix changes an architectural decision, update or supersede the ADR in the same change.
- Add a test only when the logic is easy to break and hard to notice in the app (parsing, state machines, config, protocols). Behavior a person sees or feels is checked in the running app.
- Run `./scripts/check.sh fast` and `cargo clippy --workspace --all-targets -- -D warnings`.
