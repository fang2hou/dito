# ADR-0007: Cloud speech vendors through a declarative config

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Each speech vendor has a different HTTP shape and changes it over time. Code per vendor means a release for every change and no way for users to fix or add a vendor.

## Decision

A cloud speech provider is a JSON connection config: connection, language map, request template and response pointers, with a fixed set of placeholders and no expression language. Presets ship as templates; users can edit them or describe a new vendor. Each profile has a stable id that also names its Keychain account, so bundled templates can be added without touching saved ones. Keys stay outside the JSON, Content-Type is never user-controlled, and redirects are off. The contract is `design/asr-json-config.md`.

## Alternatives Considered

### One adapter per vendor

- Pros: Typed and tested
- Cons: Constant maintenance; users can't fix anything
- Why not chosen: Rejected

### A scripting or expression language in the config

- Pros: Flexible
- Cons: A security and support burden
- Why not chosen: Rejected

## Consequences

Vendors that need streaming, polling or multi-step uploads don't fit v1 and need a new config version or a dedicated adapter.

## Pitfalls

Moves that look reasonable but go against this decision:

- Adding vendor-specific code paths for something the config can express.
- Adding expressions or scripts to the config.
- Enabling redirects, or allowing keys inside the JSON.

## Review Triggers

Several wanted vendors need streaming or multi-step flows.

## Code

`crates/dito-core/src/provider/configured_asr/`, `design/asr-templates/`.
