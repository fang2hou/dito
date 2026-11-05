---
name: dev-signing
description: Local code-signing identity for Dito development ("dito-dev") — set it up on a new machine with scripts/certs.sh so macOS TCC permissions (microphone, accessibility, input monitoring) stop resetting on every rebuild of cargo run/test or scripts/bundle.sh. Use when local dev hits permission re-prompts, when configuring this repo on a new computer, or when touching signing, certs.sh, dev-runner.sh, .cargo/config.toml, or crates/dito-platform permissions.
---

# Dito local dev code signing

## Why

macOS TCC (Privacy & Security grants: microphone, accessibility, input monitoring)
keys each grant off the app's code signature. Unsigned or ad-hoc-signed binaries get
a new signature on every rebuild, so a permission granted yesterday "suddenly
disappears" today — and for unbundled `cargo run` binaries the system lists the
grant under the launching terminal instead of the app.

Fix: a self-signed code-signing identity created once per machine and trusted for
code signing. Binaries are re-signed with it (stable identifier
`com.japan-d2.dito` + stable certificate = stable designated requirement = one
persistent TCC entry for the app and the test binaries).

## One-time setup per machine

```sh
scripts/certs.sh
```

Creates the `dito-dev` identity (RSA 2048, EKU codeSigning, 10 years) in the login
keychain and trusts it for code signing; macOS shows one authorization prompt.
Idempotent — safe to re-run; exits early when the identity already exists.

Verify:

```sh
security find-identity -v -p codesigning   # lists "dito-dev"
```

## How it hooks into the build (already wired, nothing to remember)

- `.cargo/config.toml` sets a cargo runner → `scripts/dev-runner.sh` re-signs the
  fresh binary before every `cargo run` / `cargo test` launch. Unset identity ⇒
  falls back to running unsigned (old behavior) with one stderr hint.
- `scripts/bundle.sh` auto-detects `dito-dev` when `CODESIGN_IDENTITY` is unset;
  exporting a real identity (e.g. `Developer ID Application: …`) still wins.

Verify a signed dev binary:

```sh
codesign -dv target/debug/dito
codesign -dr - target/debug/dito   # designated => identifier "com.japan-d2.dito" and certificate root = H"…"
```

The certificate hash must be identical across rebuilds — that is exactly what keeps
the TCC grant.

## LaunchServices is part of the permission story

- A bare `target/debug/dito` has no Info.plist: macOS silently denies the microphone (no
  usage description, no prompt, nothing in System Settings). `scripts/dev-runner.sh` wraps
  the binary into `target/bundle/DitoDev.app` (same bundle id and identity as the release
  bundle) and launches it via `open -W -n` so TCC sees a proper app.
- TCC prompts are attributed to LaunchServices-launched apps. A shell-exec'd binary is
  attributed to its parent terminal, which suppresses mic prompts and even
  "open System Settings" from working. Always launch the app via `open`, never exec the
  inner binary.
- The global-shortcut event tap is an ACTIVE tap (`CGEventTapOptions::Default`) and needs
  **Accessibility only**. Input Monitoring is for listen-only taps; dito must not request it.

## Notes and troubleshooting

- Only `cargo run` / `cargo test` go through the runner. Binaries executed directly
  from `target/` (e.g. `./target/debug/examples/press_shortcut`, `./target/debug/dito`)
  stay unsigned and lose their grants on every rebuild — launch them via
  `cargo run …` or `scripts/dev-runner.sh <binary> [args…]` instead.
- Stale grants from the ad-hoc era may still be listed (attributed to the terminal
  or to an old ad-hoc signature): remove them in System Settings → Privacy &
  Security, then grant fresh to Dito once. Unbundled dev runs are listed under
  `dito` (the signed binary), not under the terminal.
- A "codesign wants to access key dito-dev" keychain prompt means the ACL lost
  `/usr/bin/codesign`: `security import` with `-T` sets it, or re-run
  `scripts/certs.sh` after deleting the old identity.
- Identity valid but signing fails with "not trusted": re-trust with
  `security add-trusted-cert -p codeSign -k ~/Library/Keychains/login.keychain-db <cert.pem>`.
- Cert expiry (10y) or compromise: delete the cert + key in Keychain Access, remove
  trust (export with `security find-certificate -c dito-dev -p > cert.pem`, then
  `security remove-trusted-cert cert.pem` — it takes the cert file, not a keychain),
  re-run `scripts/certs.sh`. Grants reset once, then persist again.
- Override the identity name with `DITO_CODESIGN_IDENTITY` (default `dito-dev`);
  both `scripts/certs.sh` and `scripts/dev-runner.sh` honor it.
- Private key never leaves the machine's login keychain — no secrets in the repo;
  every machine issues its own cert.
