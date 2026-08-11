# Architecture

```text
WeCom ── official Node adapter ──┐
                                ├─ JSONL/stdin/stdout ─ Rust bridge ─ JSON-RPC/stdio ─ codex app-server
Feishu ─ official Node adapter ──┘                                      │
                                                                       └─ trusted project-switch tool
```

## Boundaries

### Node adapters

Adapters own only platform protocol details: authentication, long connections, mention filtering, quoted-message extraction, event normalization, streaming reply APIs, and platform error fallback. The WeCom adapter also downloads and decrypts inbound media, validates its image signature and limits, and saves it only in the daemon-provided private media directory. Adapters write logs to stderr and protocol messages to stdout.

Feishu reply commands carry the normalized native topic ID back to the adapter. Both the initial stream and any fallback send reply to the originating message; `replyInThread` is enabled only for a real `threadId`, while ordinary `rootId` reply chains remain normal replies.

### Rust bridge

The bridge owns process supervision, ACLs, idempotency, conversation serialization, SQLite state, App Server JSON-RPC, explicit conversation routing, thread reuse, project-ID validation, the approval-user-gated live project catalog, final-result delivery, in-flight turn steering, and token-correlated IM interactions. Codex selects the project; the bridge resolves only trusted configured IDs to canonical paths.

### Codex

Codex receives the original request as user text and IM metadata as application context. The operator-configured safety guardrail, routing prompt, and current project catalog are placed in trusted App Server `developerInstructions`, never in the untrusted user text. For status questions Codex can call read-only `codex_app.list_threads` and `codex_app.read_thread`. For repository work it calls `codex_app.switch_to_project` with a configured ID when the current cwd is not already correct. `codex_app.list_projects` reads the live catalog, while catalog mutations require the originating IM user to be an adapter approval user and are persisted atomically before the in-memory catalog changes. The bridge does not classify the request or read Codex's private SQLite/rollout files.

Production data access uses capability routing rather than shell-command classification. ysql is the approval-free, read-only, field-masked channel for lookup, preconditions, and verification. Its MCP tools publish standard read-only, non-destructive annotations so Codex can make the approval decision from capability metadata and action semantics; the Bridge neither parses elicitation text nor keeps a tool allowlist. Codex cannot use shell, Python, an application database session, or a direct database client as a substitute. The only production write is one bounded opscli execution after all ysql analysis is complete; that execution receives the single structured approval for the change. Post-change verification returns to ysql without another approval.

## Adapter protocol

Inbound example with an optional normalized quote:

```json
{"type":"message","platform":"wecom","message_id":"msg-1","chat_id":"chat-1","chat_type":"group","user_id":"u-1","content":"继续检查这个问题","image_paths":["/private/daemon-media/example.png"],"mentioned_bot":true,"quoted_text":"上一次结论……\n\n[Codex任务:R8K3P2Q7W9XZ]"}
```

Feishu keeps native topics and ordinary reply chains in separate fields:

```json
{"type":"message","platform":"lark","message_id":"msg-2","chat_id":"chat-1","chat_type":"group","user_id":"u-1","content":"继续检查","mentioned_bot":true,"thread_id":"topic-1","root_id":"root-message-1"}
```

Rust gives `thread_id` routing precedence when both are present. Without a native topic, `root_id` restores an ordinary reply chain; when both are absent, `message_id` becomes the root of a new chain.

`image_paths` is optional. The Rust bridge canonicalizes every path, requires a regular non-empty file below the daemon-created media root, enforces count and size limits again, and rejects the whole message on any mismatch.

Outbound example:

```json
{"type":"reply","platform":"wecom","message_id":"msg-1","chat_id":"chat-1","content":"正在检查…","finished":false}
```

## App Server subset

The MVP deliberately implements a narrow, version-tolerant subset:

- client request/response multiplexing by JSON-RPC ID;
- `initialize` followed by `initialized`;
- `thread/start` for a read-only routing task, and a nested full-access `thread/start` with the selected project as cwd and sole runtime workspace root;
- `turn/start` with unmodified user text, optional `localImage` items, and application metadata;
- `turn/steer` with text and optional `localImage` items for additional messages received while the conversation already has an active turn;
- `thread/unsubscribe` after terminal turn handling so inactive conversations can be unloaded;
- `item/tool/call` callbacks mapped to `thread/list`, `thread/read`, and the Bridge-owned `switch_to_project`;
- `item/agentMessage/delta`, `item/completed`, and `turn/completed` notifications;
- server requests for user input, command/file/permission approval, connector approval, and MCP elicitation, correlated back to the originating IM conversation with short-lived tokens;
- fail-closed timeout handling and rejection of secret-input prompts in IM.

Unknown notifications are ignored. Unknown server requests receive a JSON-RPC method-not-found error so protocol additions do not crash the bridge.

Approval commands are accepted only from the current adapter's `approval_users`. When the current requester is one of those users, Bridge appends a trusted developer instruction telling Codex not to call `codex_app.request_approval`, request human approval, or pause for approval; the instruction is regenerated when a task starts, resumes, or switches projects. Clarification and MCP form answers are still accepted only from the user who started the active turn. Selected-project tasks run with `danger-full-access` and `approvalPolicy=never`, so App Server does not interrupt ordinary repository work. For other requesters, Codex decides from the trusted policy and actual action whether a final gate is required; for one bounded production mutation or comparably destructive action it calls `codex_app.request_approval` immediately before execution. The Bridge validates the structured fields and returns `approved=true|false` without classifying the action itself. Approvals grant only the enumerated operation, never a session-wide rule. Once approved, the same planned steps must not generate repeated approval requests. A material target, scope, or impact change still requires a new approval. Ordinary Codex progress events are not forwarded to IM; interaction prompts and the final result are delivered separately.

Approval presentation is driven exclusively by typed App Server callbacks, not by bridge-side inspection of command text. For ordinary-user requests, WeCom command, file, permission, and connector approval callbacks finish the current Markdown stream and then actively send a separate `button_interaction` template card. The card is deliberately not appended through `stream_with_template_card`, because WeCom clients can silently drop a card added after earlier stream updates. A card click is normalized back into the existing token-correlated approval path, where Rust checks the WeCom adapter's `approval_users`, conversation scope, card `task_id`, timeout, and single-use state before replying to App Server. Successful decisions replace the shared card; an unauthorized or stale click updates only the clicker's view.

Because prose cannot create a typed callback, `codex_app.request_approval` is a generic dynamic tool. Codex supplies the risk category, exact target, bounded scope, impact, recovery plan, and every covered step after safe analysis. The Bridge pauses that tool call until the authorized user decides; the actual high-risk operation is executed only after the tool returns `approved=true`.

The operator guardrail reduces the chance that untrusted IM text can induce a destructive action, but it is not a substitute for App Server sandboxing and explicit approval callbacks. Actions that do not trigger an App Server approval still depend on the configured workspace boundary and model compliance.

## Inbound WeCom images

The daemon creates a private temporary media directory at startup and passes only that directory to the WeCom adapter. For image and mixed messages, the adapter asks the official SDK to download and decrypt each image, checks PNG/JPEG/GIF/WebP magic bytes, enforces 5-image and 10-MiB limits, and writes files with owner-only permissions. The Rust bridge independently canonicalizes and validates those paths before building Codex `localImage` input items. Files are scheduled for expiry by the adapter and the whole directory is removed when the daemon exits.

## State model

SQLite stores processed message IDs, conversation mappings, and WeCom group task-reference mappings. Runtime configuration defaults to `~/.codex-bridge/bridge.toml`; relative SQLite paths are resolved from the configuration directory, so the example `bridge.sqlite3` lives beside it rather than in the source checkout. A WeCom or Feishu direct message keeps one current thread and `/new` clears that pointer. Each unquoted WeCom group mention creates a new thread; the bridge persists `platform + chat_id + task reference → thread_id` and appends the reference to the final reply. Quoting that reply restores the matching thread. The group scope prevents a copied reference from resolving in another group. For Feishu groups, a native `threadId` maps a topic to one persisted Codex thread, while `rootId` independently maps an ordinary reply chain. A mention containing neither ID creates a new task under `chat_id + message_id`, allowing later replies carrying that message as `rootId` to resume it.

Optional `codex.model` and `codex.model_provider` values select a model only for App Server tasks created or resumed through the IM bridge. The provider remains defined in the user's Codex configuration, and its credential stays in the environment variable named by that provider's `env_key`.

Active turns and pending interaction tokens are intentionally in-memory only; restarting the bridge fails those requests closed with the App Server process. Persisted direct and group task mappings survive restart.
