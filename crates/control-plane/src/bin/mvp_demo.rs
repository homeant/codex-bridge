use std::{
    env,
    error::Error,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
};

use control_plane::{
    AcceptancePolicy, AgentCapability, AgentDefinition, AgentVisibility, ApprovalStatus,
    ControlPlaneError, ControlPlaneSnapshot, MvpControlPlane, RuntimeProject, RuntimeProjectStatus,
    RuntimeStatus, TaskStatus, TaskType, WorkspaceRole,
};
use runtime_connector_protocol::{RunApprovalRequested, RunCompleted, Sensitivity};
use serde::Serialize;
use serde_json::{Value, json};

const DEFAULT_ADDR: &str = "127.0.0.1:8787";
const PROJECT_KEY: &str = "orders-api";

fn main() -> Result<(), Box<dyn Error>> {
    let addr = parse_addr();
    let listener = TcpListener::bind(&addr)?;
    let state = Arc::new(Mutex::new(DemoState::seeded()?));
    println!("Enterprise AI collaboration MVP demo listening on http://{addr}");
    println!("Provider mode: local fixture runtime; no real Codex task is executed.");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle_connection(stream, state.clone()) {
                    eprintln!("demo request failed: {error}");
                }
            }
            Err(error) => eprintln!("demo connection failed: {error}"),
        }
    }
    Ok(())
}

fn parse_addr() -> String {
    let mut args = env::args().skip(1);
    let mut addr = env::var("MVP_DEMO_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_owned());
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--addr" => {
                if let Some(value) = args.next() {
                    addr = value;
                }
            }
            "--port" => {
                if let Some(value) = args.next() {
                    addr = format!("127.0.0.1:{value}");
                }
            }
            "--help" | "-h" => {
                println!(
                    "Usage: cargo run -p control-plane --bin mvp-demo -- [--addr 127.0.0.1:8787]"
                );
                std::process::exit(0);
            }
            _ => {}
        }
    }
    addr
}

#[derive(Clone, Serialize)]
struct DemoIds {
    owner_user_id: String,
    requester_user_id: String,
    outsider_user_id: String,
    workspace_id: String,
    runtime_id: String,
    database_agent_id: String,
    release_agent_id: String,
    reporting_agent_id: String,
    project_key: String,
}

#[derive(Serialize)]
struct DemoSnapshot {
    provider_mode: &'static str,
    provider_warning: &'static str,
    ids: DemoIds,
    last_event: String,
    state: ControlPlaneSnapshot,
}

struct DemoState {
    plane: MvpControlPlane,
    ids: DemoIds,
    last_event: String,
}

impl DemoState {
    fn seeded() -> Result<Self, ControlPlaneError> {
        let mut plane = MvpControlPlane::default();
        let owner = plane.seed_user("owner@example.com", "Li Si");
        let requester = plane.seed_user("requester@example.com", "Zhang San");
        let outsider = plane.seed_user("outsider@example.com", "Workspace Outsider");
        let workspace_id = plane.create_workspace(&owner, "Acme AI Collaboration")?;
        plane.add_member(&owner, &workspace_id, &requester, WorkspaceRole::Member)?;
        let runtime_id = plane.register_runtime(
            &owner,
            &workspace_id,
            "Li Si local fixture runtime",
            vec![RuntimeProject {
                project_key: PROJECT_KEY.into(),
                display_name: "Orders API".into(),
                path_digest: Some("sha256:demo-redacted-path".into()),
                status: RuntimeProjectStatus::Active,
            }],
        )?;
        plane.runtime_heartbeat(&runtime_id, RuntimeStatus::Online)?;
        let database_agent_id = create_agent(
            &mut plane,
            &owner,
            &workspace_id,
            "Database Agent",
            "Read-only database diagnostics",
            "database",
            AcceptancePolicy::AutoAcceptLowRisk,
        )?;
        let release_agent_id = create_agent(
            &mut plane,
            &owner,
            &workspace_id,
            "Release Agent",
            "Release checks that require owner confirmation",
            "release",
            AcceptancePolicy::RequiresOwnerApproval,
        )?;
        let reporting_agent_id = create_agent(
            &mut plane,
            &owner,
            &workspace_id,
            "Reporting Agent",
            "Summarizes completed work and audit evidence",
            "reporting",
            AcceptancePolicy::AutoAcceptLowRisk,
        )?;
        for agent_id in [&database_agent_id, &release_agent_id, &reporting_agent_id] {
            plane.bind_agent_runtime(&owner, agent_id, &runtime_id, PROJECT_KEY)?;
        }
        Ok(Self {
            plane,
            ids: DemoIds {
                owner_user_id: owner,
                requester_user_id: requester,
                outsider_user_id: outsider,
                workspace_id,
                runtime_id,
                database_agent_id,
                release_agent_id,
                reporting_agent_id,
                project_key: PROJECT_KEY.into(),
            },
            last_event: "demo fixture seeded".into(),
        })
    }

    fn snapshot(&self) -> DemoSnapshot {
        DemoSnapshot {
            provider_mode: "fixture",
            provider_warning: "local fixture runtime only; no real Codex App Server task is executed",
            ids: self.ids.clone(),
            last_event: self.last_event.clone(),
            state: self.plane.snapshot(),
        }
    }
}

fn create_agent(
    plane: &mut MvpControlPlane,
    owner: &str,
    workspace_id: &str,
    name: &str,
    description: &str,
    capability: &str,
    acceptance_policy: AcceptancePolicy,
) -> Result<String, ControlPlaneError> {
    plane.create_agent(
        owner,
        workspace_id,
        AgentDefinition {
            name: name.into(),
            description: description.into(),
            visibility: AgentVisibility::Public,
            acceptance_policy,
            capabilities: vec![AgentCapability {
                capability_key: capability.into(),
                display_name: capability.into(),
                sensitivity: Sensitivity::Internal,
            }],
        },
    )
}

fn handle_connection(
    mut stream: TcpStream,
    state: Arc<Mutex<DemoState>>,
) -> Result<(), Box<dyn Error>> {
    let request = HttpRequest::read(&mut stream)?;
    let response = route(request, state);
    stream.write_all(&response)?;
    stream.flush()?;
    Ok(())
}

fn route(request: HttpRequest, state: Arc<Mutex<DemoState>>) -> Vec<u8> {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => html_response(INDEX_HTML),
        ("GET", "/api/snapshot") => with_state(&state, |demo| ok_json(json!(demo.snapshot()))),
        ("POST", "/api/demo/reset") => with_state_mut(&state, |demo| {
            *demo = DemoState::seeded()?;
            Ok(ok_json(json!({
                "ok": true,
                "result": { "reset": true },
                "snapshot": demo.snapshot()
            })))
        }),
        ("POST", "/api/runtime/status") => with_body_state(&request.body, &state, runtime_status),
        ("POST", "/api/conversations/start") => {
            with_body_state(&request.body, &state, start_conversation)
        }
        ("POST", "/api/runtime/run-next") => with_body_state(&request.body, &state, run_next),
        ("POST", "/api/approvals/decide") => {
            with_body_state(&request.body, &state, decide_approval)
        }
        ("POST", "/api/runs/complete") => with_body_state(&request.body, &state, complete_run),
        ("POST", "/api/handoffs/create") => with_body_state(&request.body, &state, create_handoff),
        ("POST", "/api/open-tasks/create") => {
            with_body_state(&request.body, &state, create_open_task)
        }
        ("POST", "/api/open-tasks/claim") => {
            with_body_state(&request.body, &state, claim_open_task)
        }
        ("POST", "/api/demo/permission-denied") => {
            with_body_state(&request.body, &state, permission_denied)
        }
        _ => json_response(404, json!({ "ok": false, "error": "not found" })),
    }
}

fn with_state(
    state: &Arc<Mutex<DemoState>>,
    handler: impl FnOnce(&DemoState) -> Vec<u8>,
) -> Vec<u8> {
    match state.lock() {
        Ok(demo) => handler(&demo),
        Err(_) => json_response(500, json!({ "ok": false, "error": "state lock poisoned" })),
    }
}

fn with_state_mut(
    state: &Arc<Mutex<DemoState>>,
    handler: impl FnOnce(&mut DemoState) -> Result<Vec<u8>, ControlPlaneError>,
) -> Vec<u8> {
    match state.lock() {
        Ok(mut demo) => match handler(&mut demo) {
            Ok(response) => response,
            Err(error) => json_response(409, json!({ "ok": false, "error": error.to_string() })),
        },
        Err(_) => json_response(500, json!({ "ok": false, "error": "state lock poisoned" })),
    }
}

fn with_body_state(
    body: &[u8],
    state: &Arc<Mutex<DemoState>>,
    handler: fn(Value, &mut DemoState) -> Result<Value, ControlPlaneError>,
) -> Vec<u8> {
    let body = if body.is_empty() {
        Value::Object(Default::default())
    } else {
        match serde_json::from_slice(body) {
            Ok(value) => value,
            Err(error) => {
                return json_response(
                    400,
                    json!({ "ok": false, "error": format!("invalid JSON body: {error}") }),
                );
            }
        }
    };
    with_state_mut(state, |demo| {
        let result = handler(body, demo)?;
        Ok(ok_json(json!({
            "ok": true,
            "result": result,
            "snapshot": demo.snapshot()
        })))
    })
}

fn runtime_status(body: Value, demo: &mut DemoState) -> Result<Value, ControlPlaneError> {
    let runtime_id = string_or(&body, "runtime_id", &demo.ids.runtime_id);
    let status = match string_or(&body, "status", "online").as_str() {
        "offline" => RuntimeStatus::Offline,
        "connecting" => RuntimeStatus::Connecting,
        "online" => RuntimeStatus::Online,
        "busy" => RuntimeStatus::Busy,
        "degraded" => RuntimeStatus::Degraded,
        value => {
            return Err(ControlPlaneError::Validation(format!(
                "unsupported runtime status: {value}"
            )));
        }
    };
    demo.plane.runtime_heartbeat(&runtime_id, status)?;
    demo.last_event = format!("runtime {runtime_id} status set to {status:?}");
    Ok(json!({ "runtime_id": runtime_id, "status": status }))
}

fn start_conversation(body: Value, demo: &mut DemoState) -> Result<Value, ControlPlaneError> {
    let requester = string_or(&body, "requester_user_id", &demo.ids.requester_user_id);
    let workspace_id = string_or(&body, "workspace_id", &demo.ids.workspace_id);
    let agent_id = string_or(&body, "agent_id", &demo.ids.database_agent_id);
    let prompt = string_or(
        &body,
        "prompt",
        "Inspect the latest order-processing incident and return a short result.",
    );
    let start =
        demo.plane
            .start_agent_conversation(&requester, &workspace_id, &agent_id, &prompt)?;
    demo.last_event = format!("conversation {} started", start.conversation_id);
    Ok(json!({ "conversation_start": start }))
}

fn run_next(body: Value, demo: &mut DemoState) -> Result<Value, ControlPlaneError> {
    let runtime_id = string_or(&body, "runtime_id", &demo.ids.runtime_id);
    let request_approval = bool_or(&body, "request_approval", false);
    let offered = demo.plane.offer_next_task(&runtime_id)?;
    let task_id = match offered {
        Some(offer) => offer.task_id,
        None => {
            first_awaiting_acceptance(&demo.plane.snapshot(), &runtime_id).ok_or_else(|| {
                ControlPlaneError::InvalidState("no queued or offered task for runtime".into())
            })?
        }
    };
    let run_start = demo.plane.runtime_accept_task(&runtime_id, &task_id)?;
    let mut response = json!({ "run_start": run_start });
    if request_approval {
        let approval_id = demo.plane.runtime_request_approval(
            &runtime_id,
            RunApprovalRequested {
                run_id: response["run_start"]["run_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                action_type: string_or(&body, "action_type", "demo_protected_operation"),
                target: string_or(&body, "target", "demo://orders-api/protected-change"),
                scope: string_or(&body, "scope", "single fixture run"),
                impact: string_or(
                    &body,
                    "impact",
                    "demonstrates owner approval before continuing",
                ),
                recovery_plan: string_or(&body, "recovery_plan", "deny the fixture approval"),
                steps: vec!["fixture runtime waits for approval".into()],
                requested_payload: json!({ "mode": "fixture" }),
            },
        )?;
        response["approval_request_id"] = json!(approval_id);
        demo.last_event = "fixture runtime requested approval".into();
    } else {
        let run_id = response["run_start"]["run_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        demo.plane.runtime_complete_run(
            &runtime_id,
            RunCompleted {
                run_id: run_id.clone(),
                result_text: fixture_result(&response["run_start"]["prompt"]),
                result_ref: Some(format!("fixture://runs/{run_id}")),
            },
        )?;
        response["completed"] = json!(true);
        demo.last_event = format!("fixture runtime completed run {run_id}");
    }
    Ok(response)
}

fn decide_approval(body: Value, demo: &mut DemoState) -> Result<Value, ControlPlaneError> {
    let approval_id = string_or_else(&body, "approval_id", || {
        first_pending_approval(&demo.plane.snapshot()).unwrap_or_default()
    });
    if approval_id.is_empty() {
        return Err(ControlPlaneError::InvalidState(
            "no pending approval request".into(),
        ));
    }
    let approver = string_or(&body, "approver_user_id", &demo.ids.owner_user_id);
    let approved = bool_or(&body, "approved", true);
    let outcome = demo.plane.decide_approval(
        &approver,
        &approval_id,
        approved,
        Some("demo decision".into()),
    )?;
    demo.last_event = format!("approval {approval_id} decided: {approved}");
    Ok(json!({ "approval_outcome": outcome }))
}

fn complete_run(body: Value, demo: &mut DemoState) -> Result<Value, ControlPlaneError> {
    let runtime_id = string_or(&body, "runtime_id", &demo.ids.runtime_id);
    let run_id = string_or_else(&body, "run_id", || {
        first_running_run(&demo.plane.snapshot(), &runtime_id).unwrap_or_default()
    });
    if run_id.is_empty() {
        return Err(ControlPlaneError::InvalidState("no running run".into()));
    }
    let result_text = string_or(
        &body,
        "result_text",
        "Fixture provider completed the approved task and returned a visible result.",
    );
    demo.plane.runtime_complete_run(
        &runtime_id,
        RunCompleted {
            run_id: run_id.clone(),
            result_text: result_text.clone(),
            result_ref: Some(format!("fixture://runs/{run_id}")),
        },
    )?;
    demo.last_event = format!("run {run_id} completed");
    Ok(json!({ "run_id": run_id, "result_text": result_text }))
}

fn create_handoff(body: Value, demo: &mut DemoState) -> Result<Value, ControlPlaneError> {
    let parent_task_id = string_or_else(&body, "parent_task_id", || {
        first_non_terminal_assigned_task(&demo.plane.snapshot()).unwrap_or_default()
    });
    if parent_task_id.is_empty() {
        return Err(ControlPlaneError::InvalidState(
            "no non-terminal assigned task available for handoff".into(),
        ));
    }
    let target_agent_id = string_or(&body, "target_agent_id", &demo.ids.reporting_agent_id);
    let requester = string_or(&body, "requester_user_id", &demo.ids.requester_user_id);
    let reason = string_or(
        &body,
        "reason",
        "Summarize the current run for audit handoff.",
    );
    let handoff = demo.plane.request_handoff(
        &requester,
        &parent_task_id,
        &target_agent_id,
        &reason,
        runtime_connector_protocol::SharedContextManifest {
            summary: "Demo handoff shares only the user request and audit summary.".into(),
            items: vec![runtime_connector_protocol::SharedContextItem {
                label: "redacted request".into(),
                kind: runtime_connector_protocol::SharedContextKind::UserRequest,
                sensitivity: Sensitivity::Internal,
                redacted: true,
            }],
        },
    )?;
    demo.last_event = format!("handoff task {} created", handoff.task_id);
    Ok(json!({ "handoff": handoff }))
}

fn create_open_task(body: Value, demo: &mut DemoState) -> Result<Value, ControlPlaneError> {
    let requester = string_or(&body, "requester_user_id", &demo.ids.requester_user_id);
    let workspace_id = string_or(&body, "workspace_id", &demo.ids.workspace_id);
    let prompt = string_or(&body, "prompt", "Summarize drift in the public task pool.");
    let task_id = demo.plane.create_open_task(
        &requester,
        &workspace_id,
        &prompt,
        vec!["reporting".into()],
        Sensitivity::Internal,
    )?;
    demo.last_event = format!("open task {task_id} created");
    Ok(json!({ "task_id": task_id }))
}

fn claim_open_task(body: Value, demo: &mut DemoState) -> Result<Value, ControlPlaneError> {
    let task_id = string_or_else(&body, "task_id", || {
        first_open_task(&demo.plane.snapshot()).unwrap_or_default()
    });
    if task_id.is_empty() {
        return Err(ControlPlaneError::InvalidState(
            "no queued open task to claim".into(),
        ));
    }
    let owner = string_or(&body, "owner_user_id", &demo.ids.owner_user_id);
    let agent_id = string_or(&body, "agent_id", &demo.ids.reporting_agent_id);
    let offer = demo.plane.claim_open_task(&owner, &task_id, &agent_id)?;
    demo.last_event = format!("open task {task_id} claimed");
    Ok(json!({ "offer": offer }))
}

fn permission_denied(body: Value, demo: &mut DemoState) -> Result<Value, ControlPlaneError> {
    let workspace_id = string_or(&body, "workspace_id", &demo.ids.workspace_id);
    let agent_id = string_or(&body, "agent_id", &demo.ids.database_agent_id);
    match demo.plane.start_agent_conversation(
        &demo.ids.outsider_user_id,
        &workspace_id,
        &agent_id,
        "outsider should not be able to call this agent",
    ) {
        Ok(_) => Err(ControlPlaneError::InvalidState(
            "permission check unexpectedly succeeded".into(),
        )),
        Err(error) => {
            demo.last_event = format!("permission denied path returned: {error}");
            Ok(json!({ "expected_error": error.to_string() }))
        }
    }
}

fn first_awaiting_acceptance(snapshot: &ControlPlaneSnapshot, runtime_id: &str) -> Option<String> {
    snapshot
        .tasks
        .iter()
        .find(|task| {
            task.status == TaskStatus::AwaitingAcceptance
                && task.assigned_runtime_id.as_deref() == Some(runtime_id)
        })
        .map(|task| task.id.clone())
}

fn first_pending_approval(snapshot: &ControlPlaneSnapshot) -> Option<String> {
    snapshot
        .approvals
        .iter()
        .find(|approval| approval.status == ApprovalStatus::Pending)
        .map(|approval| approval.id.clone())
}

fn first_running_run(snapshot: &ControlPlaneSnapshot, runtime_id: &str) -> Option<String> {
    snapshot
        .runs
        .iter()
        .find(|run| run.runtime_id == runtime_id && run.status == control_plane::RunStatus::Running)
        .map(|run| run.id.clone())
}

fn first_non_terminal_assigned_task(snapshot: &ControlPlaneSnapshot) -> Option<String> {
    snapshot
        .tasks
        .iter()
        .find(|task| {
            task.assigned_agent_id.is_some()
                && !matches!(
                    task.status,
                    TaskStatus::Completed
                        | TaskStatus::Failed
                        | TaskStatus::Rejected
                        | TaskStatus::Cancelled
                        | TaskStatus::Expired
                )
        })
        .map(|task| task.id.clone())
}

fn first_open_task(snapshot: &ControlPlaneSnapshot) -> Option<String> {
    snapshot
        .tasks
        .iter()
        .find(|task| task.task_type == TaskType::Open && task.status == TaskStatus::Queued)
        .map(|task| task.id.clone())
}

fn fixture_result(prompt: &Value) -> String {
    let prompt = prompt.as_str().unwrap_or("demo task");
    format!("Fixture provider result: completed `{prompt}` with redacted local project context.")
}

fn string_or(body: &Value, key: &str, fallback: &str) -> String {
    body.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

fn string_or_else(body: &Value, key: &str, fallback: impl FnOnce() -> String) -> String {
    body.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(fallback)
}

fn bool_or(body: &Value, key: &str, fallback: bool) -> bool {
    body.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

impl HttpRequest {
    fn read(stream: &mut TcpStream) -> Result<Self, Box<dyn Error>> {
        let mut data = Vec::new();
        let mut buffer = [0_u8; 8192];
        let mut header_end = None;
        let mut content_length = 0_usize;
        loop {
            let read = stream.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..read]);
            if header_end.is_none()
                && let Some(index) = find_header_end(&data)
            {
                header_end = Some(index);
                let headers = String::from_utf8_lossy(&data[..index]);
                content_length = parse_content_length(&headers);
            }
            if let Some(index) = header_end
                && data.len() >= index + 4 + content_length
            {
                break;
            }
            if data.len() > 1024 * 1024 {
                return Err("request too large".into());
            }
        }
        let Some(index) = header_end else {
            return Err("malformed HTTP request".into());
        };
        let headers = String::from_utf8_lossy(&data[..index]);
        let request_line = headers.lines().next().ok_or("missing request line")?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next().ok_or("missing method")?.to_owned();
        let raw_path = parts.next().ok_or("missing path")?;
        let path = raw_path.split('?').next().unwrap_or(raw_path).to_owned();
        let body_start = index + 4;
        let body_end = body_start + content_length;
        Ok(Self {
            method,
            path,
            body: data[body_start..body_end].to_vec(),
        })
    }
}

fn find_header_end(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &str) -> usize {
    headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn ok_json(value: Value) -> Vec<u8> {
    json_response(200, value)
}

fn json_response(status: u16, value: Value) -> Vec<u8> {
    let body = serde_json::to_vec_pretty(&value).unwrap_or_else(|_| b"{}".to_vec());
    response(status, "application/json; charset=utf-8", body)
}

fn html_response(html: &str) -> Vec<u8> {
    response(200, "text/html; charset=utf-8", html.as_bytes().to_vec())
}

fn response(status: u16, content_type: &str, body: Vec<u8>) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        409 => "Conflict",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend(body);
    response
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>企业 AI 协作 MVP 演示台</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f6f7f9;
      --surface: #ffffff;
      --ink: #1d232b;
      --muted: #667085;
      --line: #d8dde5;
      --blue: #2563eb;
      --teal: #0f766e;
      --green: #16834a;
      --red: #c2410c;
      --amber: #b7791f;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background: var(--bg);
      color: var(--ink);
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      font-size: 14px;
      letter-spacing: 0;
    }
    header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      padding: 18px 24px;
      border-bottom: 1px solid var(--line);
      background: var(--surface);
      position: sticky;
      top: 0;
      z-index: 2;
    }
    h1 {
      margin: 0;
      font-size: 22px;
      font-weight: 650;
    }
    p { margin: 0; }
    main {
      display: grid;
      grid-template-columns: 320px minmax(0, 1fr);
      gap: 18px;
      padding: 18px 24px 28px;
    }
    section, aside {
      background: var(--surface);
      border: 1px solid var(--line);
      border-radius: 8px;
    }
    aside {
      padding: 14px;
      align-self: start;
      position: sticky;
      top: 74px;
    }
    .stack { display: grid; gap: 14px; }
    .eyebrow {
      color: var(--teal);
      font-size: 12px;
      font-weight: 700;
      margin-bottom: 4px;
    }
    .summary {
      display: grid;
      gap: 6px;
      padding: 11px 12px;
      background: #f7fbfa;
      border: 1px solid #b7d8d3;
      border-radius: 8px;
      color: #164e45;
      line-height: 1.5;
    }
    .summary strong { color: #0f3f38; }
    .group {
      display: grid;
      gap: 8px;
    }
    .group-title {
      color: #475467;
      font-size: 12px;
      font-weight: 700;
    }
    .toolbar {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 8px;
    }
    button {
      min-height: 34px;
      border: 1px solid #bac3d0;
      background: #fff;
      color: var(--ink);
      border-radius: 6px;
      padding: 7px 10px;
      font: inherit;
      cursor: pointer;
      text-align: left;
      display: grid;
      gap: 2px;
    }
    button strong { font-weight: 650; }
    button small {
      color: var(--muted);
      font-size: 12px;
      line-height: 1.35;
    }
    button.primary {
      background: var(--blue);
      border-color: var(--blue);
      color: #fff;
      text-align: center;
    }
    button.primary small { color: #dbeafe; }
    button:hover { border-color: var(--blue); }
    button:disabled { color: #98a2b3; cursor: not-allowed; }
    .status {
      display: flex;
      align-items: center;
      gap: 8px;
      color: var(--muted);
      min-width: 0;
    }
    .dot {
      width: 9px;
      height: 9px;
      border-radius: 50%;
      background: var(--green);
      flex: 0 0 auto;
    }
    .muted { color: var(--muted); }
    .event-label {
      color: #475467;
      font-size: 12px;
      font-weight: 700;
    }
    .content {
      display: grid;
      gap: 18px;
      min-width: 0;
    }
    .panel { overflow: hidden; }
    .panel h2 {
      margin: 0;
      padding: 12px 14px;
      font-size: 15px;
      border-bottom: 1px solid var(--line);
      background: #fbfcfe;
    }
    .panel-body { padding: 12px 14px; overflow-x: auto; }
    .explainer {
      display: grid;
      gap: 12px;
      line-height: 1.55;
    }
    .metrics {
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 8px;
    }
    .metric {
      display: grid;
      gap: 4px;
      padding: 9px 10px;
      border: 1px solid #edf0f4;
      border-radius: 8px;
      background: #fbfcfe;
      min-width: 0;
    }
    .metric strong {
      color: var(--ink);
      font-size: 13px;
    }
    .metric span {
      color: var(--muted);
      font-size: 12px;
      line-height: 1.35;
    }
    table {
      width: 100%;
      border-collapse: collapse;
      min-width: 680px;
    }
    th, td {
      padding: 8px 9px;
      border-bottom: 1px solid #edf0f4;
      vertical-align: top;
      text-align: left;
      overflow-wrap: anywhere;
    }
    th {
      color: #475467;
      font-size: 12px;
      font-weight: 650;
      background: #fbfcfe;
    }
    .pill {
      display: inline-flex;
      align-items: center;
      min-height: 22px;
      padding: 2px 8px;
      border-radius: 999px;
      border: 1px solid var(--line);
      color: #344054;
      background: #f9fafb;
      white-space: nowrap;
    }
    .mono {
      color: #344054;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
    }
    .queued, .online, .completed, .approved, .active { color: var(--green); border-color: #a7d7bd; background: #effaf4; }
    .awaitingapproval, .awaitingruntime, .awaitingacceptance, .pending, .running { color: var(--amber); border-color: #e7c77c; background: #fff8e6; }
    .rejected, .failed, .offline, .cancelled { color: var(--red); border-color: #f0b399; background: #fff4ef; }
    .event-log {
      min-height: 40px;
      padding: 10px 12px;
      background: #111827;
      color: #e5e7eb;
      border-radius: 6px;
      overflow-wrap: anywhere;
    }
    @media (max-width: 900px) {
      main { grid-template-columns: 1fr; padding: 14px; }
      aside { position: static; }
      header { align-items: flex-start; flex-direction: column; }
      .metrics { grid-template-columns: 1fr 1fr; }
    }
    @media (max-width: 560px) {
      .toolbar, .metrics { grid-template-columns: 1fr; }
    }
  </style>
</head>
<body>
  <header>
    <div>
      <div class="eyebrow">本地可运行演示</div>
      <h1>企业 AI 协作 MVP 演示台</h1>
      <div class="muted">演示成员如何把工作交给公开 Agent，本地 Runtime 如何接单、审批、回传结果并留下审计记录。</div>
    </div>
    <div class="status"><span class="dot"></span><span id="mode">Loading</span></div>
  </header>
  <main>
    <aside class="stack">
      <div class="summary">
        <strong>这页看什么</strong>
        <span>左侧按顺序触发业务动作，右侧实时展示 Agent、任务、会话消息和审计证据。</span>
      </div>
      <div class="group">
        <div class="group-title">推荐主流程</div>
        <button class="primary" data-action="reset"><strong>重置演示数据</strong><small>回到干净工作区</small></button>
        <button data-action="start-db"><strong>1. 成员向公开 Agent 提问</strong><small>生成会话和定向任务</small></button>
        <button data-action="run-next"><strong>2. Runtime 接单并返回结果</strong><small>任务完成后消息和审计会更新</small></button>
      </div>
      <div class="group">
        <div class="group-title">审批与异常场景</div>
        <button data-action="start-release"><strong>发起需负责人审批的发布检查</strong><small>生成待审批任务</small></button>
        <div class="toolbar">
          <button data-action="approve"><strong>批准</strong><small>通过待审批项</small></button>
          <button data-action="deny"><strong>拒绝</strong><small>拒绝待审批项</small></button>
        </div>
        <button data-action="run-next-approval"><strong>Runtime 执行中请求审批</strong><small>模拟高风险操作暂停</small></button>
        <button data-action="complete-run"><strong>完成已批准的运行</strong><small>回传执行结果</small></button>
      </div>
      <div class="group">
        <div class="group-title">协作能力</div>
        <div class="toolbar">
          <button data-action="offline"><strong>Runtime 离线</strong><small>任务进入等待</small></button>
          <button data-action="online"><strong>Runtime 在线</strong><small>恢复接单能力</small></button>
        </div>
        <button data-action="handoff"><strong>创建 @Agent 转交</strong><small>把上下文转给报告 Agent</small></button>
        <button data-action="open-task"><strong>创建公开任务池任务</strong><small>不指定 Agent，等待认领</small></button>
        <button data-action="claim-open"><strong>认领公开任务</strong><small>报告 Agent 接下任务</small></button>
        <button data-action="permission-denied"><strong>验证越权访问失败</strong><small>工作区外用户会被拒绝</small></button>
      </div>
      <div class="event-label">最近一次动作</div>
      <div class="event-log" id="last-event">还没有操作</div>
    </aside>
    <div class="content">
      <section class="panel">
        <h2>这套程序在证明什么</h2>
        <div class="panel-body explainer">
          <p>它不是聊天机器人成品，而是企业 AI 协作的本地 MVP：证明公开 Agent 目录、任务派发、Runtime 在线/离线、负责人审批、@Agent 转交、公开任务池和审计留痕这些产品链路可以跑通。</p>
          <div class="metrics">
            <div class="metric"><strong>公开 Agent</strong><span>成员能直接发起协作</span></div>
            <div class="metric"><strong>本地 Runtime</strong><span>模拟 Codex 执行环境接单</span></div>
            <div class="metric"><strong>审批</strong><span>高风险操作先暂停确认</span></div>
            <div class="metric"><strong>审计</strong><span>关键动作都有可追踪记录</span></div>
          </div>
        </div>
      </section>
      <section class="panel"><h2>Agent 与 Runtime 状态</h2><div class="panel-body" id="agents"></div></section>
      <section class="panel"><h2>任务、运行与审批</h2><div class="panel-body" id="tasks"></div></section>
      <section class="panel"><h2>会话消息</h2><div class="panel-body" id="messages"></div></section>
      <section class="panel"><h2>审计记录</h2><div class="panel-body" id="audit"></div></section>
    </div>
  </main>
  <script>
    let snapshot = null;
    const post = (url, body = {}) => fetch(url, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body)
    }).then(r => r.json());
    const get = url => fetch(url).then(r => r.json());
    const cls = value => String(value || "").replace(/_/g, "").toLowerCase();
    const esc = value => String(value ?? "").replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" }[c]));
    const labels = {
      active: "可用",
      approved: "已批准",
      awaiting_acceptance: "等待接单",
      awaiting_approval: "等待审批",
      awaiting_runtime: "等待 Runtime",
      auto_accept_low_risk: "低风险自动接单",
      cancelled: "已取消",
      completed: "已完成",
      failed: "失败",
      fixture: "本地演示",
      handoff: "@Agent 转交",
      internal: "内部",
      offline: "离线",
      online: "在线",
      open: "公开任务池",
      pending: "待处理",
      public: "公开",
      queued: "排队中",
      rejected: "已拒绝",
      requires_owner_approval: "需负责人审批",
      running: "执行中",
      targeted: "定向任务",
      agent: "Agent",
      runtime: "Runtime",
      task: "任务",
      user: "成员",
      workspace: "工作区"
    };
    const eventLabels = {
      "agent.created": "创建 Agent",
      "agent.runtime_bound": "绑定 Runtime",
      "approval.decided": "审批完成",
      "approval.requested": "请求审批",
      "conversation.started": "发起会话",
      "handoff.created": "创建 @Agent 转交",
      "runtime.heartbeat": "Runtime 状态更新",
      "run.completed": "运行完成",
      "run.started": "运行开始",
      "task.created": "创建任务",
      "task.open_claimed": "认领公开任务",
      "task.open_created": "创建公开任务",
      "workspace.created": "创建工作区",
      "workspace.member_added": "添加成员"
    };
    const agentNames = {
      "Database Agent": "数据库诊断 Agent",
      "Release Agent": "发布检查 Agent",
      "Reporting Agent": "报告汇总 Agent"
    };
    const label = value => labels[String(value ?? "")] || value || "";
    const eventLabel = value => eventLabels[String(value ?? "")] || value || "";
    const agentLabel = value => agentNames[String(value ?? "")] || value || "";
    const idText = value => value ? `<span class="mono">${esc(value)}</span>` : "无";
    const pill = value => `<span class="pill ${cls(value)}">${esc(label(value))}</span>`;
    const lastEvent = value => {
      const text = String(value || "");
      if (text === "demo fixture seeded") return "演示数据已重置，当前是干净工作区。";
      if (text.includes("conversation") && text.includes("started")) return "已发起 Agent 会话，右侧会出现新任务和消息。";
      if (text.includes("completed run")) return "Runtime 已完成任务，右侧任务、消息和审计已更新。";
      if (text.includes("requested approval")) return "Runtime 已暂停并请求审批，请在左侧批准或拒绝。";
      if (text.includes("approval") && text.includes("decided")) return "审批已处理，右侧审批状态已更新。";
      if (text.includes("open task") && text.includes("created")) return "已创建公开任务池任务，等待 Agent 认领。";
      if (text.includes("open task") && text.includes("claimed")) return "公开任务已被 Agent 认领。";
      if (text.includes("handoff task")) return "已创建 @Agent 转交任务。";
      if (text.includes("permission denied")) return "越权访问已按预期被拒绝。";
      if (text.includes("status set to Offline")) return "Runtime 已离线，新任务会等待 Runtime 恢复。";
      if (text.includes("status set to Online")) return "Runtime 已在线，可以继续接单。";
      if (text.includes("run") && text.includes("completed")) return "运行已完成，结果已回传。";
      return text || "还没有操作";
    };
    const table = (headers, rows) => `<table><thead><tr>${headers.map(h => `<th>${h}</th>`).join("")}</tr></thead><tbody>${rows.join("") || `<tr><td colspan="${headers.length}" class="muted">暂无数据</td></tr>`}</tbody></table>`;
    async function refresh() {
      snapshot = await get("/api/snapshot");
      render(snapshot);
    }
    async function act(name) {
      const ids = snapshot.ids;
      let response;
      try {
        if (name === "reset") response = await post("/api/demo/reset");
        if (name === "offline") response = await post("/api/runtime/status", { status: "offline" });
        if (name === "online") response = await post("/api/runtime/status", { status: "online" });
        if (name === "start-db") response = await post("/api/conversations/start", { agent_id: ids.database_agent_id, prompt: "Inspect the orders incident." });
        if (name === "run-next") response = await post("/api/runtime/run-next");
        if (name === "start-release") response = await post("/api/conversations/start", { agent_id: ids.release_agent_id, prompt: "Prepare a protected release check." });
        if (name === "approve") response = await post("/api/approvals/decide", { approved: true });
        if (name === "deny") response = await post("/api/approvals/decide", { approved: false });
        if (name === "run-next-approval") response = await post("/api/runtime/run-next", { request_approval: true });
        if (name === "complete-run") response = await post("/api/runs/complete");
        if (name === "handoff") response = await post("/api/handoffs/create");
        if (name === "open-task") response = await post("/api/open-tasks/create");
        if (name === "claim-open") response = await post("/api/open-tasks/claim");
        if (name === "permission-denied") response = await post("/api/demo/permission-denied");
        snapshot = response.snapshot || await get("/api/snapshot");
        render(snapshot, response.ok === false ? response.error : null);
      } catch (error) {
        render(snapshot, String(error));
      }
    }
    function render(data, error) {
      document.getElementById("mode").textContent = `${label(data.provider_mode)}模式`;
      document.getElementById("last-event").textContent = error ? `操作失败：${error}` : lastEvent(data.last_event);
      const agentById = Object.fromEntries(data.state.agents.map(agent => [agent.id, agentLabel(agent.name)]));
      const agents = data.state.agents.map(agent => {
        const binding = data.state.bindings.find(b => b.agent_id === agent.id);
        const runtime = data.state.runtimes.find(r => r.id === binding?.runtime_id);
        return `<tr><td>${esc(agentLabel(agent.name))}</td><td>${esc(label(agent.visibility))}</td><td>${esc(label(agent.acceptance_policy))}</td><td>${pill(agent.status)}</td><td>${esc(runtime?.display_name || "未绑定")}</td><td>${pill(runtime?.status)}</td></tr>`;
      });
      document.getElementById("agents").innerHTML = table(["Agent", "可见性", "接单策略", "Agent 状态", "Runtime", "Runtime 状态"], agents);
      const tasks = data.state.tasks.map(task => {
        const run = data.state.runs.find(r => r.task_id === task.id);
        const approval = data.state.approvals.find(a => a.task_id === task.id);
        return `<tr><td>${idText(task.id)}</td><td>${esc(label(task.task_type))}</td><td>${pill(task.status)}</td><td>${esc(agentById[task.assigned_agent_id] || "未分配")}</td><td>${run ? `${idText(run.id)} ${pill(run.status)}` : "无"}</td><td>${approval ? `${idText(approval.id)} ${pill(approval.status)}` : "无"}</td><td>${esc(task.prompt)}</td></tr>`;
      });
      document.getElementById("tasks").innerHTML = table(["任务", "类型", "状态", "负责 Agent", "运行", "审批", "用户请求"], tasks);
      const messages = data.state.messages.map(message => `<tr><td>${idText(message.conversation_id)}</td><td>${esc(label(message.sender_type))}</td><td>${idText(message.sender_id)}</td><td>${esc(message.content)}</td></tr>`);
      document.getElementById("messages").innerHTML = table(["会话", "发送方类型", "发送方", "内容"], messages);
      const audit = data.state.audit_events.slice(-20).reverse().map(event => `<tr><td>${idText(event.id)}</td><td>${esc(eventLabel(event.action))}</td><td>${esc(label(event.resource_type))}</td><td>${idText(event.resource_id)}</td><td>${esc(JSON.stringify(event.redacted_payload))}</td></tr>`);
      document.getElementById("audit").innerHTML = table(["记录", "动作", "对象", "对象 ID", "脱敏载荷"], audit);
    }
    document.querySelectorAll("button[data-action]").forEach(button => button.addEventListener("click", () => act(button.dataset.action)));
    refresh();
  </script>
</body>
</html>
"#;
