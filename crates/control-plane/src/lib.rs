use std::collections::{HashMap, HashSet};

use runtime_connector_protocol::{
    ProviderKind, RunApprovalRequested, RunCompleted, RunStart, Sensitivity, SharedContextManifest,
    TaskKind, TaskOffer,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

pub const CORE_SCHEMA: &str = include_str!("../migrations/0001_core.sql");

pub type UserId = String;
pub type WorkspaceId = String;
pub type AgentId = String;
pub type RuntimeId = String;
pub type ConversationId = String;
pub type MessageId = String;
pub type TaskId = String;
pub type RunId = String;
pub type ApprovalRequestId = String;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ControlPlaneError {
    #[error("{resource} not found: {id}")]
    NotFound { resource: &'static str, id: String },
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("validation error: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub display_name: String,
    pub status: UserStatus,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    PendingVerification,
    Active,
    Disabled,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub description: Option<String>,
    pub created_by: UserId,
    pub status: WorkspaceStatus,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Active,
    Disabled,
    Archived,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceMember {
    pub workspace_id: WorkspaceId,
    pub user_id: UserId,
    pub role: WorkspaceRole,
    pub status: MemberStatus,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceRole {
    Owner,
    Admin,
    Member,
}

impl WorkspaceRole {
    fn can_administer(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemberStatus {
    Active,
    Disabled,
    Left,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Agent {
    pub id: AgentId,
    pub workspace_id: WorkspaceId,
    pub owner_user_id: UserId,
    pub name: String,
    pub description: String,
    pub visibility: AgentVisibility,
    pub status: AgentStatus,
    pub acceptance_policy: AcceptancePolicy,
    pub capabilities: Vec<AgentCapability>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentVisibility {
    Private,
    Public,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Active,
    Paused,
    Suspended,
    Archived,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcceptancePolicy {
    AutoAcceptLowRisk,
    RequiresOwnerApproval,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AgentCapability {
    pub capability_key: String,
    pub display_name: String,
    pub sensitivity: Sensitivity,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Runtime {
    pub id: RuntimeId,
    pub workspace_id: WorkspaceId,
    pub owner_user_id: UserId,
    pub provider: ProviderKind,
    pub display_name: String,
    pub status: RuntimeStatus,
    pub projects: HashMap<String, RuntimeProject>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStatus {
    Offline,
    Connecting,
    Online,
    Busy,
    Degraded,
    Revoked,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RuntimeProject {
    pub project_key: String,
    pub display_name: String,
    pub path_digest: Option<String>,
    pub status: RuntimeProjectStatus,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProjectStatus {
    Active,
    Disabled,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AgentRuntimeBinding {
    pub workspace_id: WorkspaceId,
    pub agent_id: AgentId,
    pub runtime_id: RuntimeId,
    pub primary_project_key: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Conversation {
    pub id: ConversationId,
    pub workspace_id: WorkspaceId,
    pub creator_user_id: UserId,
    pub channel: ConversationChannel,
    pub title: Option<String>,
    pub status: ConversationStatus,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationChannel {
    Platform,
    Wecom,
    Lark,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationStatus {
    Active,
    Archived,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ConversationParticipant {
    pub workspace_id: WorkspaceId,
    pub conversation_id: ConversationId,
    pub participant_type: ParticipantType,
    pub participant_id: String,
    pub role: ParticipantRole,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantType {
    User,
    Agent,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRole {
    Creator,
    TargetAgent,
    MentionedAgent,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ConversationMessage {
    pub id: MessageId,
    pub workspace_id: WorkspaceId,
    pub conversation_id: ConversationId,
    pub sender_type: SenderType,
    pub sender_id: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SenderType {
    User,
    Agent,
    System,
    Runtime,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Task {
    pub id: TaskId,
    pub workspace_id: WorkspaceId,
    pub conversation_id: Option<ConversationId>,
    pub creator_user_id: Option<UserId>,
    pub target_agent_id: Option<AgentId>,
    pub assigned_agent_id: Option<AgentId>,
    pub assigned_runtime_id: Option<RuntimeId>,
    pub parent_task_id: Option<TaskId>,
    pub task_type: TaskType,
    pub prompt: String,
    pub required_capabilities: Vec<String>,
    pub sensitivity: Sensitivity,
    pub shared_context_manifest: SharedContextManifest,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Targeted,
    Open,
    Handoff,
}

impl From<TaskType> for TaskKind {
    fn from(value: TaskType) -> Self {
        match value {
            TaskType::Targeted => Self::Targeted,
            TaskType::Open => Self::Open,
            TaskType::Handoff => Self::Handoff,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Created,
    Queued,
    AwaitingRuntime,
    AwaitingAcceptance,
    Assigned,
    Running,
    AwaitingInput,
    AwaitingApproval,
    Completed,
    Failed,
    Rejected,
    Cancelled,
    Expired,
}

impl TaskStatus {
    fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Rejected | Self::Cancelled | Self::Expired
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Run {
    pub id: RunId,
    pub workspace_id: WorkspaceId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub runtime_id: RuntimeId,
    pub provider: ProviderKind,
    pub status: RunStatus,
    pub result_ref: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Starting,
    Running,
    AwaitingInput,
    AwaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub id: ApprovalRequestId,
    pub workspace_id: WorkspaceId,
    pub task_id: TaskId,
    pub run_id: Option<RunId>,
    pub approver_user_id: UserId,
    pub action_type: String,
    pub target: String,
    pub scope: String,
    pub impact: String,
    pub recovery_plan: String,
    pub status: ApprovalStatus,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AuditEvent {
    pub id: String,
    pub workspace_id: WorkspaceId,
    pub actor_type: ActorType,
    pub actor_id: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub redacted_payload: Value,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    User,
    Agent,
    Runtime,
    System,
    Channel,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ConversationStart {
    pub conversation_id: ConversationId,
    pub message_id: MessageId,
    pub task_id: TaskId,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct HandoffStart {
    pub task_id: TaskId,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ApprovalOutcome {
    pub approval_request_id: ApprovalRequestId,
    pub task_id: TaskId,
    pub run_id: Option<RunId>,
    pub approved: bool,
    pub reason: Option<String>,
}

impl ApprovalOutcome {
    pub fn runtime_decision(
        &self,
    ) -> Result<runtime_connector_protocol::ApprovalDecision, ControlPlaneError> {
        let Some(run_id) = self.run_id.clone() else {
            return Err(ControlPlaneError::InvalidState(
                "task-level approval has no runtime decision".into(),
            ));
        };
        Ok(runtime_connector_protocol::ApprovalDecision {
            approval_request_id: self.approval_request_id.clone(),
            run_id,
            approved: self.approved,
            reason: self.reason.clone(),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ControlPlaneSnapshot {
    pub users: Vec<User>,
    pub workspaces: Vec<Workspace>,
    pub members: Vec<WorkspaceMember>,
    pub agents: Vec<Agent>,
    pub runtimes: Vec<Runtime>,
    pub bindings: Vec<AgentRuntimeBinding>,
    pub conversations: Vec<Conversation>,
    pub participants: Vec<ConversationParticipant>,
    pub messages: Vec<ConversationMessage>,
    pub tasks: Vec<Task>,
    pub runs: Vec<Run>,
    pub approvals: Vec<ApprovalRequest>,
    pub audit_events: Vec<AuditEvent>,
}

#[derive(Debug, Default)]
pub struct MvpControlPlane {
    next_id: u64,
    users: HashMap<UserId, User>,
    workspaces: HashMap<WorkspaceId, Workspace>,
    members: HashMap<(WorkspaceId, UserId), WorkspaceMember>,
    agents: HashMap<AgentId, Agent>,
    runtimes: HashMap<RuntimeId, Runtime>,
    bindings: HashMap<AgentId, AgentRuntimeBinding>,
    conversations: HashMap<ConversationId, Conversation>,
    participants: Vec<ConversationParticipant>,
    messages: Vec<ConversationMessage>,
    tasks: HashMap<TaskId, Task>,
    runs: HashMap<RunId, Run>,
    approvals: HashMap<ApprovalRequestId, ApprovalRequest>,
    audit_events: Vec<AuditEvent>,
    offered_tasks: HashSet<TaskId>,
}

impl MvpControlPlane {
    pub fn seed_user(&mut self, email: &str, display_name: &str) -> UserId {
        let id = self.next_id("usr");
        self.users.insert(
            id.clone(),
            User {
                id: id.clone(),
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                status: UserStatus::Active,
            },
        );
        id
    }

    pub fn create_workspace(
        &mut self,
        owner_user_id: &str,
        name: &str,
    ) -> Result<WorkspaceId, ControlPlaneError> {
        self.active_user(owner_user_id)?;
        let workspace_id = self.next_id("wsp");
        self.workspaces.insert(
            workspace_id.clone(),
            Workspace {
                id: workspace_id.clone(),
                name: name.to_owned(),
                description: None,
                created_by: owner_user_id.to_owned(),
                status: WorkspaceStatus::Active,
            },
        );
        self.members.insert(
            (workspace_id.clone(), owner_user_id.to_owned()),
            WorkspaceMember {
                workspace_id: workspace_id.clone(),
                user_id: owner_user_id.to_owned(),
                role: WorkspaceRole::Owner,
                status: MemberStatus::Active,
            },
        );
        self.audit(AuditDraft {
            workspace_id: &workspace_id,
            actor_type: ActorType::User,
            actor_id: Some(owner_user_id.to_owned()),
            action: "workspace.created",
            resource_type: "workspace",
            resource_id: Some(workspace_id.clone()),
            redacted_payload: json!({ "name": name }),
        });
        Ok(workspace_id)
    }

    pub fn add_member(
        &mut self,
        actor_user_id: &str,
        workspace_id: &str,
        user_id: &str,
        role: WorkspaceRole,
    ) -> Result<(), ControlPlaneError> {
        self.require_workspace_active(workspace_id)?;
        self.active_user(user_id)?;
        let actor = self.active_member(workspace_id, actor_user_id)?;
        if !actor.role.can_administer() {
            return Err(ControlPlaneError::Forbidden(
                "only owner/admin can add members".into(),
            ));
        }
        self.members.insert(
            (workspace_id.to_owned(), user_id.to_owned()),
            WorkspaceMember {
                workspace_id: workspace_id.to_owned(),
                user_id: user_id.to_owned(),
                role,
                status: MemberStatus::Active,
            },
        );
        self.audit(AuditDraft {
            workspace_id,
            actor_type: ActorType::User,
            actor_id: Some(actor_user_id.to_owned()),
            action: "workspace.member_added",
            resource_type: "user",
            resource_id: Some(user_id.to_owned()),
            redacted_payload: json!({ "role": role }),
        });
        Ok(())
    }

    pub fn register_runtime(
        &mut self,
        owner_user_id: &str,
        workspace_id: &str,
        display_name: &str,
        projects: Vec<RuntimeProject>,
    ) -> Result<RuntimeId, ControlPlaneError> {
        self.active_member(workspace_id, owner_user_id)?;
        let runtime_id = self.next_id("rtm");
        self.runtimes.insert(
            runtime_id.clone(),
            Runtime {
                id: runtime_id.clone(),
                workspace_id: workspace_id.to_owned(),
                owner_user_id: owner_user_id.to_owned(),
                provider: ProviderKind::CodexAppServer,
                display_name: display_name.to_owned(),
                status: RuntimeStatus::Offline,
                projects: projects
                    .into_iter()
                    .map(|project| (project.project_key.clone(), project))
                    .collect(),
            },
        );
        self.audit(AuditDraft {
            workspace_id,
            actor_type: ActorType::User,
            actor_id: Some(owner_user_id.to_owned()),
            action: "runtime.registered",
            resource_type: "runtime",
            resource_id: Some(runtime_id.clone()),
            redacted_payload: json!({ "provider": "codex_app_server" }),
        });
        Ok(runtime_id)
    }

    pub fn runtime_heartbeat(
        &mut self,
        runtime_id: &str,
        status: RuntimeStatus,
    ) -> Result<(), ControlPlaneError> {
        if matches!(status, RuntimeStatus::Revoked) {
            return Err(ControlPlaneError::Validation(
                "heartbeat cannot set revoked".into(),
            ));
        }
        let workspace_id = {
            let runtime = self.runtime_mut(runtime_id)?;
            if runtime.status == RuntimeStatus::Revoked {
                return Err(ControlPlaneError::Forbidden("runtime revoked".into()));
            }
            runtime.status = status;
            runtime.workspace_id.clone()
        };
        self.audit(AuditDraft {
            workspace_id: &workspace_id,
            actor_type: ActorType::Runtime,
            actor_id: Some(runtime_id.to_owned()),
            action: "runtime.heartbeat",
            resource_type: "runtime",
            resource_id: Some(runtime_id.to_owned()),
            redacted_payload: json!({ "status": status }),
        });
        if matches!(status, RuntimeStatus::Online | RuntimeStatus::Busy) {
            self.resume_tasks_waiting_for_runtime(runtime_id)?;
        }
        Ok(())
    }

    pub fn create_agent(
        &mut self,
        owner_user_id: &str,
        workspace_id: &str,
        definition: AgentDefinition,
    ) -> Result<AgentId, ControlPlaneError> {
        self.active_member(workspace_id, owner_user_id)?;
        if definition.capabilities.is_empty() {
            return Err(ControlPlaneError::Validation(
                "agent requires at least one capability".into(),
            ));
        }
        let agent_id = self.next_id("agt");
        self.agents.insert(
            agent_id.clone(),
            Agent {
                id: agent_id.clone(),
                workspace_id: workspace_id.to_owned(),
                owner_user_id: owner_user_id.to_owned(),
                name: definition.name,
                description: definition.description,
                visibility: definition.visibility,
                status: AgentStatus::Active,
                acceptance_policy: definition.acceptance_policy,
                capabilities: definition.capabilities,
            },
        );
        self.audit(AuditDraft {
            workspace_id,
            actor_type: ActorType::User,
            actor_id: Some(owner_user_id.to_owned()),
            action: "agent.created",
            resource_type: "agent",
            resource_id: Some(agent_id.clone()),
            redacted_payload: json!({ "visibility": definition.visibility }),
        });
        Ok(agent_id)
    }

    pub fn bind_agent_runtime(
        &mut self,
        actor_user_id: &str,
        agent_id: &str,
        runtime_id: &str,
        project_key: &str,
    ) -> Result<(), ControlPlaneError> {
        let agent = self.agent(agent_id)?.clone();
        let runtime = self.runtime(runtime_id)?.clone();
        if agent.workspace_id != runtime.workspace_id {
            return Err(ControlPlaneError::Forbidden(
                "agent and runtime are in different workspaces".into(),
            ));
        }
        let actor = self.active_member(&agent.workspace_id, actor_user_id)?;
        if actor.user_id != agent.owner_user_id && !actor.role.can_administer() {
            return Err(ControlPlaneError::Forbidden(
                "only the agent owner or workspace admin can bind runtime".into(),
            ));
        }
        if runtime.owner_user_id != agent.owner_user_id {
            return Err(ControlPlaneError::Forbidden(
                "first slice requires each agent to use its owner's runtime".into(),
            ));
        }
        let Some(project) = runtime.projects.get(project_key) else {
            return Err(ControlPlaneError::NotFound {
                resource: "runtime_project",
                id: project_key.to_owned(),
            });
        };
        if project.status != RuntimeProjectStatus::Active {
            return Err(ControlPlaneError::InvalidState(
                "runtime project is disabled".into(),
            ));
        }
        self.bindings.insert(
            agent_id.to_owned(),
            AgentRuntimeBinding {
                workspace_id: agent.workspace_id.clone(),
                agent_id: agent_id.to_owned(),
                runtime_id: runtime_id.to_owned(),
                primary_project_key: project_key.to_owned(),
                enabled: true,
            },
        );
        self.audit(AuditDraft {
            workspace_id: &agent.workspace_id,
            actor_type: ActorType::User,
            actor_id: Some(actor_user_id.to_owned()),
            action: "agent.runtime_bound",
            resource_type: "agent",
            resource_id: Some(agent_id.to_owned()),
            redacted_payload: json!({ "runtime_id": runtime_id, "project_key": project_key }),
        });
        Ok(())
    }

    pub fn start_agent_conversation(
        &mut self,
        requester_user_id: &str,
        workspace_id: &str,
        target_agent_id: &str,
        prompt: &str,
    ) -> Result<ConversationStart, ControlPlaneError> {
        self.active_member(workspace_id, requester_user_id)?;
        let agent = self.callable_agent(workspace_id, requester_user_id, target_agent_id)?;
        let conversation_id = self.next_id("cnv");
        let message_id = self.next_id("msg");
        self.conversations.insert(
            conversation_id.clone(),
            Conversation {
                id: conversation_id.clone(),
                workspace_id: workspace_id.to_owned(),
                creator_user_id: requester_user_id.to_owned(),
                channel: ConversationChannel::Platform,
                title: Some(agent.name.clone()),
                status: ConversationStatus::Active,
            },
        );
        self.participants.push(ConversationParticipant {
            workspace_id: workspace_id.to_owned(),
            conversation_id: conversation_id.clone(),
            participant_type: ParticipantType::User,
            participant_id: requester_user_id.to_owned(),
            role: ParticipantRole::Creator,
        });
        self.participants.push(ConversationParticipant {
            workspace_id: workspace_id.to_owned(),
            conversation_id: conversation_id.clone(),
            participant_type: ParticipantType::Agent,
            participant_id: target_agent_id.to_owned(),
            role: ParticipantRole::TargetAgent,
        });
        self.messages.push(ConversationMessage {
            id: message_id.clone(),
            workspace_id: workspace_id.to_owned(),
            conversation_id: conversation_id.clone(),
            sender_type: SenderType::User,
            sender_id: Some(requester_user_id.to_owned()),
            content: prompt.to_owned(),
        });
        let status = self.initial_task_status(&agent)?;
        let task_id = self.create_task(TaskDraft {
            workspace_id: workspace_id.to_owned(),
            conversation_id: Some(conversation_id.clone()),
            creator_user_id: Some(requester_user_id.to_owned()),
            target_agent_id: Some(target_agent_id.to_owned()),
            assigned_agent_id: Some(target_agent_id.to_owned()),
            parent_task_id: None,
            task_type: TaskType::Targeted,
            prompt: prompt.to_owned(),
            required_capabilities: agent
                .capabilities
                .iter()
                .map(|capability| capability.capability_key.clone())
                .collect(),
            sensitivity: max_sensitivity(&agent.capabilities),
            shared_context_manifest: SharedContextManifest::empty(),
            status,
        });
        self.prepare_new_task(&task_id)?;
        self.audit(AuditDraft {
            workspace_id,
            actor_type: ActorType::User,
            actor_id: Some(requester_user_id.to_owned()),
            action: "task.created",
            resource_type: "task",
            resource_id: Some(task_id.clone()),
            redacted_payload: json!({ "task_type": "targeted", "agent_id": target_agent_id }),
        });
        Ok(ConversationStart {
            conversation_id,
            message_id,
            task_id,
            status,
        })
    }

    pub fn create_open_task(
        &mut self,
        requester_user_id: &str,
        workspace_id: &str,
        prompt: &str,
        required_capabilities: Vec<String>,
        sensitivity: Sensitivity,
    ) -> Result<TaskId, ControlPlaneError> {
        self.active_member(workspace_id, requester_user_id)?;
        let task_id = self.create_task(TaskDraft {
            workspace_id: workspace_id.to_owned(),
            conversation_id: None,
            creator_user_id: Some(requester_user_id.to_owned()),
            target_agent_id: None,
            assigned_agent_id: None,
            parent_task_id: None,
            task_type: TaskType::Open,
            prompt: prompt.to_owned(),
            required_capabilities,
            sensitivity,
            shared_context_manifest: SharedContextManifest::empty(),
            status: TaskStatus::Queued,
        });
        self.audit(AuditDraft {
            workspace_id,
            actor_type: ActorType::User,
            actor_id: Some(requester_user_id.to_owned()),
            action: "task.open_created",
            resource_type: "task",
            resource_id: Some(task_id.clone()),
            redacted_payload: json!({ "disclosure": "redacted_summary_only" }),
        });
        Ok(task_id)
    }

    pub fn claim_open_task(
        &mut self,
        actor_user_id: &str,
        task_id: &str,
        agent_id: &str,
    ) -> Result<TaskOffer, ControlPlaneError> {
        let task = self.task(task_id)?.clone();
        if task.task_type != TaskType::Open || task.status != TaskStatus::Queued {
            return Err(ControlPlaneError::InvalidState(
                "only queued open tasks can be claimed".into(),
            ));
        }
        let agent = self.callable_agent(&task.workspace_id, actor_user_id, agent_id)?;
        if agent.owner_user_id != actor_user_id {
            return Err(ControlPlaneError::Forbidden(
                "only the candidate agent owner can claim in the first slice".into(),
            ));
        }
        let agent_capabilities: HashSet<&str> = agent
            .capabilities
            .iter()
            .map(|capability| capability.capability_key.as_str())
            .collect();
        if !task
            .required_capabilities
            .iter()
            .all(|capability| agent_capabilities.contains(capability.as_str()))
        {
            return Err(ControlPlaneError::Forbidden(
                "agent does not satisfy required capabilities".into(),
            ));
        }
        let (runtime_id, project_key) = self.ready_binding(&agent)?;
        {
            let task = self.task_mut(task_id)?;
            task.assigned_agent_id = Some(agent_id.to_owned());
            task.target_agent_id = Some(agent_id.to_owned());
            task.assigned_runtime_id = Some(runtime_id.clone());
            task.status = TaskStatus::AwaitingAcceptance;
        }
        self.offered_tasks.insert(task_id.to_owned());
        self.audit(AuditDraft {
            workspace_id: &task.workspace_id,
            actor_type: ActorType::User,
            actor_id: Some(actor_user_id.to_owned()),
            action: "task.open_claimed",
            resource_type: "task",
            resource_id: Some(task_id.to_owned()),
            redacted_payload: json!({ "agent_id": agent_id }),
        });
        Ok(task_offer_from(self.task(task_id)?.clone(), &project_key))
    }

    pub fn offer_next_task(
        &mut self,
        runtime_id: &str,
    ) -> Result<Option<TaskOffer>, ControlPlaneError> {
        let runtime = self.runtime(runtime_id)?.clone();
        if !matches!(runtime.status, RuntimeStatus::Online | RuntimeStatus::Busy) {
            return Ok(None);
        }
        let Some(task_id) = self
            .tasks
            .values()
            .filter(|task| task.status == TaskStatus::Queued)
            .filter(|task| {
                task.assigned_agent_id
                    .as_ref()
                    .and_then(|agent_id| self.bindings.get(agent_id))
                    .is_some_and(|binding| binding.runtime_id == runtime_id && binding.enabled)
            })
            .min_by(|left, right| left.id.cmp(&right.id))
            .map(|task| task.id.clone())
        else {
            return Ok(None);
        };
        let (agent_id, project_key, workspace_id) = {
            let task = self.task(&task_id)?;
            let agent_id = task
                .assigned_agent_id
                .clone()
                .ok_or_else(|| ControlPlaneError::InvalidState("task has no agent".into()))?;
            let binding = self.binding(&agent_id)?;
            (
                agent_id,
                binding.primary_project_key.clone(),
                task.workspace_id.clone(),
            )
        };
        {
            let task = self.task_mut(&task_id)?;
            task.status = TaskStatus::AwaitingAcceptance;
            task.assigned_runtime_id = Some(runtime_id.to_owned());
        }
        self.offered_tasks.insert(task_id.clone());
        self.audit(AuditDraft {
            workspace_id: &workspace_id,
            actor_type: ActorType::System,
            actor_id: None,
            action: "task.offered",
            resource_type: "task",
            resource_id: Some(task_id.clone()),
            redacted_payload: json!({ "runtime_id": runtime_id, "agent_id": agent_id }),
        });
        let task = self.task(&task_id)?.clone();
        Ok(Some(task_offer_from(task, &project_key)))
    }

    pub fn runtime_accept_task(
        &mut self,
        runtime_id: &str,
        task_id: &str,
    ) -> Result<RunStart, ControlPlaneError> {
        let runtime = self.runtime(runtime_id)?.clone();
        if !matches!(runtime.status, RuntimeStatus::Online | RuntimeStatus::Busy) {
            return Err(ControlPlaneError::InvalidState(
                "runtime is not online".into(),
            ));
        }
        if !self.offered_tasks.contains(task_id) {
            return Err(ControlPlaneError::InvalidState(
                "task was not offered to this runtime".into(),
            ));
        }
        let task = self.task(task_id)?.clone();
        if task.status != TaskStatus::AwaitingAcceptance {
            return Err(ControlPlaneError::InvalidState(
                "task is not awaiting acceptance".into(),
            ));
        }
        if task.assigned_runtime_id.as_deref() != Some(runtime_id) {
            return Err(ControlPlaneError::Forbidden(
                "task assigned to a different runtime".into(),
            ));
        }
        self.offered_tasks.remove(task_id);
        let agent_id = task
            .assigned_agent_id
            .clone()
            .ok_or_else(|| ControlPlaneError::InvalidState("task has no assigned agent".into()))?;
        let project_key = self.binding(&agent_id)?.primary_project_key.clone();
        let run_id = self.next_id("run");
        self.runs.insert(
            run_id.clone(),
            Run {
                id: run_id.clone(),
                workspace_id: task.workspace_id.clone(),
                task_id: task_id.to_owned(),
                agent_id: agent_id.clone(),
                runtime_id: runtime_id.to_owned(),
                provider: runtime.provider,
                status: RunStatus::Running,
                result_ref: None,
                error_code: None,
                error_message: None,
            },
        );
        {
            let task = self.task_mut(task_id)?;
            task.status = TaskStatus::Running;
        }
        self.audit(AuditDraft {
            workspace_id: &task.workspace_id,
            actor_type: ActorType::Runtime,
            actor_id: Some(runtime_id.to_owned()),
            action: "run.started",
            resource_type: "run",
            resource_id: Some(run_id.clone()),
            redacted_payload: json!({ "task_id": task_id, "provider": "codex_app_server" }),
        });
        Ok(RunStart {
            task_id: task_id.to_owned(),
            run_id,
            agent_id,
            provider: ProviderKind::CodexAppServer,
            project_key,
            prompt: task.prompt,
            developer_instructions: "Run inside the runtime's trusted project catalog; fail closed for unknown project keys.".into(),
            shared_context_manifest: task.shared_context_manifest,
        })
    }

    pub fn runtime_request_approval(
        &mut self,
        runtime_id: &str,
        request: RunApprovalRequested,
    ) -> Result<ApprovalRequestId, ControlPlaneError> {
        let run = self.run(&request.run_id)?.clone();
        if run.runtime_id != runtime_id {
            return Err(ControlPlaneError::Forbidden(
                "approval request came from a different runtime".into(),
            ));
        }
        if run.status != RunStatus::Running {
            return Err(ControlPlaneError::InvalidState(
                "only running runs can request approval".into(),
            ));
        }
        let agent = self.agent(&run.agent_id)?.clone();
        let approval_id = self.next_id("apr");
        self.approvals.insert(
            approval_id.clone(),
            ApprovalRequest {
                id: approval_id.clone(),
                workspace_id: run.workspace_id.clone(),
                task_id: run.task_id.clone(),
                run_id: Some(run.id.clone()),
                approver_user_id: agent.owner_user_id,
                action_type: request.action_type,
                target: request.target,
                scope: request.scope,
                impact: request.impact,
                recovery_plan: request.recovery_plan,
                status: ApprovalStatus::Pending,
            },
        );
        {
            self.task_mut(&run.task_id)?.status = TaskStatus::AwaitingApproval;
            self.run_mut(&run.id)?.status = RunStatus::AwaitingApproval;
        }
        self.audit(AuditDraft {
            workspace_id: &run.workspace_id,
            actor_type: ActorType::Runtime,
            actor_id: Some(runtime_id.to_owned()),
            action: "approval.requested",
            resource_type: "approval_request",
            resource_id: Some(approval_id.clone()),
            redacted_payload: json!({ "task_id": run.task_id, "run_id": run.id }),
        });
        Ok(approval_id)
    }

    pub fn decide_approval(
        &mut self,
        approver_user_id: &str,
        approval_id: &str,
        approved: bool,
        reason: Option<String>,
    ) -> Result<ApprovalOutcome, ControlPlaneError> {
        let approval = self.approval(approval_id)?.clone();
        if approval.status != ApprovalStatus::Pending {
            return Err(ControlPlaneError::InvalidState(
                "approval has already been decided".into(),
            ));
        }
        if approval.approver_user_id != approver_user_id {
            let member = self.active_member(&approval.workspace_id, approver_user_id)?;
            if !member.role.can_administer() {
                return Err(ControlPlaneError::Forbidden(
                    "user is not the approval owner or workspace admin".into(),
                ));
            }
        }
        match approval.run_id.as_deref() {
            Some(run_id) if approved => {
                self.approval_mut(approval_id)?.status = ApprovalStatus::Approved;
                self.run_mut(run_id)?.status = RunStatus::Running;
                self.task_mut(&approval.task_id)?.status = TaskStatus::Running;
            }
            Some(run_id) => {
                self.approval_mut(approval_id)?.status = ApprovalStatus::Rejected;
                self.run_mut(run_id)?.status = RunStatus::Cancelled;
                self.task_mut(&approval.task_id)?.status = TaskStatus::Rejected;
            }
            None if approved => {
                self.approval_mut(approval_id)?.status = ApprovalStatus::Approved;
                let next_status = self.status_after_task_approval(&approval.task_id)?;
                self.task_mut(&approval.task_id)?.status = next_status;
            }
            None => {
                self.approval_mut(approval_id)?.status = ApprovalStatus::Rejected;
                self.task_mut(&approval.task_id)?.status = TaskStatus::Rejected;
            }
        }
        self.audit(AuditDraft {
            workspace_id: &approval.workspace_id,
            actor_type: ActorType::User,
            actor_id: Some(approver_user_id.to_owned()),
            action: "approval.decided",
            resource_type: "approval_request",
            resource_id: Some(approval_id.to_owned()),
            redacted_payload: json!({ "approved": approved }),
        });
        Ok(ApprovalOutcome {
            approval_request_id: approval_id.to_owned(),
            task_id: approval.task_id,
            run_id: approval.run_id,
            approved,
            reason,
        })
    }

    pub fn runtime_complete_run(
        &mut self,
        runtime_id: &str,
        completed: RunCompleted,
    ) -> Result<(), ControlPlaneError> {
        let run = self.run(&completed.run_id)?.clone();
        if run.runtime_id != runtime_id {
            return Err(ControlPlaneError::Forbidden(
                "completion came from a different runtime".into(),
            ));
        }
        if run.status != RunStatus::Running {
            return Err(ControlPlaneError::InvalidState(
                "only running runs can complete".into(),
            ));
        }
        {
            let run = self.run_mut(&completed.run_id)?;
            run.status = RunStatus::Completed;
            run.result_ref = completed.result_ref;
        }
        {
            let task = self.task_mut(&run.task_id)?;
            task.status = TaskStatus::Completed;
        }
        if let Some(conversation_id) = self.task(&run.task_id)?.conversation_id.clone() {
            let message_id = self.next_id("msg");
            self.messages.push(ConversationMessage {
                id: message_id,
                workspace_id: run.workspace_id.clone(),
                conversation_id,
                sender_type: SenderType::Agent,
                sender_id: Some(run.agent_id.clone()),
                content: completed.result_text,
            });
        }
        self.audit(AuditDraft {
            workspace_id: &run.workspace_id,
            actor_type: ActorType::Runtime,
            actor_id: Some(runtime_id.to_owned()),
            action: "run.completed",
            resource_type: "run",
            resource_id: Some(completed.run_id),
            redacted_payload: json!({ "task_id": run.task_id }),
        });
        Ok(())
    }

    pub fn request_handoff(
        &mut self,
        requester_user_id: &str,
        parent_task_id: &str,
        target_agent_id: &str,
        reason: &str,
        manifest: SharedContextManifest,
    ) -> Result<HandoffStart, ControlPlaneError> {
        let parent = self.task(parent_task_id)?.clone();
        if parent.status.is_terminal() {
            return Err(ControlPlaneError::InvalidState(
                "terminal parent task cannot create handoff".into(),
            ));
        }
        self.active_member(&parent.workspace_id, requester_user_id)?;
        let target_agent =
            self.callable_agent(&parent.workspace_id, requester_user_id, target_agent_id)?;
        let source_agent_id = parent.assigned_agent_id.clone().ok_or_else(|| {
            ControlPlaneError::InvalidState("parent task has no source agent".into())
        })?;
        if self.handoff_chain_contains(parent_task_id, target_agent_id)?
            || source_agent_id == target_agent_id
        {
            return Err(ControlPlaneError::InvalidState(
                "handoff loop detected".into(),
            ));
        }
        if self.handoff_depth(parent_task_id)? >= 2 {
            return Err(ControlPlaneError::InvalidState(
                "handoff depth limit exceeded".into(),
            ));
        }
        let status = self.initial_task_status(&target_agent)?;
        let task_id = self.create_task(TaskDraft {
            workspace_id: parent.workspace_id.clone(),
            conversation_id: parent.conversation_id.clone(),
            creator_user_id: Some(requester_user_id.to_owned()),
            target_agent_id: Some(target_agent_id.to_owned()),
            assigned_agent_id: Some(target_agent_id.to_owned()),
            parent_task_id: Some(parent_task_id.to_owned()),
            task_type: TaskType::Handoff,
            prompt: reason.to_owned(),
            required_capabilities: target_agent
                .capabilities
                .iter()
                .map(|capability| capability.capability_key.clone())
                .collect(),
            sensitivity: max_sensitivity(&target_agent.capabilities),
            shared_context_manifest: manifest,
            status,
        });
        self.prepare_new_task(&task_id)?;
        if let Some(conversation_id) = parent.conversation_id {
            self.participants.push(ConversationParticipant {
                workspace_id: parent.workspace_id.clone(),
                conversation_id,
                participant_type: ParticipantType::Agent,
                participant_id: target_agent_id.to_owned(),
                role: ParticipantRole::MentionedAgent,
            });
        }
        self.audit(AuditDraft {
            workspace_id: &parent.workspace_id,
            actor_type: ActorType::User,
            actor_id: Some(requester_user_id.to_owned()),
            action: "handoff.created",
            resource_type: "task",
            resource_id: Some(task_id.clone()),
            redacted_payload: json!({
                "parent_task_id": parent_task_id,
                "source_agent_id": source_agent_id,
                "target_agent_id": target_agent_id
            }),
        });
        Ok(HandoffStart { task_id, status })
    }

    pub fn task(&self, task_id: &str) -> Result<&Task, ControlPlaneError> {
        self.tasks
            .get(task_id)
            .ok_or_else(|| ControlPlaneError::NotFound {
                resource: "task",
                id: task_id.to_owned(),
            })
    }

    pub fn run(&self, run_id: &str) -> Result<&Run, ControlPlaneError> {
        self.runs
            .get(run_id)
            .ok_or_else(|| ControlPlaneError::NotFound {
                resource: "run",
                id: run_id.to_owned(),
            })
    }

    pub fn approval(&self, approval_id: &str) -> Result<&ApprovalRequest, ControlPlaneError> {
        self.approvals
            .get(approval_id)
            .ok_or_else(|| ControlPlaneError::NotFound {
                resource: "approval_request",
                id: approval_id.to_owned(),
            })
    }

    pub fn conversation_messages(&self, conversation_id: &str) -> Vec<&ConversationMessage> {
        self.messages
            .iter()
            .filter(|message| message.conversation_id == conversation_id)
            .collect()
    }

    pub fn conversation_participants(
        &self,
        conversation_id: &str,
    ) -> Vec<&ConversationParticipant> {
        self.participants
            .iter()
            .filter(|participant| participant.conversation_id == conversation_id)
            .collect()
    }

    pub fn audit_events(&self, workspace_id: &str) -> Vec<&AuditEvent> {
        self.audit_events
            .iter()
            .filter(|event| event.workspace_id == workspace_id)
            .collect()
    }

    pub fn snapshot(&self) -> ControlPlaneSnapshot {
        let mut users = self.users.values().cloned().collect::<Vec<_>>();
        users.sort_by(|left, right| left.id.cmp(&right.id));
        let mut workspaces = self.workspaces.values().cloned().collect::<Vec<_>>();
        workspaces.sort_by(|left, right| left.id.cmp(&right.id));
        let mut members = self.members.values().cloned().collect::<Vec<_>>();
        members.sort_by(|left, right| {
            left.workspace_id
                .cmp(&right.workspace_id)
                .then(left.user_id.cmp(&right.user_id))
        });
        let mut agents = self.agents.values().cloned().collect::<Vec<_>>();
        agents.sort_by(|left, right| left.id.cmp(&right.id));
        let mut runtimes = self.runtimes.values().cloned().collect::<Vec<_>>();
        runtimes.sort_by(|left, right| left.id.cmp(&right.id));
        let mut bindings = self.bindings.values().cloned().collect::<Vec<_>>();
        bindings.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
        let mut conversations = self.conversations.values().cloned().collect::<Vec<_>>();
        conversations.sort_by(|left, right| left.id.cmp(&right.id));
        let mut tasks = self.tasks.values().cloned().collect::<Vec<_>>();
        tasks.sort_by(|left, right| left.id.cmp(&right.id));
        let mut runs = self.runs.values().cloned().collect::<Vec<_>>();
        runs.sort_by(|left, right| left.id.cmp(&right.id));
        let mut approvals = self.approvals.values().cloned().collect::<Vec<_>>();
        approvals.sort_by(|left, right| left.id.cmp(&right.id));
        ControlPlaneSnapshot {
            users,
            workspaces,
            members,
            agents,
            runtimes,
            bindings,
            conversations,
            participants: self.participants.clone(),
            messages: self.messages.clone(),
            tasks,
            runs,
            approvals,
            audit_events: self.audit_events.clone(),
        }
    }

    fn create_task(&mut self, draft: TaskDraft) -> TaskId {
        let task_id = self.next_id("tsk");
        self.tasks.insert(
            task_id.clone(),
            Task {
                id: task_id.clone(),
                workspace_id: draft.workspace_id,
                conversation_id: draft.conversation_id,
                creator_user_id: draft.creator_user_id,
                target_agent_id: draft.target_agent_id,
                assigned_agent_id: draft.assigned_agent_id,
                assigned_runtime_id: None,
                parent_task_id: draft.parent_task_id,
                task_type: draft.task_type,
                prompt: draft.prompt,
                required_capabilities: draft.required_capabilities,
                sensitivity: draft.sensitivity,
                shared_context_manifest: draft.shared_context_manifest,
                status: draft.status,
            },
        );
        task_id
    }

    fn prepare_new_task(&mut self, task_id: &str) -> Result<(), ControlPlaneError> {
        if self.task(task_id)?.status == TaskStatus::AwaitingApproval {
            self.create_task_execution_approval(task_id)?;
        }
        Ok(())
    }

    fn resume_tasks_waiting_for_runtime(
        &mut self,
        runtime_id: &str,
    ) -> Result<(), ControlPlaneError> {
        let waiting = self
            .tasks
            .values()
            .filter(|task| task.status == TaskStatus::AwaitingRuntime)
            .filter_map(|task| {
                let agent_id = task.assigned_agent_id.as_deref()?;
                let binding = self.bindings.get(agent_id)?;
                (binding.runtime_id == runtime_id && binding.enabled).then(|| task.id.clone())
            })
            .collect::<Vec<_>>();
        for task_id in waiting {
            let next_status = self.status_after_runtime_available(&task_id)?;
            {
                let task = self.task_mut(&task_id)?;
                task.status = next_status;
            }
            if next_status == TaskStatus::AwaitingApproval {
                self.create_task_execution_approval(&task_id)?;
            }
            let workspace_id = self.task(&task_id)?.workspace_id.clone();
            self.audit(AuditDraft {
                workspace_id: &workspace_id,
                actor_type: ActorType::System,
                actor_id: None,
                action: "task.runtime_available",
                resource_type: "task",
                resource_id: Some(task_id),
                redacted_payload: json!({ "runtime_id": runtime_id, "status": next_status }),
            });
        }
        Ok(())
    }

    fn status_after_runtime_available(
        &self,
        task_id: &str,
    ) -> Result<TaskStatus, ControlPlaneError> {
        let task = self.task(task_id)?;
        let agent_id = task
            .assigned_agent_id
            .as_deref()
            .ok_or_else(|| ControlPlaneError::InvalidState("task has no assigned agent".into()))?;
        let agent = self.agent(agent_id)?;
        self.ready_binding(agent)?;
        Ok(match agent.acceptance_policy {
            AcceptancePolicy::AutoAcceptLowRisk => TaskStatus::Queued,
            AcceptancePolicy::RequiresOwnerApproval
                if self.has_task_execution_approval(task_id, ApprovalStatus::Approved) =>
            {
                TaskStatus::Queued
            }
            AcceptancePolicy::RequiresOwnerApproval => TaskStatus::AwaitingApproval,
        })
    }

    fn status_after_task_approval(&self, task_id: &str) -> Result<TaskStatus, ControlPlaneError> {
        let task = self.task(task_id)?;
        let agent_id = task
            .assigned_agent_id
            .as_deref()
            .ok_or_else(|| ControlPlaneError::InvalidState("task has no assigned agent".into()))?;
        let agent = self.agent(agent_id)?;
        Ok(if self.ready_binding(agent).is_ok() {
            TaskStatus::Queued
        } else {
            TaskStatus::AwaitingRuntime
        })
    }

    fn create_task_execution_approval(
        &mut self,
        task_id: &str,
    ) -> Result<ApprovalRequestId, ControlPlaneError> {
        if let Some(existing_id) = self
            .approvals
            .values()
            .find(|approval| {
                approval.task_id == task_id
                    && approval.run_id.is_none()
                    && approval.status == ApprovalStatus::Pending
            })
            .map(|approval| approval.id.clone())
        {
            return Ok(existing_id);
        }
        let task = self.task(task_id)?.clone();
        let agent_id = task
            .assigned_agent_id
            .as_deref()
            .ok_or_else(|| ControlPlaneError::InvalidState("task has no assigned agent".into()))?;
        let agent = self.agent(agent_id)?.clone();
        let approval_id = self.next_id("apr");
        self.approvals.insert(
            approval_id.clone(),
            ApprovalRequest {
                id: approval_id.clone(),
                workspace_id: task.workspace_id.clone(),
                task_id: task_id.to_owned(),
                run_id: None,
                approver_user_id: agent.owner_user_id,
                action_type: "agent_task_execution".into(),
                target: format!("agent:{} task:{}", agent.id, task_id),
                scope: "one queued agent task in the current workspace".into(),
                impact: "allows the responsible runtime to accept and execute this task".into(),
                recovery_plan: "reject the approval before the runtime accepts the task".into(),
                status: ApprovalStatus::Pending,
            },
        );
        self.audit(AuditDraft {
            workspace_id: &task.workspace_id,
            actor_type: ActorType::System,
            actor_id: None,
            action: "approval.requested",
            resource_type: "approval_request",
            resource_id: Some(approval_id.clone()),
            redacted_payload: json!({ "task_id": task_id, "run_id": null }),
        });
        Ok(approval_id)
    }

    fn has_task_execution_approval(&self, task_id: &str, status: ApprovalStatus) -> bool {
        self.approvals.values().any(|approval| {
            approval.task_id == task_id && approval.run_id.is_none() && approval.status == status
        })
    }

    fn initial_task_status(&self, agent: &Agent) -> Result<TaskStatus, ControlPlaneError> {
        let Ok((_, _)) = self.ready_binding(agent) else {
            return Ok(TaskStatus::AwaitingRuntime);
        };
        Ok(match agent.acceptance_policy {
            AcceptancePolicy::AutoAcceptLowRisk => TaskStatus::Queued,
            AcceptancePolicy::RequiresOwnerApproval => TaskStatus::AwaitingApproval,
        })
    }

    fn ready_binding(&self, agent: &Agent) -> Result<(RuntimeId, String), ControlPlaneError> {
        let binding = self.binding(&agent.id)?;
        let runtime = self.runtime(&binding.runtime_id)?;
        if !binding.enabled {
            return Err(ControlPlaneError::InvalidState(
                "agent runtime binding is disabled".into(),
            ));
        }
        if !matches!(runtime.status, RuntimeStatus::Online | RuntimeStatus::Busy) {
            return Err(ControlPlaneError::InvalidState("runtime is offline".into()));
        }
        if !runtime
            .projects
            .get(&binding.primary_project_key)
            .is_some_and(|project| project.status == RuntimeProjectStatus::Active)
        {
            return Err(ControlPlaneError::NotFound {
                resource: "runtime_project",
                id: binding.primary_project_key.clone(),
            });
        }
        Ok((
            binding.runtime_id.clone(),
            binding.primary_project_key.clone(),
        ))
    }

    fn callable_agent(
        &self,
        workspace_id: &str,
        requester_user_id: &str,
        agent_id: &str,
    ) -> Result<Agent, ControlPlaneError> {
        let member = self.active_member(workspace_id, requester_user_id)?;
        let agent = self.agent(agent_id)?;
        if agent.workspace_id != workspace_id {
            return Err(ControlPlaneError::NotFound {
                resource: "agent",
                id: agent_id.to_owned(),
            });
        }
        if agent.status != AgentStatus::Active {
            return Err(ControlPlaneError::InvalidState(
                "agent is not active".into(),
            ));
        }
        if agent.visibility == AgentVisibility::Private
            && agent.owner_user_id != requester_user_id
            && !member.role.can_administer()
        {
            return Err(ControlPlaneError::Forbidden(
                "private agent is not callable by this member".into(),
            ));
        }
        Ok(agent.clone())
    }

    fn active_user(&self, user_id: &str) -> Result<&User, ControlPlaneError> {
        let user = self
            .users
            .get(user_id)
            .ok_or_else(|| ControlPlaneError::NotFound {
                resource: "user",
                id: user_id.to_owned(),
            })?;
        if user.status != UserStatus::Active {
            return Err(ControlPlaneError::Forbidden("user is not active".into()));
        }
        Ok(user)
    }

    fn require_workspace_active(&self, workspace_id: &str) -> Result<(), ControlPlaneError> {
        let workspace =
            self.workspaces
                .get(workspace_id)
                .ok_or_else(|| ControlPlaneError::NotFound {
                    resource: "workspace",
                    id: workspace_id.to_owned(),
                })?;
        if workspace.status != WorkspaceStatus::Active {
            return Err(ControlPlaneError::Forbidden(
                "workspace is not active".into(),
            ));
        }
        Ok(())
    }

    fn active_member(
        &self,
        workspace_id: &str,
        user_id: &str,
    ) -> Result<&WorkspaceMember, ControlPlaneError> {
        self.require_workspace_active(workspace_id)?;
        self.active_user(user_id)?;
        let member = self
            .members
            .get(&(workspace_id.to_owned(), user_id.to_owned()))
            .ok_or_else(|| ControlPlaneError::Forbidden("user is not a workspace member".into()))?;
        if member.status != MemberStatus::Active {
            return Err(ControlPlaneError::Forbidden(
                "workspace membership is not active".into(),
            ));
        }
        Ok(member)
    }

    fn agent(&self, agent_id: &str) -> Result<&Agent, ControlPlaneError> {
        self.agents
            .get(agent_id)
            .ok_or_else(|| ControlPlaneError::NotFound {
                resource: "agent",
                id: agent_id.to_owned(),
            })
    }

    fn runtime(&self, runtime_id: &str) -> Result<&Runtime, ControlPlaneError> {
        self.runtimes
            .get(runtime_id)
            .ok_or_else(|| ControlPlaneError::NotFound {
                resource: "runtime",
                id: runtime_id.to_owned(),
            })
    }

    fn runtime_mut(&mut self, runtime_id: &str) -> Result<&mut Runtime, ControlPlaneError> {
        self.runtimes
            .get_mut(runtime_id)
            .ok_or_else(|| ControlPlaneError::NotFound {
                resource: "runtime",
                id: runtime_id.to_owned(),
            })
    }

    fn binding(&self, agent_id: &str) -> Result<&AgentRuntimeBinding, ControlPlaneError> {
        self.bindings
            .get(agent_id)
            .ok_or_else(|| ControlPlaneError::NotFound {
                resource: "agent_runtime_binding",
                id: agent_id.to_owned(),
            })
    }

    fn task_mut(&mut self, task_id: &str) -> Result<&mut Task, ControlPlaneError> {
        self.tasks
            .get_mut(task_id)
            .ok_or_else(|| ControlPlaneError::NotFound {
                resource: "task",
                id: task_id.to_owned(),
            })
    }

    fn run_mut(&mut self, run_id: &str) -> Result<&mut Run, ControlPlaneError> {
        self.runs
            .get_mut(run_id)
            .ok_or_else(|| ControlPlaneError::NotFound {
                resource: "run",
                id: run_id.to_owned(),
            })
    }

    fn approval_mut(
        &mut self,
        approval_id: &str,
    ) -> Result<&mut ApprovalRequest, ControlPlaneError> {
        self.approvals
            .get_mut(approval_id)
            .ok_or_else(|| ControlPlaneError::NotFound {
                resource: "approval_request",
                id: approval_id.to_owned(),
            })
    }

    fn handoff_chain_contains(
        &self,
        task_id: &str,
        agent_id: &str,
    ) -> Result<bool, ControlPlaneError> {
        let mut current = Some(task_id.to_owned());
        while let Some(current_id) = current {
            let task = self.task(&current_id)?;
            if task.assigned_agent_id.as_deref() == Some(agent_id) {
                return Ok(true);
            }
            current = task.parent_task_id.clone();
        }
        Ok(false)
    }

    fn handoff_depth(&self, task_id: &str) -> Result<usize, ControlPlaneError> {
        let mut depth = 0;
        let mut current = Some(task_id.to_owned());
        while let Some(current_id) = current {
            let task = self.task(&current_id)?;
            if task.task_type == TaskType::Handoff {
                depth += 1;
            }
            current = task.parent_task_id.clone();
        }
        Ok(depth)
    }

    fn audit(&mut self, draft: AuditDraft<'_>) {
        let id = self.next_id("aud");
        self.audit_events.push(AuditEvent {
            id,
            workspace_id: draft.workspace_id.to_owned(),
            actor_type: draft.actor_type,
            actor_id: draft.actor_id,
            action: draft.action.to_owned(),
            resource_type: draft.resource_type.to_owned(),
            resource_id: draft.resource_id,
            redacted_payload: draft.redacted_payload,
        });
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}_{}", self.next_id)
    }
}

#[derive(Debug, Clone)]
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    pub visibility: AgentVisibility,
    pub acceptance_policy: AcceptancePolicy,
    pub capabilities: Vec<AgentCapability>,
}

struct AuditDraft<'a> {
    workspace_id: &'a str,
    actor_type: ActorType,
    actor_id: Option<String>,
    action: &'a str,
    resource_type: &'a str,
    resource_id: Option<String>,
    redacted_payload: Value,
}

struct TaskDraft {
    workspace_id: WorkspaceId,
    conversation_id: Option<ConversationId>,
    creator_user_id: Option<UserId>,
    target_agent_id: Option<AgentId>,
    assigned_agent_id: Option<AgentId>,
    parent_task_id: Option<TaskId>,
    task_type: TaskType,
    prompt: String,
    required_capabilities: Vec<String>,
    sensitivity: Sensitivity,
    shared_context_manifest: SharedContextManifest,
    status: TaskStatus,
}

fn task_offer_from(task: Task, project_key: &str) -> TaskOffer {
    TaskOffer {
        task_id: task.id,
        agent_id: task.assigned_agent_id.unwrap_or_default(),
        requester_user_id: task.creator_user_id.unwrap_or_default(),
        task_kind: task.task_type.into(),
        prompt: task.prompt,
        required_capabilities: task.required_capabilities,
        sensitivity: task.sensitivity,
        project_key: project_key.to_owned(),
        shared_context_manifest: task.shared_context_manifest,
    }
}

fn max_sensitivity(capabilities: &[AgentCapability]) -> Sensitivity {
    capabilities
        .iter()
        .map(|capability| capability.sensitivity)
        .max_by_key(|sensitivity| match sensitivity {
            Sensitivity::Normal => 0,
            Sensitivity::Internal => 1,
            Sensitivity::Sensitive => 2,
            Sensitivity::Restricted => 3,
        })
        .unwrap_or(Sensitivity::Normal)
}

#[cfg(test)]
mod tests {
    use runtime_connector_protocol::{
        RunApprovalRequested, RunCompleted, SharedContextItem, SharedContextKind,
    };

    use super::*;

    #[test]
    fn direct_public_agent_flow_creates_task_run_result_and_audit_without_employee_chat() {
        let mut plane = seeded_plane();
        let Fixture {
            workspace_id,
            requester,
            owner: _,
            agent_id,
            runtime_id,
        } = fixture(
            &mut plane,
            AcceptancePolicy::AutoAcceptLowRisk,
            AgentVisibility::Public,
        );

        let start = plane
            .start_agent_conversation(
                &requester,
                &workspace_id,
                &agent_id,
                "inspect read replicas",
            )
            .unwrap();
        assert_eq!(start.status, TaskStatus::Queued);

        let offer = plane.offer_next_task(&runtime_id).unwrap().unwrap();
        assert_eq!(offer.task_id, start.task_id);
        assert_eq!(offer.task_kind, TaskKind::Targeted);
        assert_eq!(offer.project_key, "orders-api");

        let run_start = plane
            .runtime_accept_task(&runtime_id, &start.task_id)
            .unwrap();
        assert_eq!(run_start.provider, ProviderKind::CodexAppServer);
        assert_eq!(
            plane.task(&start.task_id).unwrap().status,
            TaskStatus::Running
        );

        plane
            .runtime_complete_run(
                &runtime_id,
                RunCompleted {
                    run_id: run_start.run_id.clone(),
                    result_text: "replicas are healthy".into(),
                    result_ref: Some("result://run".into()),
                },
            )
            .unwrap();

        assert_eq!(
            plane.task(&start.task_id).unwrap().status,
            TaskStatus::Completed
        );
        assert_eq!(
            plane.run(&run_start.run_id).unwrap().status,
            RunStatus::Completed
        );
        let messages = plane.conversation_messages(&start.conversation_id);
        assert!(
            messages
                .iter()
                .any(|message| message.content == "replicas are healthy")
        );
        let user_participants = plane
            .conversation_participants(&start.conversation_id)
            .into_iter()
            .filter(|participant| participant.participant_type == ParticipantType::User)
            .count();
        assert_eq!(user_participants, 1);
        assert!(
            plane
                .audit_events(&workspace_id)
                .iter()
                .any(|event| event.action == "run.completed")
        );
    }

    #[test]
    fn private_or_cross_workspace_agent_is_not_callable_by_regular_member() {
        let mut plane = seeded_plane();
        let Fixture {
            workspace_id,
            requester,
            agent_id,
            ..
        } = fixture(
            &mut plane,
            AcceptancePolicy::AutoAcceptLowRisk,
            AgentVisibility::Private,
        );

        let private_error = plane
            .start_agent_conversation(&requester, &workspace_id, &agent_id, "hello")
            .unwrap_err();
        assert!(matches!(private_error, ControlPlaneError::Forbidden(_)));

        let outsider = plane.seed_user("outsider@example.com", "Outsider");
        let other_workspace = plane.create_workspace(&outsider, "Other").unwrap();
        let cross_workspace_error = plane
            .start_agent_conversation(&requester, &other_workspace, &agent_id, "hello")
            .unwrap_err();
        assert!(matches!(
            cross_workspace_error,
            ControlPlaneError::Forbidden(_) | ControlPlaneError::NotFound { .. }
        ));
    }

    #[test]
    fn offline_runtime_waits_instead_of_succeeding_silently() {
        let mut plane = seeded_plane();
        let Fixture {
            workspace_id,
            requester,
            agent_id,
            runtime_id,
            ..
        } = fixture_without_online_runtime(
            &mut plane,
            AcceptancePolicy::AutoAcceptLowRisk,
            AgentVisibility::Public,
        );

        let start = plane
            .start_agent_conversation(&requester, &workspace_id, &agent_id, "run check")
            .unwrap();
        assert_eq!(start.status, TaskStatus::AwaitingRuntime);
        assert!(plane.offer_next_task(&runtime_id).unwrap().is_none());
        assert!(plane.runs.is_empty());
    }

    #[test]
    fn offline_runtime_requeues_waiting_task_after_heartbeat() {
        let mut plane = seeded_plane();
        let Fixture {
            workspace_id,
            requester,
            agent_id,
            runtime_id,
            ..
        } = fixture_without_online_runtime(
            &mut plane,
            AcceptancePolicy::AutoAcceptLowRisk,
            AgentVisibility::Public,
        );

        let start = plane
            .start_agent_conversation(&requester, &workspace_id, &agent_id, "run check")
            .unwrap();
        assert_eq!(start.status, TaskStatus::AwaitingRuntime);

        plane
            .runtime_heartbeat(&runtime_id, RuntimeStatus::Online)
            .unwrap();

        assert_eq!(
            plane.task(&start.task_id).unwrap().status,
            TaskStatus::Queued
        );
        let offer = plane.offer_next_task(&runtime_id).unwrap().unwrap();
        assert_eq!(offer.task_id, start.task_id);
    }

    #[test]
    fn wrong_runtime_cannot_consume_another_runtime_offer() {
        let mut plane = seeded_plane();
        let Fixture {
            workspace_id,
            requester,
            owner,
            agent_id,
            runtime_id,
        } = fixture(
            &mut plane,
            AcceptancePolicy::AutoAcceptLowRisk,
            AgentVisibility::Public,
        );
        let other_runtime = plane
            .register_runtime(
                &owner,
                &workspace_id,
                "Owner backup runtime",
                vec![RuntimeProject {
                    project_key: "orders-api".into(),
                    display_name: "Orders API".into(),
                    path_digest: Some("sha256:redacted-backup".into()),
                    status: RuntimeProjectStatus::Active,
                }],
            )
            .unwrap();
        plane
            .runtime_heartbeat(&other_runtime, RuntimeStatus::Online)
            .unwrap();

        let start = plane
            .start_agent_conversation(&requester, &workspace_id, &agent_id, "run check")
            .unwrap();
        plane.offer_next_task(&runtime_id).unwrap().unwrap();

        let wrong_runtime_error = plane
            .runtime_accept_task(&other_runtime, &start.task_id)
            .unwrap_err();
        assert_eq!(
            wrong_runtime_error,
            ControlPlaneError::Forbidden("task assigned to a different runtime".into())
        );
        assert!(
            plane
                .runtime_accept_task(&runtime_id, &start.task_id)
                .is_ok()
        );
    }

    #[test]
    fn owner_approval_policy_creates_task_approval_and_queues_after_approval() {
        let mut plane = seeded_plane();
        let Fixture {
            workspace_id,
            requester,
            owner,
            agent_id,
            runtime_id,
        } = fixture(
            &mut plane,
            AcceptancePolicy::RequiresOwnerApproval,
            AgentVisibility::Public,
        );

        let start = plane
            .start_agent_conversation(&requester, &workspace_id, &agent_id, "run check")
            .unwrap();
        assert_eq!(start.status, TaskStatus::AwaitingApproval);
        let approval_id = plane
            .approvals
            .values()
            .find(|approval| approval.task_id == start.task_id && approval.run_id.is_none())
            .map(|approval| approval.id.clone())
            .expect("task execution approval");

        let outcome = plane
            .decide_approval(&owner, &approval_id, true, Some("approved".into()))
            .unwrap();

        assert!(outcome.approved);
        assert_eq!(outcome.run_id, None);
        assert_eq!(
            plane.task(&start.task_id).unwrap().status,
            TaskStatus::Queued
        );
        assert!(plane.offer_next_task(&runtime_id).unwrap().is_some());
    }

    #[test]
    fn owner_approval_waits_for_runtime_before_creating_task_approval() {
        let mut plane = seeded_plane();
        let Fixture {
            workspace_id,
            requester,
            agent_id,
            runtime_id,
            ..
        } = fixture_without_online_runtime(
            &mut plane,
            AcceptancePolicy::RequiresOwnerApproval,
            AgentVisibility::Public,
        );

        let start = plane
            .start_agent_conversation(&requester, &workspace_id, &agent_id, "run check")
            .unwrap();
        assert_eq!(start.status, TaskStatus::AwaitingRuntime);
        assert!(
            plane
                .approvals
                .values()
                .all(|approval| approval.task_id != start.task_id)
        );

        plane
            .runtime_heartbeat(&runtime_id, RuntimeStatus::Online)
            .unwrap();

        assert_eq!(
            plane.task(&start.task_id).unwrap().status,
            TaskStatus::AwaitingApproval
        );
        assert!(
            plane
                .approvals
                .values()
                .any(|approval| approval.task_id == start.task_id && approval.run_id.is_none())
        );
    }

    #[test]
    fn approval_rejection_is_one_shot_and_user_visible() {
        let mut plane = seeded_plane();
        let Fixture {
            workspace_id,
            requester,
            owner,
            agent_id,
            runtime_id,
        } = fixture(
            &mut plane,
            AcceptancePolicy::AutoAcceptLowRisk,
            AgentVisibility::Public,
        );

        let start = plane
            .start_agent_conversation(&requester, &workspace_id, &agent_id, "delete temp branch")
            .unwrap();
        plane.offer_next_task(&runtime_id).unwrap().unwrap();
        let run_start = plane
            .runtime_accept_task(&runtime_id, &start.task_id)
            .unwrap();
        let approval_id = plane
            .runtime_request_approval(
                &runtime_id,
                RunApprovalRequested {
                    run_id: run_start.run_id.clone(),
                    action_type: "delete_branch".into(),
                    target: "temp/demo".into(),
                    scope: "repository branch".into(),
                    impact: "removes remote branch".into(),
                    recovery_plan: "restore from reflog".into(),
                    steps: vec!["git push origin :temp/demo".into()],
                    requested_payload: json!({ "redacted": true }),
                },
            )
            .unwrap();

        let decision = plane
            .decide_approval(&owner, &approval_id, false, Some("too broad".into()))
            .unwrap();
        assert!(!decision.approved);
        assert_eq!(
            plane.approval(&approval_id).unwrap().status,
            ApprovalStatus::Rejected
        );
        assert_eq!(
            plane.task(&start.task_id).unwrap().status,
            TaskStatus::Rejected
        );
        assert_eq!(
            plane.run(&run_start.run_id).unwrap().status,
            RunStatus::Cancelled
        );
    }

    #[test]
    fn handoff_creates_child_task_with_minimal_context_and_rejects_loop() {
        let mut plane = seeded_plane();
        let Fixture {
            workspace_id,
            requester,
            owner,
            agent_id,
            runtime_id,
        } = fixture(
            &mut plane,
            AcceptancePolicy::AutoAcceptLowRisk,
            AgentVisibility::Public,
        );
        let second_agent = create_bound_agent(
            &mut plane,
            &workspace_id,
            &owner,
            &runtime_id,
            "reporting",
            AgentVisibility::Public,
        );

        let start = plane
            .start_agent_conversation(&requester, &workspace_id, &agent_id, "analyze db")
            .unwrap();
        plane.offer_next_task(&runtime_id).unwrap().unwrap();
        plane
            .runtime_accept_task(&runtime_id, &start.task_id)
            .unwrap();
        let manifest = SharedContextManifest {
            summary: "Only includes the user request and a redacted query summary.".into(),
            items: vec![SharedContextItem {
                label: "request".into(),
                kind: SharedContextKind::UserRequest,
                sensitivity: Sensitivity::Internal,
                redacted: true,
            }],
        };

        let handoff = plane
            .request_handoff(
                &requester,
                &start.task_id,
                &second_agent,
                "build a short report",
                manifest.clone(),
            )
            .unwrap();
        let child = plane.task(&handoff.task_id).unwrap();
        assert_eq!(child.task_type, TaskType::Handoff);
        assert_eq!(
            child.parent_task_id.as_deref(),
            Some(start.task_id.as_str())
        );
        assert_eq!(child.shared_context_manifest, manifest);

        let loop_error = plane
            .request_handoff(
                &requester,
                &handoff.task_id,
                &agent_id,
                "loop back",
                SharedContextManifest::empty(),
            )
            .unwrap_err();
        assert_eq!(
            loop_error,
            ControlPlaneError::InvalidState("handoff loop detected".into())
        );
    }

    #[test]
    fn open_pool_discloses_redacted_summary_then_claims_with_capability_check() {
        let mut plane = seeded_plane();
        let Fixture {
            workspace_id,
            requester,
            owner,
            runtime_id,
            ..
        } = fixture(
            &mut plane,
            AcceptancePolicy::AutoAcceptLowRisk,
            AgentVisibility::Public,
        );
        let reporting_agent = create_bound_agent(
            &mut plane,
            &workspace_id,
            &owner,
            &runtime_id,
            "reporting",
            AgentVisibility::Public,
        );
        let open_task = plane
            .create_open_task(
                &requester,
                &workspace_id,
                "summarize data drift",
                vec!["reporting".into()],
                Sensitivity::Internal,
            )
            .unwrap();

        let offer = plane
            .claim_open_task(&owner, &open_task, &reporting_agent)
            .unwrap();
        assert_eq!(offer.task_kind, TaskKind::Open);
        assert_eq!(offer.agent_id, reporting_agent);
        assert!(
            plane
                .audit_events(&workspace_id)
                .iter()
                .any(|event| event.action == "task.open_created"
                    && event.redacted_payload["disclosure"] == "redacted_summary_only")
        );
    }

    #[test]
    fn migration_contains_all_stage_one_core_tables() {
        for table in [
            "users",
            "workspaces",
            "workspace_members",
            "agents",
            "runtimes",
            "conversations",
            "tasks",
            "runs",
            "approval_requests",
            "audit_events",
        ] {
            assert!(CORE_SCHEMA.contains(&format!("create table {table}")));
        }
    }

    struct Fixture {
        workspace_id: WorkspaceId,
        requester: UserId,
        owner: UserId,
        agent_id: AgentId,
        runtime_id: RuntimeId,
    }

    fn seeded_plane() -> MvpControlPlane {
        MvpControlPlane::default()
    }

    fn fixture(
        plane: &mut MvpControlPlane,
        acceptance_policy: AcceptancePolicy,
        visibility: AgentVisibility,
    ) -> Fixture {
        let fixture = fixture_without_online_runtime(plane, acceptance_policy, visibility);
        plane
            .runtime_heartbeat(&fixture.runtime_id, RuntimeStatus::Online)
            .unwrap();
        fixture
    }

    fn fixture_without_online_runtime(
        plane: &mut MvpControlPlane,
        acceptance_policy: AcceptancePolicy,
        visibility: AgentVisibility,
    ) -> Fixture {
        let owner = plane.seed_user("owner@example.com", "Owner");
        let requester = plane.seed_user("requester@example.com", "Requester");
        let workspace_id = plane.create_workspace(&owner, "Acme").unwrap();
        plane
            .add_member(&owner, &workspace_id, &requester, WorkspaceRole::Member)
            .unwrap();
        let runtime_id = plane
            .register_runtime(
                &owner,
                &workspace_id,
                "Owner MacBook",
                vec![RuntimeProject {
                    project_key: "orders-api".into(),
                    display_name: "Orders API".into(),
                    path_digest: Some("sha256:redacted".into()),
                    status: RuntimeProjectStatus::Active,
                }],
            )
            .unwrap();
        let agent_id = plane
            .create_agent(
                &owner,
                &workspace_id,
                AgentDefinition {
                    name: "database".into(),
                    description: "Read-only database diagnostics".into(),
                    visibility,
                    acceptance_policy,
                    capabilities: vec![AgentCapability {
                        capability_key: "database".into(),
                        display_name: "Database diagnostics".into(),
                        sensitivity: Sensitivity::Internal,
                    }],
                },
            )
            .unwrap();
        plane
            .bind_agent_runtime(&owner, &agent_id, &runtime_id, "orders-api")
            .unwrap();
        Fixture {
            workspace_id,
            requester,
            owner,
            agent_id,
            runtime_id,
        }
    }

    fn create_bound_agent(
        plane: &mut MvpControlPlane,
        workspace_id: &str,
        owner: &str,
        runtime_id: &str,
        capability: &str,
        visibility: AgentVisibility,
    ) -> AgentId {
        let agent_id = plane
            .create_agent(
                owner,
                workspace_id,
                AgentDefinition {
                    name: capability.into(),
                    description: format!("{capability} agent"),
                    visibility,
                    acceptance_policy: AcceptancePolicy::AutoAcceptLowRisk,
                    capabilities: vec![AgentCapability {
                        capability_key: capability.into(),
                        display_name: capability.into(),
                        sensitivity: Sensitivity::Internal,
                    }],
                },
            )
            .unwrap();
        plane
            .bind_agent_runtime(owner, &agent_id, runtime_id, "orders-api")
            .unwrap();
        agent_id
    }
}
