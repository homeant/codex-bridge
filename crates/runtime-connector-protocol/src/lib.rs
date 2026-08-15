use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: &str = "2026-08-15";

pub type WorkspaceId = String;
pub type RuntimeId = String;
pub type AgentId = String;
pub type TaskId = String;
pub type RunId = String;
pub type ApprovalRequestId = String;
pub type ProjectKey = String;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EventEnvelope<T> {
    pub event_id: String,
    pub event_type: String,
    pub protocol_version: String,
    pub workspace_id: WorkspaceId,
    pub runtime_id: RuntimeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    pub trace_id: String,
    pub occurred_at: String,
    pub payload: T,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Ack {
    pub event_id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProtocolError>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RuntimeHello {
    pub connector_version: String,
    pub provider: ProviderKind,
    pub provider_version: String,
    #[serde(default)]
    pub projects: Vec<RuntimeProjectSummary>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RuntimeProjectSummary {
    pub project_key: ProjectKey,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    CodexAppServer,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlPlaneCommand {
    TaskOffered(TaskOffer),
    RunStart(RunStart),
    ApprovalDecided(ApprovalDecision),
    RunCancel(RunCancel),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeEvent {
    Hello(RuntimeHello),
    Heartbeat(RuntimeHeartbeat),
    TaskAccepted(TaskAccepted),
    RunStarted(RunStarted),
    RunProgress(RunProgress),
    RunInputRequested(RunInputRequested),
    RunApprovalRequested(RunApprovalRequested),
    RunCompleted(RunCompleted),
    RunFailed(RunFailed),
    HandoffRequested(HandoffRequest),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RuntimeHeartbeat {
    pub status: RuntimeReportedStatus,
    #[serde(default)]
    pub active_run_count: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeReportedStatus {
    Online,
    Busy,
    Degraded,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskOffer {
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub requester_user_id: String,
    pub task_kind: TaskKind,
    pub prompt: String,
    pub required_capabilities: Vec<String>,
    pub sensitivity: Sensitivity,
    pub project_key: ProjectKey,
    pub shared_context_manifest: SharedContextManifest,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Targeted,
    Open,
    Handoff,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Normal,
    Internal,
    Sensitive,
    Restricted,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SharedContextManifest {
    pub summary: String,
    #[serde(default)]
    pub items: Vec<SharedContextItem>,
}

impl SharedContextManifest {
    pub fn empty() -> Self {
        Self {
            summary: String::new(),
            items: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SharedContextItem {
    pub label: String,
    pub kind: SharedContextKind,
    pub sensitivity: Sensitivity,
    pub redacted: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SharedContextKind {
    UserRequest,
    ConversationSummary,
    FileReference,
    RunResult,
    AuditReference,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskAccepted {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunStart {
    pub task_id: TaskId,
    pub run_id: RunId,
    pub agent_id: AgentId,
    pub provider: ProviderKind,
    pub project_key: ProjectKey,
    pub prompt: String,
    pub developer_instructions: String,
    pub shared_context_manifest: SharedContextManifest,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunStarted {
    pub run_id: RunId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_turn_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunProgress {
    pub run_id: RunId,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunInputRequested {
    pub run_id: RunId,
    pub prompt: String,
    pub secret: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunApprovalRequested {
    pub run_id: RunId,
    pub action_type: String,
    pub target: String,
    pub scope: String,
    pub impact: String,
    pub recovery_plan: String,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub requested_payload: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ApprovalDecision {
    pub approval_request_id: ApprovalRequestId,
    pub run_id: RunId,
    pub approved: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunCompleted {
    pub run_id: RunId,
    pub result_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_ref: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunFailed {
    pub run_id: RunId,
    pub error_code: String,
    pub error_message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunCancel {
    pub task_id: TaskId,
    pub run_id: RunId,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct HandoffRequest {
    pub parent_task_id: TaskId,
    pub source_agent_id: AgentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_id: Option<AgentId>,
    pub reason: String,
    pub required_capabilities: Vec<String>,
    pub shared_context_manifest: SharedContextManifest,
}
