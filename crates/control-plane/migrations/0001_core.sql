create table users (
  id text primary key,
  email text not null unique,
  display_name text not null,
  status text not null check (status in ('pending_verification', 'active', 'disabled'))
);

create table workspaces (
  id text primary key,
  name text not null,
  description text,
  created_by text not null references users(id),
  status text not null check (status in ('active', 'disabled', 'archived'))
);

create table workspace_members (
  workspace_id text not null references workspaces(id),
  user_id text not null references users(id),
  role text not null check (role in ('owner', 'admin', 'member')),
  status text not null check (status in ('active', 'disabled', 'left')),
  primary key (workspace_id, user_id)
);

create table agents (
  id text primary key,
  workspace_id text not null references workspaces(id),
  owner_user_id text not null references users(id),
  name text not null,
  description text not null,
  visibility text not null check (visibility in ('private', 'public')),
  status text not null check (status in ('active', 'paused', 'suspended', 'archived')),
  acceptance_policy text not null check (acceptance_policy in ('auto_accept_low_risk', 'requires_owner_approval')),
  unique (workspace_id, owner_user_id, name)
);

create table agent_capabilities (
  workspace_id text not null references workspaces(id),
  agent_id text not null references agents(id) on delete cascade,
  capability_key text not null,
  display_name text not null,
  sensitivity text not null check (sensitivity in ('normal', 'internal', 'sensitive', 'restricted')),
  primary key (agent_id, capability_key)
);

create table runtimes (
  id text primary key,
  workspace_id text not null references workspaces(id),
  owner_user_id text not null references users(id),
  provider text not null check (provider in ('codex_app_server')),
  display_name text not null,
  status text not null check (status in ('offline', 'connecting', 'online', 'busy', 'degraded', 'revoked'))
);

create table runtime_projects (
  workspace_id text not null references workspaces(id),
  runtime_id text not null references runtimes(id) on delete cascade,
  project_key text not null,
  display_name text not null,
  path_digest text,
  status text not null check (status in ('active', 'disabled')),
  primary key (runtime_id, project_key)
);

create table agent_runtime_bindings (
  workspace_id text not null references workspaces(id),
  agent_id text not null references agents(id) on delete cascade,
  runtime_id text not null references runtimes(id),
  primary_project_key text not null,
  enabled boolean not null,
  primary key (agent_id, runtime_id)
);

create table conversations (
  id text primary key,
  workspace_id text not null references workspaces(id),
  creator_user_id text not null references users(id),
  channel text not null check (channel in ('platform', 'wecom', 'lark')),
  title text,
  status text not null check (status in ('active', 'archived'))
);

create table conversation_participants (
  workspace_id text not null references workspaces(id),
  conversation_id text not null references conversations(id) on delete cascade,
  participant_type text not null check (participant_type in ('user', 'agent')),
  participant_id text not null,
  role text not null check (role in ('creator', 'target_agent', 'mentioned_agent')),
  primary key (conversation_id, participant_type, participant_id)
);

create table conversation_messages (
  id text primary key,
  workspace_id text not null references workspaces(id),
  conversation_id text not null references conversations(id) on delete cascade,
  sender_type text not null check (sender_type in ('user', 'agent', 'system', 'runtime')),
  sender_id text,
  content text not null
);

create table tasks (
  id text primary key,
  workspace_id text not null references workspaces(id),
  conversation_id text references conversations(id),
  creator_user_id text references users(id),
  target_agent_id text references agents(id),
  assigned_agent_id text references agents(id),
  assigned_runtime_id text references runtimes(id),
  parent_task_id text references tasks(id),
  task_type text not null check (task_type in ('targeted', 'open', 'handoff')),
  prompt text not null,
  required_capabilities text not null,
  sensitivity text not null check (sensitivity in ('normal', 'internal', 'sensitive', 'restricted')),
  shared_context_manifest text not null,
  status text not null check (status in ('created', 'queued', 'awaiting_runtime', 'awaiting_acceptance', 'assigned', 'running', 'awaiting_input', 'awaiting_approval', 'completed', 'failed', 'rejected', 'cancelled', 'expired'))
);

create table runs (
  id text primary key,
  workspace_id text not null references workspaces(id),
  task_id text not null references tasks(id) on delete cascade,
  agent_id text not null references agents(id),
  runtime_id text not null references runtimes(id),
  provider text not null check (provider in ('codex_app_server')),
  status text not null check (status in ('starting', 'running', 'awaiting_input', 'awaiting_approval', 'completed', 'failed', 'cancelled')),
  result_ref text,
  error_code text,
  error_message text
);

create table approval_requests (
  id text primary key,
  workspace_id text not null references workspaces(id),
  task_id text not null references tasks(id),
  run_id text references runs(id),
  approver_user_id text not null references users(id),
  action_type text not null,
  target text not null,
  scope text not null,
  impact text not null,
  recovery_plan text not null,
  status text not null check (status in ('pending', 'approved', 'rejected', 'expired', 'cancelled'))
);

create table audit_events (
  id text primary key,
  workspace_id text not null references workspaces(id),
  actor_type text not null check (actor_type in ('user', 'agent', 'runtime', 'system', 'channel')),
  actor_id text,
  action text not null,
  resource_type text not null,
  resource_id text,
  redacted_payload text not null
);

create index idx_agents_workspace_visibility on agents(workspace_id, visibility, status);
create index idx_tasks_queue on tasks(workspace_id, status, target_agent_id);
create index idx_runtime_status on runtimes(workspace_id, status);
create index idx_audit_workspace on audit_events(workspace_id);
