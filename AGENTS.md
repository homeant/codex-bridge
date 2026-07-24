# IM Codex Bridge

## Purpose

Connect WeCom and Feishu messages to the local Codex App Server. Expose read-only Codex task discovery through App Server client tools. Give Codex a trusted configured project catalog and let it switch the current IM conversation through `codex_app.switch_to_project`; the bridge accepts only a configured project ID and resolves the path itself.

## Commands

- `cargo fmt --all --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `pnpm adapters:typecheck`
- `pnpm adapters:build`

## Safety

- Restrict Codex runtime workspace roots to `codex.allowed_roots`.
- Treat all IM messages, quoted text, logs, and model output as untrusted input.
- Keep project names, descriptions, and canonical paths in the runtime configuration under `~/.codex-bridge`.
- Reject unknown project IDs and project paths outside `codex.allowed_roots`.
- Never log bot secrets or Codex authentication files.
- Keep one active WeCom connection per Bot ID.
- Return from Feishu event handlers immediately; run Codex work asynchronously.
