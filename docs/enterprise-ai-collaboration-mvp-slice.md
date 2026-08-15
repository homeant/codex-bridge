# Enterprise AI Collaboration MVP Slice

This repository now contains the first reviewable vertical slice for the enterprise AI collaboration platform described in `docs/enterprise-ai-collaboration-design.md`, plus a local demo surface for product handoff.

## Scope

The slice is intentionally library-level. It establishes the platform contracts that the daemon, desktop/Web UI, Runtime Connector, and optional IM Channel adapters can build on without changing production behavior in this repository.

Implemented:

- `crates/runtime-connector-protocol`: versioned Runtime Connector DTOs for task offers, run start, progress, approval, cancellation, completion, handoff, shared-context manifests, and runtime project summaries.
- `crates/control-plane`: an MVP control-plane domain service with Workspace, member, Agent, Runtime, conversation, task, run, approval, and audit models.
- `crates/control-plane/migrations/0001_core.sql`: the first persistence schema draft for the core tables and indexes.
- `cargo run -p control-plane --bin mvp-demo -- --addr 127.0.0.1:8787`: a local HTTP API and browser UI backed by seeded fixture state. See `docs/enterprise-ai-collaboration-demo.md`.
- Unit tests that run a fake local Runtime/Codex Provider chain through public-Agent conversation, task offer, run start, task-level owner approval, runtime approval rejection, result return, Runtime-offline recovery, Handoff, public task-pool claim, and cross-Workspace/private-Agent denial paths.

Deferred:

- Production HTTP handlers, production UI routing, Postgres repository implementation, and a live Runtime WebSocket server.
- Hosted Runtime, multi-Runtime scheduling, cross-Workspace collaboration, automatic DAG orchestration, and Channel-first flows.
- Real Codex App Server execution; tests use the protocol boundary as a fake Runtime/Provider.

## Execution Chain

The first end-to-end path is:

1. A Workspace member starts a platform conversation with a public active Agent.
2. The control plane validates `workspace_id`, active membership, Agent visibility/status, Runtime binding, project key, and acceptance policy.
3. A `targeted` task is created and queued, with exactly one ordinary user participant and one Agent participant in the conversation.
4. `offer_next_task` produces a Runtime Connector `TaskOffer` for the Agent owner's online local Runtime.
5. `runtime_accept_task` creates a Codex App Server `RunStart` command with the trusted project key and shared-context manifest.
6. `runtime_complete_run` marks the run/task completed, appends the Agent result message, and records audit events.

Failure paths are explicit:

- Private Agents cannot be called by regular members.
- Cross-Workspace Agent access fails closed.
- Offline Runtime leaves the task in `awaiting_runtime`; when the bound Runtime reports online again, the task returns to a dispatchable or owner-approval state.
- Owner-confirmation Agents create a task-level `ApprovalRequest` before the Runtime can accept the task.
- A Runtime cannot consume another Runtime's task offer by attempting to accept it.
- Approval rejection moves the task to `rejected` and cancels the run.
- Handoff rejects repeated Agent loops and stores a minimal shared-context manifest.

## Migration and Configuration Notes

`0001_core.sql` uses text status columns with `check` constraints so the MVP can evolve without early enum churn. All core tables carry `workspace_id` where isolation matters. Runtime projects store `project_key`, display metadata, and optional `path_digest`; raw local paths stay in the local Runtime configuration, matching the existing bridge catalog safety model.

The Runtime Connector must authenticate with a Runtime token rather than a UI session. The current slice models Runtime identity and status, but token hashing and WebSocket authentication remain part of the next implementation layer.

The existing WeCom and Feishu adapters remain optional entry points. They should call the control plane after external identity binding succeeds, and must not write the core platform tables directly.

## Validation

Run:

```bash
cargo test -p control-plane
cargo test -p runtime-connector-protocol
cargo test --workspace
```

For the whole repository, keep the existing gates:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
pnpm adapters:typecheck
pnpm adapters:build
```
