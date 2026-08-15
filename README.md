# IM Codex Bridge

Connect WeCom and Feishu conversations to the local Codex App Server. The bridge forwards the original colleague request, exposes read-only local Codex task lookup, and lets Codex switch the IM conversation to a trusted configured project through `codex_app.switch_to_project`.

See [Requirements](docs/requirements.md) and [Architecture](docs/architecture.md).

For a real WeCom bot smoke test, follow [WeCom end-to-end verification](docs/wecom-e2e.md). The checklist covers Node visibility, Codex login, project configuration, adapter selection, ACLs, expected messages, and common failures.

## Development

```bash
pnpm install
pnpm adapters:build
cargo test --workspace
mkdir -p ~/.codex-bridge
cp config/bridge.example.toml ~/.codex-bridge/bridge.toml
cp .env.example .env
chmod 700 ~/.codex-bridge
chmod 600 ~/.codex-bridge/bridge.toml .env
cargo run -p bridge-daemon -- --check
cargo run -p bridge-daemon
```

Runtime configuration and SQLite state live outside the checkout by default: configuration is `~/.codex-bridge/bridge.toml`, while a relative `state.sqlite_path` is resolved from that same directory. Configure `codex.project_router_prompt` and `[[projects]]` there. Project paths are canonicalized and must remain inside `codex.allowed_roots`; Codex passes only a configured project ID to the bridge. Codex can inspect the live catalog with `codex_app.list_projects`, while `add_project`, `update_project`, and `delete_project` atomically update both the running bridge and `bridge.toml`. Catalog mutations are accepted only when the current IM requester is listed in that adapter's `approval_users`; a new or changed path must already exist below an allowed root, and the final project cannot be deleted. Set each adapter's `cwd` to this repository so its relative script path resolves. Adapter credentials may be injected by the debugger or placed in the ignored repository-root `.env`; disable adapters you are not running.

To select a model or reasoning effort only for IM traffic, set `codex.model`, `codex.model_provider`, and optionally `codex.reasoning_effort` in `~/.codex-bridge/bridge.toml`. The supported reasoning efforts are `none`, `minimal`, `low`, `medium`, `high`, and `xhigh`. Define the provider in `~/.codex/config.toml`; its `env_key` names an environment variable that must be present in the Bridge service process. The bridge applies the model and reasoning effort to IM turns without changing the global Codex model selection.

Set `codex.operator_guardrail` to the operator-maintained safety policy for IM requests. The bridge places it at the beginning of App Server `developerInstructions` for both new and resumed tasks while keeping the colleague's message unmodified. Keep destructive-operation rules here rather than in the project catalog or group message.

Access control is configured independently under each `[[adapters]]` entry through `[adapters.access]`, so WeCom and Feishu IDs never share a namespace. Within one adapter, `allowed_users` controls direct messages, `allowed_groups` controls group messages, and `approval_users` names administrators whose own tasks do not request approval and who may approve command, file, permission, or connector actions requested by other users. Missing access configuration denies every user and group. During an active task, use the short request token shown by the bot with `/answer`, `/approve`, `/deny`, or `/cancel`.

Selected-project tasks start with full local execution permission and App Server's automatic approval prompts disabled, so ordinary repository work does not generate approval cards. ysql declares its tools read-only through standard MCP annotations; Codex evaluates those annotations and the requested action instead of relying on a Bridge whitelist. For a production mutation or comparably destructive action requested by an ordinary user, Codex calls the generic `codex_app.request_approval` tool once immediately before execution, and the WeCom adapter renders a button card with approve and deny actions. When the originating requester is in that adapter's `approval_users`, Bridge adds a trusted per-request instruction telling Codex not to call the approval tool or pause for approval. The bridge never decides from shell-command or MCP-prompt text whether approval is needed. Group members can see and click a card created for an ordinary user's task, but Rust accepts the result only when the clicker's `userid` is in `approval_users`; unauthorized clicks do not consume the request and update only that user's card view with an error. Slash commands remain available as a compatibility fallback.

For ordinary requesters, the bridge's built-in trusted instructions require Codex to decide whether the ready action is high risk and, when it is, call `codex_app.request_approval` after safe analysis and preparation. Printing “waiting for approval” does not create an approval, and an unapproved action must remain unperformed. The trusted approval-user override is appended after this general policy only when Bridge authenticates the originating requester against the current adapter's configuration.

WeCom conversation routing is explicit. Direct messages continue the current Codex task; send `/new` to clear it, or `/new your question` to create a new task immediately. In a group, every unquoted mention creates a new Codex task. Final group replies include a short `Codex任务` reference; quote that reply and mention the bot to continue the referenced task. A quoted message without a valid bridge-generated reference is rejected instead of being guessed.

WeCom accepts single-image and mixed text/image messages. The adapter uses the official SDK to download and decrypt each image, stores validated PNG, JPEG, GIF, or WebP files in a daemon-created private temporary directory, and submits them as Codex App Server `localImage` inputs. Each message is limited to 5 images and each image to 10 MiB; download, decryption, or validation failures produce a retryable user-facing error.

Feishu direct messages continue the current Codex task and support `/new` with the same semantics as WeCom direct messages. In a Feishu group, a native topic uses `chatId + threadId`, while an ordinary reply chain uses `chatId + rootId`; the two identifiers remain distinct. A mention with neither ID starts a new Codex task and registers its `messageId` as the reply-chain root. Subsequent mentions in that topic or reply chain resume the mapped task. Different topics, reply chains, and unthreaded mentions remain isolated.

The Node executable must be on the `PATH` inherited by `bridge-daemon`; having `pnpm` available is not sufficient. Before it starts Codex App Server, the Rust daemon resolves the user's interactive login-shell environment and merges it with its own environment, with explicit daemon values taking precedence. That complete merged environment is passed only to the Codex child process, allowing provider `env_key` values from shell startup files such as `~/.zshrc` to work when the daemon was launched by a desktop application. Shell resolution is non-TTY, bounded by a timeout, and ignores unrelated startup output. WeCom and Feishu adapters inherit debugger-provided environment variables and otherwise load the repository-root `.env` themselves. The Rust daemon does not load `.env`; `BRIDGE_CONFIG` can be exported by the debugger or launching shell to select a non-default configuration path.
