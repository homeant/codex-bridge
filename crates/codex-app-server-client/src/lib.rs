use std::{
    collections::HashMap, ffi::OsString, path::Path, process::Stdio, sync::Arc, time::Duration,
};

use serde_json::{Value, json};
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{Mutex, broadcast, mpsc, oneshot},
    time::timeout,
};
use tracing::{debug, error, warn};

#[derive(Debug, Error)]
pub enum AppServerError {
    #[error("failed to start Codex App Server: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("Codex App Server stdio is unavailable")]
    MissingStdio,
    #[error("Codex App Server request failed: {0}")]
    Request(String),
    #[error("Codex App Server request timed out")]
    Timeout,
    #[error("Codex App Server protocol error: {0}")]
    Protocol(String),
    #[error("Codex App Server connection closed")]
    Closed,
}

#[derive(Debug, Clone)]
pub enum TurnEvent {
    Progress(String),
    Completed { text: String, status: String },
    Failed(String),
}

pub struct TurnHandle {
    pub turn_id: String,
    pub events: mpsc::UnboundedReceiver<TurnEvent>,
}

pub struct ServerRequest {
    pub method: String,
    pub params: Value,
    response: oneshot::Sender<ServerRequestResponse>,
}

enum ServerRequestResponse {
    Result(Value),
    Error { code: i64, message: String },
}

impl ServerRequest {
    pub fn respond(self, result: Value) -> Result<(), AppServerError> {
        self.response
            .send(ServerRequestResponse::Result(result))
            .map_err(|_| AppServerError::Closed)
    }

    pub fn reject(self, code: i64, message: impl Into<String>) -> Result<(), AppServerError> {
        self.response
            .send(ServerRequestResponse::Error {
                code,
                message: message.into(),
            })
            .map_err(|_| AppServerError::Closed)
    }
}

type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>;

#[derive(Clone)]
pub struct CodexAppServer {
    writer: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    notifications: broadcast::Sender<Value>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
    server_requests: Arc<Mutex<Option<mpsc::UnboundedReceiver<ServerRequest>>>>,
    model: Option<String>,
    model_provider: Option<String>,
    _child: Arc<Mutex<Child>>,
}

impl CodexAppServer {
    pub async fn spawn(binary: &str) -> Result<Self, AppServerError> {
        Self::spawn_with_model(binary, None, None).await
    }

    pub async fn spawn_with_model(
        binary: &str,
        model: Option<String>,
        model_provider: Option<String>,
    ) -> Result<Self, AppServerError> {
        Self::spawn_with_model_and_environment(binary, model, model_provider, &[]).await
    }

    pub async fn spawn_with_model_and_environment(
        binary: &str,
        model: Option<String>,
        model_provider: Option<String>,
        environment: &[(OsString, OsString)],
    ) -> Result<Self, AppServerError> {
        let mut command = app_server_command(binary, environment);
        let mut child = command.spawn().map_err(AppServerError::Spawn)?;

        let stdin = child.stdin.take().ok_or(AppServerError::MissingStdio)?;
        let stdout = child.stdout.take().ok_or(AppServerError::MissingStdio)?;
        let stderr = child.stderr.take().ok_or(AppServerError::MissingStdio)?;
        let writer = Arc::new(Mutex::new(stdin));
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let next_id = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let (notifications, _) = broadcast::channel(1024);
        let (server_requests_tx, server_requests_rx) = mpsc::unbounded_channel();

        tokio::spawn(read_stdout(
            BufReader::new(stdout),
            writer.clone(),
            pending.clone(),
            notifications.clone(),
            next_id.clone(),
            server_requests_tx,
        ));
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                debug!(target: "codex_app_server", "{line}");
            }
        });

        let client = Self {
            writer,
            pending,
            notifications,
            next_id,
            server_requests: Arc::new(Mutex::new(Some(server_requests_rx))),
            model,
            model_provider,
            _child: Arc::new(Mutex::new(child)),
        };
        client.initialize().await?;
        Ok(client)
    }

    async fn initialize(&self) -> Result<(), AppServerError> {
        self.request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "im-codex-bridge",
                    "title": "IM Codex Bridge",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": { "experimentalApi": true }
            }),
        )
        .await?;
        write_json(&self.writer, &json!({ "method": "initialized" })).await
    }

    pub async fn start_routing_thread(
        &self,
        cwd: &Path,
        workspace_roots: &[String],
        developer_instructions: &str,
    ) -> Result<String, AppServerError> {
        let result = self
            .request(
                "thread/start",
                thread_params(
                    cwd,
                    workspace_roots,
                    developer_instructions,
                    "read-only",
                    "on-request",
                    self.model.as_deref(),
                    self.model_provider.as_deref(),
                ),
            )
            .await?;
        extract_thread_id(&result, "thread/start")
    }

    pub async fn start_project_thread(
        &self,
        cwd: &Path,
        workspace_roots: &[String],
        developer_instructions: &str,
    ) -> Result<String, AppServerError> {
        let result = self
            .request(
                "thread/start",
                thread_params(
                    cwd,
                    workspace_roots,
                    developer_instructions,
                    "danger-full-access",
                    "never",
                    self.model.as_deref(),
                    self.model_provider.as_deref(),
                ),
            )
            .await?;
        extract_thread_id(&result, "thread/start")
    }

    pub async fn resume_thread(
        &self,
        thread_id: &str,
        developer_instructions: &str,
    ) -> Result<String, AppServerError> {
        let mut params = json!({
            "threadId": thread_id,
            "sandbox": "danger-full-access",
            "approvalPolicy": "never",
            "approvalsReviewer": "user",
            "developerInstructions": developer_instructions
        });
        apply_model_selection(
            &mut params,
            self.model.as_deref(),
            self.model_provider.as_deref(),
        );
        let result = self.request("thread/resume", params).await?;
        extract_thread_id(&result, "thread/resume")
    }

    pub async fn start_turn(
        &self,
        thread_id: &str,
        text: &str,
        application_context: &str,
        image_paths: &[String],
    ) -> Result<TurnHandle, AppServerError> {
        let notifications = self.notifications.subscribe();
        let result = self
            .request(
                "turn/start",
                turn_params(
                    thread_id,
                    text,
                    application_context,
                    image_paths,
                    self.model.as_deref(),
                ),
            )
            .await?;
        let turn_id = result
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppServerError::Protocol("turn/start response has no turn.id".into()))?
            .to_owned();

        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(collect_turn(
            notifications,
            tx,
            thread_id.to_owned(),
            turn_id.clone(),
        ));
        Ok(TurnHandle {
            turn_id,
            events: rx,
        })
    }

    pub async fn interrupt(&self, thread_id: &str, turn_id: &str) -> Result<(), AppServerError> {
        self.request(
            "turn/interrupt",
            json!({ "threadId": thread_id, "turnId": turn_id }),
        )
        .await?;
        Ok(())
    }

    pub async fn steer(
        &self,
        thread_id: &str,
        turn_id: &str,
        text: &str,
        application_context: &str,
        image_paths: &[String],
    ) -> Result<(), AppServerError> {
        let params = json!({
            "threadId": thread_id,
            "expectedTurnId": turn_id,
            "additionalContext": {
                "im_bridge": {
                    "kind": "application",
                    "value": application_context
                }
            },
            "input": turn_input(text, image_paths)
        });
        for attempt in 0..10 {
            match self.request("turn/steer", params.clone()).await {
                Ok(_) => return Ok(()),
                Err(AppServerError::Request(message))
                    if message.contains("no active turn to steer") && attempt < 9 =>
                {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("the steer retry loop always returns")
    }

    pub async fn unsubscribe(&self, thread_id: &str) -> Result<(), AppServerError> {
        self.request("thread/unsubscribe", json!({ "threadId": thread_id }))
            .await?;
        Ok(())
    }

    pub async fn take_server_requests(
        &self,
    ) -> Result<mpsc::UnboundedReceiver<ServerRequest>, AppServerError> {
        self.server_requests
            .lock()
            .await
            .take()
            .ok_or_else(|| AppServerError::Protocol("server request stream already taken".into()))
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, AppServerError> {
        request_with_parts(&self.writer, &self.pending, &self.next_id, method, params).await
    }
}

fn app_server_command(binary: &str, environment: &[(OsString, OsString)]) -> Command {
    let mut command = Command::new(binary);
    command
        .args(["app-server", "--listen", "stdio://"])
        .envs(environment.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

fn turn_input(text: &str, image_paths: &[String]) -> Vec<Value> {
    let mut input = vec![json!({
        "type": "text",
        "text": text,
        "text_elements": []
    })];
    input.extend(
        image_paths
            .iter()
            .map(|path| json!({ "type": "localImage", "path": path })),
    );
    input
}

async fn request_with_parts(
    writer: &Arc<Mutex<ChildStdin>>,
    pending: &Pending,
    next_id: &Arc<std::sync::atomic::AtomicU64>,
    method: &str,
    params: Value,
) -> Result<Value, AppServerError> {
    let id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let key = id.to_string();
    let (tx, rx) = oneshot::channel();
    pending.lock().await.insert(key.clone(), tx);
    if let Err(error) = write_json(
        writer,
        &json!({ "id": id, "method": method, "params": params }),
    )
    .await
    {
        pending.lock().await.remove(&key);
        return Err(error);
    }
    match timeout(Duration::from_secs(60), rx).await {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(message))) => Err(AppServerError::Request(message)),
        Ok(Err(_)) => Err(AppServerError::Closed),
        Err(_) => {
            pending.lock().await.remove(&key);
            Err(AppServerError::Timeout)
        }
    }
}

fn thread_params(
    cwd: &Path,
    workspace_roots: &[String],
    developer_instructions: &str,
    sandbox: &str,
    approval_policy: &str,
    model: Option<&str>,
    model_provider: Option<&str>,
) -> Value {
    let mut params = json!({
        "cwd": cwd,
        "runtimeWorkspaceRoots": workspace_roots,
        "sandbox": sandbox,
        "approvalPolicy": approval_policy,
        "approvalsReviewer": "user",
        "developerInstructions": developer_instructions,
        "dynamicTools": codex_task_tools(),
        "serviceName": "wecom_codex_bridge",
        "threadSource": "user",
        "ephemeral": false
    });
    apply_model_selection(&mut params, model, model_provider);
    params
}

fn apply_model_selection(params: &mut Value, model: Option<&str>, model_provider: Option<&str>) {
    let object = params
        .as_object_mut()
        .expect("thread parameters are always a JSON object");
    if let Some(model) = model {
        object.insert("model".into(), Value::String(model.into()));
    }
    if let Some(model_provider) = model_provider {
        object.insert("modelProvider".into(), Value::String(model_provider.into()));
    }
}

fn turn_params(
    thread_id: &str,
    text: &str,
    application_context: &str,
    image_paths: &[String],
    model: Option<&str>,
) -> Value {
    let mut params = json!({
        "threadId": thread_id,
        "additionalContext": {
            "im_bridge": {
                "kind": "application",
                "value": application_context
            }
        },
        "input": turn_input(text, image_paths)
    });
    if let Some(model) = model {
        params
            .as_object_mut()
            .expect("turn parameters are always a JSON object")
            .insert("model".into(), Value::String(model.into()));
    }
    params
}

fn codex_task_tools() -> Value {
    json!([{
        "type": "namespace",
        "name": "codex_app",
        "description": "Tools for inspecting local Codex tasks, maintaining the trusted project catalog, and switching the current IM conversation to a configured project.",
        "tools": [
            {
                "type": "function",
                "name": "request_approval",
                "description": "Request the single final human authorization for one precisely bounded production mutation or comparably destructive action. Codex decides whether the action meets the trusted approval policy. Call only after all safe analysis and preparation are complete and immediately before execution. Do not call for ordinary project work or read-only operations.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "actionType": {
                            "type": "string",
                            "enum": [
                                "production_mutation",
                                "production_delete",
                                "production_deploy",
                                "production_restart",
                                "force_push",
                                "history_rewrite",
                                "discard_work",
                                "broad_delete",
                                "other_destructive"
                            ],
                            "description": "Risk category selected by Codex from the actual planned action."
                        },
                        "summary": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Concise description of the exact action awaiting approval."
                        },
                        "target": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Exact production resource, data set, repository ref, path, or other target."
                        },
                        "scope": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Bounded scope, including filters, counts, environments, branches, or paths."
                        },
                        "impact": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Expected user-visible or operational impact."
                        },
                        "recoveryPlan": {
                            "type": "string",
                            "minLength": 1,
                            "description": "How to roll back or recover if the action is wrong."
                        },
                        "steps": {
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "string",
                                "minLength": 1
                            },
                            "description": "All executable high-risk steps covered by this one approval."
                        }
                    },
                    "required": [
                        "actionType",
                        "summary",
                        "target",
                        "scope",
                        "impact",
                        "recoveryPlan",
                        "steps"
                    ]
                },
                "deferLoading": false
            },
            {
                "type": "function",
                "name": "list_threads",
                "description": "List recent local Codex tasks by title, directory, status, and update time. Usually omit query and inspect recent titles first. Use query only to search older task titles. Message previews are intentionally omitted, and the current IM bridge thread is excluded.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Optional substring to search for in Codex task titles."
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 100,
                            "description": "Maximum number of task summaries to return."
                        }
                    }
                },
                "deferLoading": true
            },
            {
                "type": "function",
                "name": "read_thread",
                "description": "Read the status and recent user/agent messages of one local Codex task without opening or changing it.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "threadId": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Codex task id returned by list_threads."
                        },
                        "hostId": {
                            "type": "string",
                            "enum": ["local"],
                            "description": "Optional local host id returned by list_threads."
                        },
                        "turnLimit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 50,
                            "description": "Maximum number of recent turns to return."
                        },
                        "includeOutputs": {
                            "type": "boolean",
                            "description": "Include truncated command output when verification evidence is needed."
                        },
                        "maxOutputCharsPerItem": {
                            "type": "integer",
                            "minimum": 100,
                            "maximum": 10000,
                            "description": "Maximum command-output characters per item."
                        }
                    },
                    "required": ["threadId"]
                },
                "deferLoading": true
            },
            {
                "type": "function",
                "name": "list_projects",
                "description": "List the bridge's current trusted project catalog. Paths are canonical and restricted to codex.allowed_roots.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {}
                },
                "deferLoading": true
            },
            {
                "type": "function",
                "name": "add_project",
                "description": "Add a project to the trusted bridge catalog and persist it to bridge.toml. The current IM requester must be an approval user. The directory must already exist inside codex.allowed_roots.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "projectId": {
                            "type": "string",
                            "minLength": 1,
                            "description": "New stable project ID."
                        },
                        "name": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Project display name."
                        },
                        "description": {
                            "type": "string",
                            "description": "Business and engineering description used for project routing."
                        },
                        "path": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Existing project directory below codex.allowed_roots. $HOME and ~ prefixes are supported."
                        }
                    },
                    "required": ["projectId", "name", "description", "path"]
                },
                "deferLoading": true
            },
            {
                "type": "function",
                "name": "update_project",
                "description": "Update a configured project's name, description, or path and persist the catalog to bridge.toml. The stable project ID cannot be changed. The current IM requester must be an approval user.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "projectId": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Existing stable project ID."
                        },
                        "name": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Replacement display name."
                        },
                        "description": {
                            "type": "string",
                            "description": "Replacement routing description."
                        },
                        "path": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Replacement existing directory below codex.allowed_roots."
                        }
                    },
                    "required": ["projectId"],
                    "anyOf": [
                        {"required": ["name"]},
                        {"required": ["description"]},
                        {"required": ["path"]}
                    ]
                },
                "deferLoading": true
            },
            {
                "type": "function",
                "name": "delete_project",
                "description": "Delete a project from the trusted bridge catalog and persist the change to bridge.toml. The final remaining project cannot be deleted. The current IM requester must be an approval user.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "projectId": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Existing stable project ID to delete."
                        }
                    },
                    "required": ["projectId"]
                },
                "deferLoading": true
            },
            {
                "type": "function",
                "name": "switch_to_project",
                "description": "Create a new Codex task in one trusted configured project and move the current IM conversation to it. Call this only when the current task is not already in the correct project. After success, stop processing because the bridge resubmits the original user request in the new task.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "projectId": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Exact trusted project ID from the developer-provided project catalog."
                        }
                    },
                    "required": ["projectId"]
                },
                "deferLoading": false
            }
        ]
    }])
}

fn extract_thread_id(result: &Value, method: &str) -> Result<String, AppServerError> {
    result
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppServerError::Protocol(format!("{method} response has no thread.id")))
}

async fn write_json(writer: &Arc<Mutex<ChildStdin>>, value: &Value) -> Result<(), AppServerError> {
    let mut encoded =
        serde_json::to_vec(value).map_err(|error| AppServerError::Protocol(error.to_string()))?;
    encoded.push(b'\n');
    let mut writer = writer.lock().await;
    writer
        .write_all(&encoded)
        .await
        .map_err(|error| AppServerError::Request(error.to_string()))?;
    writer
        .flush()
        .await
        .map_err(|error| AppServerError::Request(error.to_string()))
}

async fn read_stdout(
    mut stdout: BufReader<tokio::process::ChildStdout>,
    writer: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    notifications: broadcast::Sender<Value>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
    server_requests: mpsc::UnboundedSender<ServerRequest>,
) {
    let mut line = String::new();
    loop {
        line.clear();
        match stdout.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                error!("failed reading Codex App Server stdout: {error}");
                break;
            }
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                warn!("ignored invalid App Server JSON: {error}");
                continue;
            }
        };

        if value.get("id").is_some() && value.get("method").is_none() {
            let key = value["id"].to_string().trim_matches('"').to_owned();
            if let Some(sender) = pending.lock().await.remove(&key) {
                let response = if let Some(error) = value.get("error") {
                    Err(error.to_string())
                } else {
                    Ok(value.get("result").cloned().unwrap_or(Value::Null))
                };
                let _ = sender.send(response);
            }
            continue;
        }

        if value.get("id").is_some() && value.get("method").is_some() {
            if value.get("method").and_then(Value::as_str) == Some("item/tool/call") {
                tokio::spawn(handle_dynamic_tool_request(
                    writer.clone(),
                    pending.clone(),
                    next_id.clone(),
                    server_requests.clone(),
                    value,
                ));
            } else {
                tokio::spawn(forward_server_request(
                    writer.clone(),
                    server_requests.clone(),
                    value,
                ));
            }
            continue;
        }
        let _ = notifications.send(value);
    }

    for (_, sender) in pending.lock().await.drain() {
        let _ = sender.send(Err("Codex App Server connection closed".into()));
    }
}

async fn forward_server_request(
    writer: Arc<Mutex<ChildStdin>>,
    server_requests: mpsc::UnboundedSender<ServerRequest>,
    request: Value,
) {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let (response_tx, response_rx) = oneshot::channel();
    let response = if server_requests
        .send(ServerRequest {
            method: method.clone(),
            params,
            response: response_tx,
        })
        .is_err()
    {
        ServerRequestResponse::Error {
            code: -32000,
            message: "IM interaction handler is unavailable".into(),
        }
    } else {
        match response_rx.await {
            Ok(response) => response,
            Err(_) => ServerRequestResponse::Error {
                code: -32000,
                message: format!("IM interaction handler dropped request: {method}"),
            },
        }
    };
    let response = match response {
        ServerRequestResponse::Result(result) => json!({ "id": id, "result": result }),
        ServerRequestResponse::Error { code, message } => {
            json!({ "id": id, "error": { "code": code, "message": message } })
        }
    };
    if let Err(error) = write_json(&writer, &response).await {
        error!("failed to answer App Server request: {error}");
    }
}

async fn handle_dynamic_tool_request(
    writer: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    next_id: Arc<std::sync::atomic::AtomicU64>,
    server_requests: mpsc::UnboundedSender<ServerRequest>,
    request: Value,
) {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let result = if params.get("namespace").and_then(Value::as_str) == Some("codex_app")
        && params
            .get("tool")
            .and_then(Value::as_str)
            .is_some_and(|tool| {
                matches!(
                    tool,
                    "request_approval"
                        | "list_projects"
                        | "add_project"
                        | "update_project"
                        | "delete_project"
                        | "switch_to_project"
                )
            }) {
        dispatch_bridge_dynamic_tool(&server_requests, params).await
    } else {
        dispatch_dynamic_tool(&writer, &pending, &next_id, &params).await
    };
    let (success, text) = match result {
        Ok(value) => (
            true,
            serde_json::to_string(&value).unwrap_or_else(|error| error.to_string()),
        ),
        Err(error) => (false, error),
    };
    let response = json!({
        "id": id,
        "result": {
            "success": success,
            "contentItems": [{ "type": "inputText", "text": text }]
        }
    });
    if let Err(error) = write_json(&writer, &response).await {
        error!("failed to answer dynamic App Server tool call: {error}");
    }
}

async fn dispatch_bridge_dynamic_tool(
    server_requests: &mpsc::UnboundedSender<ServerRequest>,
    params: Value,
) -> Result<Value, String> {
    let (response_tx, response_rx) = oneshot::channel();
    server_requests
        .send(ServerRequest {
            method: "item/tool/call".into(),
            params,
            response: response_tx,
        })
        .map_err(|_| "IM bridge tool handler is unavailable".to_owned())?;
    match response_rx.await {
        Ok(ServerRequestResponse::Result(result)) => Ok(result),
        Ok(ServerRequestResponse::Error { message, .. }) => Err(message),
        Err(_) => Err("IM bridge tool handler dropped the request".into()),
    }
}

async fn dispatch_dynamic_tool(
    writer: &Arc<Mutex<ChildStdin>>,
    pending: &Pending,
    next_id: &Arc<std::sync::atomic::AtomicU64>,
    params: &Value,
) -> Result<Value, String> {
    let namespace = params.get("namespace").and_then(Value::as_str);
    if namespace != Some("codex_app") {
        return Err(format!(
            "unsupported dynamic tool namespace: {}",
            namespace.unwrap_or("<none>")
        ));
    }
    let tool = params
        .get("tool")
        .and_then(Value::as_str)
        .ok_or_else(|| "dynamic tool call has no tool name".to_owned())?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match tool {
        "list_threads" => {
            let limit = arguments
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(50)
                .clamp(1, 100);
            let mut list_params = json!({
                "limit": limit,
                "sortKey": "updated_at",
                "sortDirection": "desc"
            });
            if let Some(query) = arguments
                .get("query")
                .and_then(Value::as_str)
                .filter(|query| !query.trim().is_empty())
            {
                list_params["searchTerm"] = Value::String(query.to_owned());
            }
            let result = request_with_parts(writer, pending, next_id, "thread/list", list_params)
                .await
                .map_err(|error| error.to_string())?;
            Ok(compact_thread_list(
                &result,
                params.get("threadId").and_then(Value::as_str),
            ))
        }
        "read_thread" => {
            if arguments
                .get("hostId")
                .and_then(Value::as_str)
                .is_some_and(|host| host != "local")
            {
                return Err("only the local Codex host is available".into());
            }
            let thread_id = arguments
                .get("threadId")
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| "read_thread requires threadId".to_owned())?;
            let result = request_with_parts(
                writer,
                pending,
                next_id,
                "thread/read",
                json!({ "threadId": thread_id, "includeTurns": true }),
            )
            .await
            .map_err(|error| error.to_string())?;
            Ok(compact_thread_read(&result, &arguments))
        }
        _ => Err(format!("unsupported codex_app tool: {tool}")),
    }
}

fn compact_thread_list(result: &Value, current_thread_id: Option<&str>) -> Value {
    let threads = result
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|thread| thread.get("id").and_then(Value::as_str) != current_thread_id)
        .map(|thread| {
            json!({
                "threadId": thread.get("id"),
                "hostId": "local",
                "title": thread.get("name").filter(|value| !value.is_null()).or_else(|| thread.get("title")),
                "status": thread.get("status"),
                "cwd": thread.get("cwd"),
                "createdAt": thread.get("createdAt"),
                "updatedAt": thread.get("updatedAt"),
                "archived": thread.get("archived"),
                "gitInfo": thread.get("gitInfo")
            })
        })
        .collect::<Vec<_>>();
    json!({
        "threads": threads,
        "nextCursor": result.get("nextCursor"),
        "note": "Results contain local Codex tasks and exclude the current IM bridge task."
    })
}

fn compact_thread_read(result: &Value, arguments: &Value) -> Value {
    let Some(thread) = result.get("thread") else {
        return json!({ "error": "thread/read response has no thread" });
    };
    let turn_limit = arguments
        .get("turnLimit")
        .and_then(Value::as_u64)
        .unwrap_or(12)
        .clamp(1, 50) as usize;
    let include_outputs = arguments
        .get("includeOutputs")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let output_limit = arguments
        .get("maxOutputCharsPerItem")
        .and_then(Value::as_u64)
        .unwrap_or(2_000)
        .clamp(100, 10_000) as usize;
    let all_turns = thread
        .get("turns")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let turns = all_turns
        .iter()
        .skip(all_turns.len().saturating_sub(turn_limit))
        .map(|turn| compact_turn(turn, include_outputs, output_limit))
        .collect::<Vec<_>>();
    json!({
        "threadId": thread.get("id"),
        "hostId": "local",
        "title": thread.get("name").filter(|value| !value.is_null()).or_else(|| thread.get("title")),
        "status": thread.get("status"),
        "cwd": thread.get("cwd"),
        "createdAt": thread.get("createdAt"),
        "updatedAt": thread.get("updatedAt"),
        "archived": thread.get("archived"),
        "gitInfo": thread.get("gitInfo"),
        "turns": turns,
        "olderTurnsOmitted": all_turns.len().saturating_sub(turns.len())
    })
}

fn compact_turn(turn: &Value, include_outputs: bool, output_limit: usize) -> Value {
    let items = turn
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| compact_thread_item(item, include_outputs, output_limit))
        .collect::<Vec<_>>();
    json!({
        "turnId": turn.get("id"),
        "status": turn.get("status"),
        "error": turn.get("error"),
        "startedAt": turn.get("startedAt"),
        "completedAt": turn.get("completedAt"),
        "items": items
    })
}

fn compact_thread_item(item: &Value, include_outputs: bool, output_limit: usize) -> Option<Value> {
    match item.get("type").and_then(Value::as_str)? {
        "userMessage" => Some(json!({
            "type": "userMessage",
            "text": message_content_text(item).map(|text| clip_chars(&text, 4_000))
        })),
        "agentMessage" => Some(json!({
            "type": "agentMessage",
            "phase": item.get("phase"),
            "text": item.get("text").and_then(Value::as_str).map(|text| clip_chars(text, 4_000))
        })),
        "commandExecution" if include_outputs => Some(json!({
            "type": "commandExecution",
            "command": item.get("command"),
            "status": item.get("status"),
            "output": item.get("aggregatedOutput").and_then(Value::as_str).map(|text| clip_chars(text, output_limit))
        })),
        _ => None,
    }
}

fn message_content_text(item: &Value) -> Option<String> {
    let texts = item
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|content| content.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>();
    (!texts.is_empty()).then(|| texts.join("\n"))
}

fn clip_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut clipped = text.chars().take(limit).collect::<String>();
    clipped.push('…');
    clipped
}

async fn collect_turn(
    mut notifications: broadcast::Receiver<Value>,
    sender: mpsc::UnboundedSender<TurnEvent>,
    thread_id: String,
    turn_id: String,
) {
    let mut message_buffers = HashMap::<String, String>::new();
    let mut latest_message = String::new();
    let mut final_message = None::<String>;
    let mut stream_error = None::<String>;
    loop {
        let value = match notifications.recv().await {
            Ok(value) => value,
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                warn!("turn stream lagged by {skipped} notifications");
                continue;
            }
            Err(broadcast::error::RecvError::Closed) => {
                let _ = sender.send(TurnEvent::Failed("App Server stream closed".into()));
                return;
            }
        };
        let method = value.get("method").and_then(Value::as_str).unwrap_or("");
        let params = value.get("params").unwrap_or(&Value::Null);
        let same_thread = params.get("threadId").and_then(Value::as_str) == Some(&thread_id);
        let same_turn = params
            .get("turnId")
            .and_then(Value::as_str)
            .map(|id| id == turn_id)
            .or_else(|| {
                params
                    .pointer("/turn/id")
                    .and_then(Value::as_str)
                    .map(|id| id == turn_id)
            })
            .unwrap_or(false);
        if !same_thread || !same_turn {
            continue;
        }

        match method {
            "item/agentMessage/delta" => {
                if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                    let item_id = params
                        .get("itemId")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    let message = message_buffers.entry(item_id.to_owned()).or_default();
                    message.push_str(delta);
                    latest_message.clone_from(message);
                    let _ = sender.send(TurnEvent::Progress(message.clone()));
                }
            }
            "item/completed" => {
                if params.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage")
                    && let Some(text) = params.pointer("/item/text").and_then(Value::as_str)
                {
                    latest_message = text.to_owned();
                    if params.pointer("/item/phase").and_then(Value::as_str) == Some("final_answer")
                    {
                        final_message = Some(text.to_owned());
                    }
                }
            }
            "error" => {
                stream_error = params
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
            }
            "turn/completed" => {
                let status = params
                    .pointer("/turn/status")
                    .and_then(Value::as_str)
                    .unwrap_or("failed")
                    .to_owned();
                if status == "failed" {
                    let message = params
                        .pointer("/turn/error/message")
                        .and_then(Value::as_str)
                        .or(stream_error.as_deref())
                        .unwrap_or("Codex turn failed")
                        .to_owned();
                    let _ = sender.send(TurnEvent::Failed(message));
                } else {
                    let text = final_message
                        .or_else(|| final_agent_message(params))
                        .unwrap_or(latest_message);
                    let _ = sender.send(TurnEvent::Completed { text, status });
                }
                return;
            }
            _ => {}
        }
    }
}

fn final_agent_message(params: &Value) -> Option<String> {
    let items = params.pointer("/turn/items")?.as_array()?;
    items
        .iter()
        .rev()
        .find(|item| {
            item.get("type").and_then(Value::as_str) == Some("agentMessage")
                && item.get("phase").and_then(Value::as_str) == Some("final_answer")
        })
        .or_else(|| {
            items
                .iter()
                .rev()
                .find(|item| item.get("type").and_then(Value::as_str) == Some("agentMessage"))
        })
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_server_command_receives_the_resolved_environment() {
        let environment = [(
            OsString::from("MODEL_PROVIDER_TOKEN"),
            OsString::from("secret-value"),
        )];

        let command = app_server_command("codex", &environment);
        let configured_value = command
            .as_std()
            .get_envs()
            .find_map(|(key, value)| {
                (key == "MODEL_PROVIDER_TOKEN").then(|| value.map(ToOwned::to_owned))
            })
            .flatten();

        assert_eq!(configured_value, Some(OsString::from("secret-value")));
    }

    #[test]
    fn thread_params_match_the_app_server_v2_wire_shape() {
        let params = thread_params(
            Path::new("/workspace"),
            &["/workspace".into(), "/other".into()],
            "trusted instructions",
            "workspace-write",
            "on-request",
            Some("gpt-5.5"),
            Some("im_proxy"),
        );

        assert_eq!(params["cwd"], "/workspace");
        assert_eq!(params["sandbox"], "workspace-write");
        assert_eq!(params["approvalPolicy"], "on-request");
        assert_eq!(params["runtimeWorkspaceRoots"][1], "/other");
        assert_eq!(params["serviceName"], "wecom_codex_bridge");
        assert_eq!(params["threadSource"], "user");
        assert_eq!(params["model"], "gpt-5.5");
        assert_eq!(params["modelProvider"], "im_proxy");
        assert_eq!(params["dynamicTools"][0]["name"], "codex_app");
        assert_eq!(
            params["dynamicTools"][0]["tools"][0]["name"],
            "request_approval"
        );
        assert_eq!(
            params["dynamicTools"][0]["tools"][1]["name"],
            "list_threads"
        );
        assert_eq!(params["dynamicTools"][0]["tools"][2]["name"], "read_thread");
        let tool_names = params["dynamicTools"][0]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            tool_names,
            [
                "request_approval",
                "list_threads",
                "read_thread",
                "list_projects",
                "add_project",
                "update_project",
                "delete_project",
                "switch_to_project"
            ]
        );
    }

    #[test]
    fn project_threads_use_full_access_while_retaining_explicit_approval_requests() {
        let params = thread_params(
            Path::new("/workspace/project"),
            &["/workspace/project".into()],
            "trusted instructions",
            "danger-full-access",
            "never",
            None,
            None,
        );

        assert_eq!(params["sandbox"], "danger-full-access");
        assert_eq!(params["approvalPolicy"], "never");
        assert_eq!(params["approvalsReviewer"], "user");
        assert_eq!(params["runtimeWorkspaceRoots"][0], "/workspace/project");
        assert!(params.get("model").is_none());
        assert!(params.get("modelProvider").is_none());
    }

    #[test]
    fn turn_input_appends_local_images_after_user_text() {
        let input = turn_input(
            "inspect screenshots",
            &["/tmp/a.png".into(), "/tmp/b.jpg".into()],
        );
        assert_eq!(input[0]["type"], "text");
        assert_eq!(input[0]["text"], "inspect screenshots");
        assert_eq!(
            input[1],
            json!({"type": "localImage", "path": "/tmp/a.png"})
        );
        assert_eq!(
            input[2],
            json!({"type": "localImage", "path": "/tmp/b.jpg"})
        );
    }

    #[test]
    fn turn_params_override_the_model_for_existing_threads() {
        let params = turn_params(
            "thread-1",
            "hello",
            "application context",
            &[],
            Some("gpt-5.5"),
        );

        assert_eq!(params["threadId"], "thread-1");
        assert_eq!(params["model"], "gpt-5.5");
        assert_eq!(
            params["additionalContext"]["im_bridge"]["value"],
            "application context"
        );
    }

    #[test]
    fn compacts_task_list_and_excludes_the_calling_thread() {
        let result = json!({
            "data": [
                {"id": "current", "title": "Current", "preview": "status lookup"},
                {
                    "id": "target",
                    "title": "Fix report cache",
                    "preview": "Delete stale report cache",
                    "status": {"type": "idle"},
                    "cwd": "/workspace/reportify",
                    "updatedAt": 123
                }
            ],
            "nextCursor": "next"
        });

        let compact = compact_thread_list(&result, Some("current"));
        assert_eq!(compact["threads"].as_array().unwrap().len(), 1);
        assert_eq!(compact["threads"][0]["threadId"], "target");
        assert_eq!(compact["threads"][0]["hostId"], "local");
        assert!(compact["threads"][0].get("preview").is_none());
        assert_eq!(compact["nextCursor"], "next");
    }

    #[test]
    fn compacts_recent_task_turns_and_optional_outputs() {
        let result = json!({
            "thread": {
                "id": "target",
                "title": "Fix report cache",
                "status": {"type": "idle"},
                "turns": [
                    {"id": "old", "items": []},
                    {
                        "id": "latest",
                        "status": "completed",
                        "items": [
                            {"type": "userMessage", "content": [{"type": "text", "text": "Did it finish?"}]},
                            {"type": "commandExecution", "command": "git status", "status": "completed", "aggregatedOutput": "clean"},
                            {"type": "agentMessage", "phase": "final_answer", "text": "Completed"}
                        ]
                    }
                ]
            }
        });
        let compact =
            compact_thread_read(&result, &json!({"turnLimit": 1, "includeOutputs": true}));

        assert_eq!(compact["turns"].as_array().unwrap().len(), 1);
        assert!(compact.get("preview").is_none());
        assert_eq!(compact["olderTurnsOmitted"], 1);
        assert_eq!(compact["turns"][0]["turnId"], "latest");
        assert_eq!(compact["turns"][0]["items"][0]["text"], "Did it finish?");
        assert_eq!(compact["turns"][0]["items"][1]["output"], "clean");
        assert_eq!(compact["turns"][0]["items"][2]["text"], "Completed");
    }

    #[test]
    fn finds_the_final_answer_before_commentary() {
        let params = json!({
            "turn": {
                "items": [
                    {"type": "agentMessage", "phase": "commentary", "text": "working"},
                    {"type": "agentMessage", "phase": "final_answer", "text": "done"}
                ]
            }
        });

        assert_eq!(final_agent_message(&params).as_deref(), Some("done"));
    }

    #[tokio::test]
    async fn collects_final_answer_from_turn_completion() {
        let (notifications, _) = broadcast::channel(8);
        let receiver = notifications.subscribe();
        let (sender, mut events) = mpsc::unbounded_channel();
        tokio::spawn(collect_turn(
            receiver,
            sender,
            "thread-1".into(),
            "turn-1".into(),
        ));

        notifications
            .send(json!({
                "method": "item/agentMessage/delta",
                "params": {
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "itemId": "message-1",
                    "delta": "partial"
                }
            }))
            .unwrap();
        notifications
            .send(json!({
                "method": "turn/completed",
                "params": {
                    "threadId": "thread-1",
                    "turn": {
                        "id": "turn-1",
                        "status": "completed",
                        "items": [
                            {"type": "agentMessage", "phase": "final_answer", "text": "final"}
                        ]
                    }
                }
            }))
            .unwrap();

        assert!(matches!(
            events.recv().await,
            Some(TurnEvent::Progress(text)) if text == "partial"
        ));
        assert!(matches!(
            events.recv().await,
            Some(TurnEvent::Completed { text, status })
                if text == "final" && status == "completed"
        ));
    }

    #[tokio::test]
    async fn server_request_delivers_a_structured_result() {
        let (response, received) = oneshot::channel();
        let request = ServerRequest {
            method: "item/fileChange/requestApproval".into(),
            params: json!({"threadId": "thread-1"}),
            response,
        };
        request
            .respond(json!({"decision": "decline"}))
            .expect("deliver response");
        match received.await.expect("receive response") {
            ServerRequestResponse::Result(result) => {
                assert_eq!(result["decision"], "decline");
            }
            ServerRequestResponse::Error { message, .. } => {
                panic!("unexpected error response: {message}")
            }
        }
    }

    #[tokio::test]
    async fn project_switch_tool_is_forwarded_to_the_bridge() {
        let (requests, mut received) = mpsc::unbounded_channel();
        let forwarding = tokio::spawn(async move {
            dispatch_bridge_dynamic_tool(
                &requests,
                json!({
                    "threadId": "router-thread",
                    "namespace": "codex_app",
                    "tool": "switch_to_project",
                    "arguments": {"projectId": "bridge"}
                }),
            )
            .await
        });

        let request = received.recv().await.expect("receive forwarded tool call");
        assert_eq!(request.method, "item/tool/call");
        assert_eq!(request.params["threadId"], "router-thread");
        assert_eq!(request.params["arguments"]["projectId"], "bridge");
        request
            .respond(json!({"switched": true, "threadId": "project-thread"}))
            .expect("respond to forwarded tool call");

        let result = forwarding.await.unwrap().unwrap();
        assert_eq!(result["threadId"], "project-thread");
    }

    #[tokio::test]
    async fn project_catalog_tool_is_forwarded_to_the_bridge() {
        let (requests, mut received) = mpsc::unbounded_channel();
        let forwarding = tokio::spawn(async move {
            dispatch_bridge_dynamic_tool(
                &requests,
                json!({
                    "threadId": "router-thread",
                    "namespace": "codex_app",
                    "tool": "add_project",
                    "arguments": {
                        "projectId": "console",
                        "name": "Console",
                        "description": "admin console",
                        "path": "/workspace/console"
                    }
                }),
            )
            .await
        });

        let request = received.recv().await.expect("receive forwarded tool call");
        assert_eq!(request.params["tool"], "add_project");
        assert_eq!(request.params["arguments"]["projectId"], "console");
        request
            .respond(json!({"added": true, "project": {"id": "console"}}))
            .expect("respond to forwarded tool call");

        let result = forwarding.await.unwrap().unwrap();
        assert_eq!(result["project"]["id"], "console");
    }
}
