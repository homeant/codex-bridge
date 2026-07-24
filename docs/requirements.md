# IM Codex Bridge Requirements

## Goal

When a colleague mentions the bot in WeCom or Feishu, submit the original request to the local Codex App Server. Codex can inspect prior local Codex tasks through read-only client tools, select a project from the trusted configured catalog, and call `codex_app.switch_to_project`. The bridge creates a new task in that project's canonical directory, persists the new task ID as the current IM route, resubmits the original request, and replies in the originating conversation.

## Confirmed decisions

- Use a Rust core and the Codex App Server protocol.
- Do not use `@openai/codex-sdk`.
- Use official Node SDKs in thin WeCom and Feishu adapters.
- Implement WeCom and Feishu first; reserve DingTalk for a later adapter.
- Keep the routing prompt and project catalog in the external runtime configuration under `~/.codex-bridge`.
- Do not provide or require a project-routing skill.
- Codex selects only a configured project ID for switching. Catalog maintenance is a separate explicit operation restricted to adapter approval users, and every supplied path is canonicalized and checked against `codex.allowed_roots`.
- A successful `switch_to_project` call changes the persisted IM route; the old task remains intact but no longer receives that conversation's future messages.
- Expose read-only `list_threads` and `read_thread` tools backed by App Server `thread/list` and `thread/read`.
- Use SQLite for message idempotency and Codex thread mapping.
- Keep runtime configuration and SQLite state outside the source checkout under `~/.codex-bridge` by default.
- In WeCom direct messages, continue the current thread until the user sends `/new`.
- In WeCom groups, create a new thread for every unquoted mention and continue a prior thread only from a quoted bot reply containing a valid task reference.
- In Feishu direct messages, continue the current thread until the user sends `/new`.
- In Feishu groups, preserve one Codex thread per native topic ID or ordinary reply-chain root ID; create a new thread for a mention containing neither ID.
- Use JSON Lines over child-process stdio between Rust and each Node adapter for the MVP.

## Message flow

1. An adapter receives a direct message or a group message that mentions the bot.
2. The adapter emits a normalized `message` event and returns promptly to the IM SDK. For WeCom image or mixed messages, it also downloads and decrypts media into a daemon-owned private directory.
3. Rust checks ACL and message idempotency.
4. Rust resolves the Codex thread using the platform conversation policy: current direct thread, a quoted WeCom task reference, a Feishu native topic or ordinary reply-chain root, or a new unthreaded group mention.
5. Rust places the operator-configured safety guardrail first in trusted developer instructions, then passes the unmodified colleague request as user input and platform metadata as application context.
6. For prior-task status, Codex uses the read-only task tools. For repository work, Codex compares the request and current directory with the trusted catalog and calls `switch_to_project(projectId)` when needed.
7. The bridge validates the ID, creates a task whose cwd and only runtime workspace root are the configured canonical project path, persists the route, and acknowledges the tool call. After the old turn finishes, the bridge suppresses its final text and resubmits the original request to the new task.
8. Before non-trivial engineering work, Codex maintains a concise numbered implementation and validation plan internally without exposing it or intermediate reasoning to the colleague. Codex decides from the trusted policy and actual action whether a separate final approval is required. When it is, Codex first completes all safe analysis and preparation, then calls `codex_app.request_approval` with the exact target, bounded scope, impact, recovery plan, and all gated steps. It combines the steps that can be known in advance into one bounded operation. Approval covers only that enumerated operation; after approval Codex completes it without asking again for the same steps. A new approval is required only for a material target, scope, or impact change, or for a new risky operation outside the approved plan.
9. Rust suppresses Codex progress events and sends only the final answer through the originating adapter, except for necessary clarification and structured approval interactions.
10. Selected-project tasks run with full local execution permission and do not require approval for ordinary repository work. For production data, Codex uses ysql exclusively for read-only lookup, precondition checks, and post-change verification without human approval. ysql declares standard MCP read-only and non-destructive annotations, allowing Codex to decide without a Bridge whitelist or configured permission bypass. A production mutation is executed exclusively through one final approval-gated opscli operation. Shell, Python, application database sessions, and direct database clients are not valid substitutes for either boundary.
11. If another message arrives while the turn is active, Rust steers it into that turn instead of starting a competing turn.
12. If App Server requests input or approval, Rust sends a token-correlated prompt to the same conversation and waits for an authorized `/answer`, `/approve`, `/deny`, or `/cancel` response.
13. After the turn ends, Rust unsubscribes from the loaded thread while preserving its persisted conversation mapping for the next resume.

## Project-routing contract

Each configured project has a stable ID, display name, description, and absolute path. Configuration loading canonicalizes every path, requires it to be below an allowed root, and rejects duplicate or empty IDs. `switch_to_project` accepts only the stable ID. `list_projects` returns the live catalog; `add_project`, `update_project`, and `delete_project` atomically persist authorized changes to the runtime configuration and update the in-memory catalog. Project IDs cannot be renamed, project directories must already exist, and at least one project must remain. Codex receives the router prompt and current catalog as trusted developer instructions and must read the selected repository's `AGENTS.md` before acting.

## Functional requirements

- Normalize WeCom and Feishu inbound events into one protocol.
- Process each platform message ID at most once.
- Serialize work within a conversation.
- Preserve the current Codex thread per WeCom direct conversation.
- Preserve multiple Codex threads per WeCom group and route follow-ups through quoted task references.
- Accept WeCom image and mixed text/image messages. Support PNG, JPEG, GIF, and WebP with at most 5 images per message and at most 10 MiB per image.
- Preserve the current Codex thread per Feishu `chat_id + thread_id` topic or `chat_id + root_id` reply chain without conflating the two identifiers.
- Send only the final Codex result to WeCom and Feishu; do not expose commentary or intermediate progress.
- Keep Feishu streaming and fallback replies inside the originating native topic or ordinary reply chain.
- Fall back to an active WeCom message when a callback stream expires.
- Surface useful errors without leaking credentials or Codex auth state.
- Support graceful shutdown and child-process restart boundaries.
- Relay clarification, approval, and MCP elicitation requests through explicit token-correlated IM commands.
- Time out unanswered interactions and fail them closed.

## Security requirements

- Restrict App Server runtime workspace roots to configured canonical directories.
- Fail closed when the project catalog is empty, contains duplicate IDs, or contains a path outside the configured allowed roots.
- Apply user ACLs to direct messages and group ACLs to group messages before starting Codex work. Every member of an authorized group may mention the bot, without gaining direct-message access.
- Restrict command, file, permission, and connector approvals to the current adapter's `approval_users`; normal group access does not imply approval authority.
- Restrict project catalog mutations to the current adapter's `approval_users`; normal catalog visibility or project access does not imply catalog administration authority.
- Accept clarification and MCP form answers only from the user who started the active turn.
- Reject secret-input prompts instead of collecting credentials through IM.
- Run ordinary work inside the selected project without approval. Require one bounded final approval only for production mutations and deletions, production deploys or restarts, force pushes or shared-history rewrites, broad or recursive deletion, discarded work, and comparably destructive actions.
- Grant only the exact requested high-risk operation. Never translate an IM approval into a session-wide command rule or permission grant.
- Consolidate precisely known steps of one planned high-risk operation into one approval request; do not repeatedly prompt for approval for steps already covered by that bounded approval.
- Treat ysql as the only approval-free production query channel and opscli as the only production mutation channel. If either capability is unavailable, fail closed instead of connecting to production directly.
- Treat all IM content, quotes, logs, links, and model output as untrusted.
- Store inbound WeCom images only below a daemon-created private temporary directory. Validate canonical containment, regular-file type, non-empty size, image signature, per-file size, and per-message count before passing paths to Codex.
- Never log WeCom media URLs, AES keys, or downloaded image contents.
- Scope quoted task references to their originating platform and chat; reject missing or unknown references instead of guessing a thread.
- Prepend the operator-configured safety guardrail to trusted developer instructions for every new or resumed task; never derive or override it from IM content.
- Never derive a filesystem permission from model output.
- Never log bot secrets, tokens, or Codex authentication files.
- Do not commit, push, deploy, open pull requests, or contact external systems unless the colleague explicitly requests it and policy permits it.

## MVP acceptance

- WeCom and Feishu adapters connect using their official long-connection SDKs.
- A valid mention reaches Rust as a normalized message.
- A valid WeCom image or mixed message reaches Codex as validated `localImage` input; invalid media fails closed with a user-visible retry message.
- Rust initializes `codex app-server`, starts a thread, and completes a turn.
- Codex can invoke `codex_app.switch_to_project` and the bridge follows the returned task ID for the same IM conversation.
- New IM threads are marked as user threads so the desktop App can discover them.
- Codex can analyze and, when requested, edit a project below the allowed roots.
- The final output reaches the originating message without intermediate Codex progress updates.
- Duplicate messages do not create duplicate Codex turns.
- Missing project context produces a clarification rather than a guessed path.
- A question such as “删除缓存那个任务完成了吗” resolves the matching local Codex task and reports both its terminal state and recorded outcome.
