# Enterprise AI Collaboration MVP Demo

This demo is the local product loop for the enterprise AI collaboration MVP. It exposes the control-plane slice through a small HTTP API and a browser UI.

The Runtime/Provider is explicitly fixture mode: it exercises the Runtime Connector and Codex Provider state transitions without starting a real Codex App Server task.

## Start

```bash
cargo run -p control-plane --bin mvp-demo -- --addr 127.0.0.1:8787
```

Open `http://127.0.0.1:8787`.

The server seeds:

- one active Workspace;
- owner, member, and outsider users;
- one online local fixture Runtime with a redacted project key;
- three public Agents: database, release, and reporting;
- Runtime bindings, audit entries, and empty task/conversation state.

## API

Read state:

```bash
curl -sS http://127.0.0.1:8787/api/snapshot
```

Drive the main flow:

```bash
curl -sS -X POST http://127.0.0.1:8787/api/conversations/start \
  -H 'content-type: application/json' \
  -d '{"prompt":"Inspect the orders incident."}'

curl -sS -X POST http://127.0.0.1:8787/api/runtime/run-next \
  -H 'content-type: application/json' \
  -d '{}'
```

Useful endpoints:

- `POST /api/demo/reset`
- `POST /api/runtime/status` with `{"status":"offline"}` or `{"status":"online"}`
- `POST /api/conversations/start`
- `POST /api/runtime/run-next`
- `POST /api/approvals/decide`
- `POST /api/runs/complete`
- `POST /api/handoffs/create`
- `POST /api/open-tasks/create`
- `POST /api/open-tasks/claim`
- `POST /api/demo/permission-denied`

## Demonstration Paths

1. Public Agent conversation: start a database conversation, run the next task, and inspect the returned Agent message plus audit events.
2. Runtime offline recovery: set Runtime offline, start a conversation, confirm task status is `awaiting_runtime`, set Runtime online, then run the task.
3. Owner approval: start a release Agent conversation, approve the task-level request, run the task with a runtime approval request, approve it, then complete the run.
4. `@Agent` handoff: create a handoff from a non-terminal task to the reporting Agent. A handoff back to the source Agent returns a loop error.
5. Public task pool: create an open task, claim it with the reporting Agent, then run it.
6. Permission failure: run the outsider check and confirm the error is explicit.

## Validation

Focused checks:

```bash
cargo test -p control-plane
```

Full repository gates:

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm adapters:typecheck
pnpm adapters:build
```

The API smoke path used for this handoff covers runtime recovery, owner approval, runtime approval, handoff loop rejection, open-pool claiming, and permission denial.

## Known Limits

- Fixture mode does not execute a real Codex App Server turn.
- State is in memory and resets on process restart or `POST /api/demo/reset`.
- The persistence migration remains a schema contract; this demo does not write to Postgres or SQLite.
- Runtime WebSocket auth, Runtime token revocation, hosted Runtime, multi-Runtime scheduling, multi-provider execution, and cross-Workspace collaboration remain out of scope for the MVP handoff.
