# ADR-0007: Cloud speech vendors through a declarative config

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Each speech vendor has a different HTTP shape and changes it over time. Code per vendor means a release for every change and no way for users to fix or add a vendor.

## Decision

A cloud speech configuration is a request JSON: connection, language map, request template and response pointers, with a fixed set of placeholders and no expression language. Templates ship with the app and pre-fill a new configuration; users can edit it or describe a new vendor. Each configuration has a stable id that also names its Keychain account, so bundled templates can be added without touching saved ones. Keys stay outside the JSON, Content-Type is never user-controlled, and redirects are off. The contract is `design/asr-json-config.md`.

The one request Dito sends that the JSON does not describe opens the connection while a recording runs: `HEAD /` to the host of `connection.base_url`, with nothing of the configuration in it ([ADR 0008](0008-data-stays-local.md), 2026-11-19).

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
