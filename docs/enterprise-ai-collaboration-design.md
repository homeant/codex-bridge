# 企业 AI 协作平台设计

状态：产品与架构基线提案  
更新时间：2026-08-14  
适用项目：IM Codex Bridge 及其后续桌面端、服务端演进

## 1. 文档目的

本文定义 IM Codex Bridge 向企业 AI 协作平台演进时的产品边界、核心领域模型、运行方式、权限原则和首版范围。

本文是新产品方向的主设计文档。现有 `desktop-app-design.md` 记录的是以本地 Bridge 管理控制台为中心的早期方案，两者暂时并存；后续实施新平台功能时，以本文为准。

## 2. 一句话定义

这是一个以 Workspace 为协作边界，让员工直接使用自己或其他员工负责的 Agent，并通过员工负责的本地或托管 Runtime 完成工作的企业 AI 协作平台。

产品不提供员工之间的聊天。员工只与 Agent 对话，Agent 可以发现和委托 Workspace 中的其他 Agent。

## 3. 已确认的产品决策

1. 用户是平台级账号，可以注册、登录，并加入多个 Workspace。
2. 用户可以创建多个 Workspace，也可以邀请其他平台注册用户加入自己创建的 Workspace。
3. Workspace 是协作、数据隔离、Agent 发现和权限控制的边界；没有 Workspace 就不能进行协作。
4. Agent 属于 Workspace，并由该 Workspace 中的一名员工负责。
5. 一个员工可以在同一个 Workspace 中负责多个 Agent，也可以在不同 Workspace 中分别负责多个 Agent。
6. Agent 不能没有负责人。负责人离开 Workspace 时，Agent 默认立即停用，除非先完成明确的负责人转移。
7. Runtime 可以运行在员工电脑上，也可以由服务端托管，但始终必须由明确的员工负责。
8. 第一版 Runtime Provider 默认只支持 Codex App Server；后续可以扩展 Claude 等 Provider。
9. 员工之间不能私聊或群聊；员工可以主动私聊 Workspace 中公开的 Agent，也可以在 Agent 会话中 `@` 其他 Agent。
10. Agent 的 `public` 表示对所属 Workspace 的全部成员可见，不支持 `selected_members`。
11. 第三方机器人不是必需组件，只是将企业微信、飞书等外部沟通工具接入 Agent 的可选 Channel。
12. 每一个外部机器人身份只路由到一个 Agent；一个 Agent 可以不配置外部机器人，也可以配置多个外部 Channel。
13. 所有委托都先进入服务端路由系统。定向委托进入目标 Agent 队列，未指定目标或主动释放的委托可以进入 Workspace 公开任务池。
14. Agent 可以发现当前 Workspace 中的其他公开 Agent、能力和可用状态，并在策略允许时发起 Handoff。

## 4. 产品边界

### 4.1 产品负责什么

- 平台用户的注册、登录与基础身份管理；
- Workspace 创建、邀请、成员和角色管理；
- Workspace 内 Agent 的创建、配置、发现和生命周期管理；
- 本地及托管 Runtime 的注册、在线状态和能力声明；
- 员工与 Agent 的直接会话；
- Agent 之间的任务委托、Handoff 和结果回传；
- 本地文件、工具、项目和敏感操作的授权；
- 第三方 IM Channel 的可选接入；
- 全链路审计、执行记录和安全策略。

### 4.2 产品不负责什么

- 员工之间的即时聊天；
- 员工群聊、频道、朋友圈、音视频会议；
- 通用邮箱、日历和在线文档套件；
- 脱离 Workspace 和员工负责人独立存在的公共 Agent；
- 默认将员工本地文件完整上传到服务端；
- 默认允许 Agent 绕过负责人、项目或 Runtime 的安全策略；
- 第一版的通用 Agent 工作流编排器或 Agent 市场。

## 5. 核心概念

### 5.1 Platform User

平台注册用户是全局身份，可以被其他平台注册用户搜索，以便收到 Workspace 邀请。

平台公开资料只包括必要信息，例如昵称、头像和简介。邮箱、设备、Runtime、Agent、项目、凭据和任务不因用户可搜索而公开。

### 5.2 Workspace

Workspace 是实际协作发生的安全边界，可以对应一家企业、一个业务团队或一个长期协作空间。

Workspace 管理：

- 成员与角色；
- Agent；
- Agent 能力目录；
- 项目与数据权限；
- 会话与任务；
- 公开任务池；
- Agent Handoff 策略；
- Runtime 使用策略；
- 第三方集成；
- 审计记录。

第一版不再在 Workspace 下增加 Team 层级。产品界面可以使用“团队空间”等更易理解的文案，但领域模型统一使用 `Workspace`。

### 5.3 Agent

Agent 是 Workspace 内受员工负责的 AI 工作入口。它可以具有明确的职责，例如：

- 代码诊断 Agent；
- 数据库分析 Agent；
- 发布检查 Agent；
- 报表 Agent；
- 需求分析 Agent。

Agent 不是员工本身，也不是无人负责的数字员工。它代表负责人愿意向 Workspace 开放的一组受控工作能力。

每个 Agent 必须满足：

- 属于且只属于一个 Workspace；
- 有且只有一个当前负责人；
- 负责人必须是该 Workspace 的有效成员；
- 至少具有一个能力说明；
- 执行时必须选择负责人可用且有权使用的 Runtime；
- 所有执行、审批和 Handoff 都可审计。

### 5.4 Runtime

Runtime 是 Agent 实际执行工作的环境，分为两类：

- `local`：运行在员工电脑上，可以访问员工授权的本地项目、工具和工作上下文；
- `hosted`：运行在服务端的隔离环境中，由员工负责配置和使用。

Runtime 属于平台用户，而不是 Workspace。一个 Runtime 可以在权限允许时承载该用户在多个 Workspace 中负责的 Agent。

Runtime 的公开能力元数据与真实执行权限必须分离。目录可以公开“具备 Rust 代码诊断能力”，但不能公开本地路径、密钥、环境变量或文件内容。

### 5.5 Conversation

Conversation 是员工与一个或多个 Agent 之间的协作会话。

第一版支持：

1. 员工与自己的 Agent 对话；
2. 员工主动私聊其他员工负责的公开 Agent；
3. 在已有 Agent 会话中 `@` 另一个公开 Agent，并创建协作分支。

Conversation 不允许将另一个员工作为消息接收者。Agent 的负责人可以查看授权请求和按策略审计执行，但不会自动成为会话参与者。

### 5.6 Task

Task 是 Agent 执行与委托的服务端记录。Conversation 面向用户，Task 面向路由、状态机、执行和审计。

Task 分为：

- `targeted`：已指定目标 Agent，进入该 Agent 的队列；
- `open`：未指定目标 Agent，进入所属 Workspace 的公开任务池；
- `handoff`：由一个 Agent 委托给另一个 Agent 的子任务。

公开任务池只对当前 Workspace 生效，不是互联网公开池，也不能跨 Workspace 自动暴露任务内容。

### 5.7 Channel

Channel 是可选的第三方沟通入口，例如企业微信或飞书机器人。

没有 Channel 时，用户可以完全通过桌面端或 Web 端与 Agent 协作。配置 Channel 后，外部消息被转换为平台内的 Conversation Message 和 Task，并使用相同的权限、路由和审计流程。

## 6. 核心关系

```text
Platform User
├─ N Workspace Membership
├─ N Runtime
└─ N Agent Ownership（跨不同 Workspace）

Workspace
├─ N Member
├─ N Agent
├─ N Project
├─ N Conversation
├─ N Task
└─ N Integration Connection

Agent
├─ 1 Workspace
├─ 1 Responsible User
├─ N Runtime Binding
└─ N Optional Channel Binding
```

关键基数：

```text
User      N ── N Workspace
Workspace 1 ── N Agent
User      1 ── N Agent
User      1 ── N Runtime
Agent     N ── N Runtime
Agent     1 ── N Channel Binding
```

第一版可以将 `Agent N ── N Runtime` 简化为每个 Agent 一个主要 Runtime，但数据模型应允许后续增加优先级、故障切换和按能力选择。

## 7. 多 Workspace 模型

用户可以：

- 创建多个 Workspace；
- 加入多个 Workspace；
- 在不同 Workspace 使用不同角色；
- 在每个 Workspace 创建和负责不同 Agent；
- 使用同一个本地 Runtime 承载多个 Workspace 的 Agent。

不同 Workspace 必须隔离：

- Agent 目录；
- 项目和本地路径映射；
- Conversation；
- Task 和公开任务池；
- Handoff；
- 第三方集成；
- 凭据；
- 审计记录。

Agent 不允许跨 Workspace 被直接 `@` 或私聊。跨 Workspace 协作不属于第一版范围。

## 8. 成员和角色

Workspace 首版角色：

- `owner`：Workspace 所有者，负责所有权转移、解散和最高级策略；
- `admin`：管理成员、Agent、集成、项目和安全策略；
- `member`：创建自己的 Agent、配置自己的 Runtime、调用公开 Agent。

创建 Workspace 的用户自动成为 `owner`。一个 Workspace 首版只保留一个 Owner，可以有多个 Admin。

### 8.1 邀请

支持：

- 在平台用户目录搜索用户并邀请；
- 生成带有效期的邀请码或邀请链接；
- 被邀请人接受或拒绝；
- Owner/Admin 撤销未处理邀请。

邀请默认加入为 Member。角色提升需要加入后由 Owner/Admin 单独操作。

### 8.2 成员退出

成员退出或被移除前必须处理其 Agent：

1. 默认将该成员负责的 Agent 设为 `suspended`；
2. 停止分配新任务；
3. 对运行中任务执行取消、等待结束或管理员接管策略；
4. 撤销该 Workspace 对其 Runtime 的使用授权；
5. 如需保留 Agent，必须先将负责人转移给另一个 Workspace 成员；
6. 转移后必须重新确认 Runtime、项目和凭据权限。

## 9. Agent 可见性与可调用性

Agent 可见性只设置两档：

- `private`：仅负责人和 Workspace 管理员可见；
- `public`：Workspace 全体成员可见。

不支持 `selected_members`，也不维护成员级可见白名单。

可见不等于可以无条件执行。系统必须分别判断：

- 是否可以发现 Agent；
- 是否可以向 Agent 发起 Conversation；
- Agent 是否接受直接委托；
- Runtime 是否在线；
- Runtime 是否具备所需能力；
- 是否可以访问目标项目或数据；
- 是否需要负责人确认；
- 是否允许 Handoff。

## 10. 员工与 Agent 的交互

### 10.1 私聊公开 Agent

张三可以在 Agent 目录中找到李四负责的数据库 Agent，并直接发起会话：

```text
张三 → 李四的数据库 Agent

张三：帮我检查昨天支付数据库的连接数峰值。
Agent：本次请求需要使用李四负责的数据库 Runtime，正在等待授权。
```

李四收到的是执行授权，不是张三发来的聊天消息：

```text
请求方：张三
目标 Agent：数据库诊断 Agent
申请能力：数据库只读查询
目标范围：支付数据库昨日连接指标

[允许本次执行] [拒绝]
```

### 10.2 在会话中 @其他 Agent

```text
张三 → 代码诊断 Agent

张三：代码没有发现问题，@数据库诊断Agent 检查连接数。
```

服务端创建一个 Handoff 子任务，并将数据库 Agent 加入当前协作上下文。父会话保留完整的任务关系，但只向新 Agent 传递完成工作所需的最少上下文。

### 10.3 Agent 主动发起 Handoff

Agent 可以根据能力目录提出协作建议：

```text
代码诊断 Agent：当前问题需要数据库只读能力，建议委托给数据库诊断 Agent。

[允许委托] [选择其他 Agent] [取消]
```

是否自动委托由 Workspace 策略、Agent 策略、数据敏感级别和负责人授权共同决定。

## 11. 服务端任务池

所有任务先持久化到服务端，再由 Runtime Connector 拉取或由服务端通过已建立的安全连接通知。

### 11.1 定向队列

用户私聊或 `@` 某个 Agent 时，创建 `targeted` Task：

```text
workspace_id + target_agent_id + requester_user_id
```

只有目标 Agent 及其负责人可以处理。

### 11.2 Workspace 公开池

以下情况可以创建 `open` Task：

- 用户只描述能力需求，没有指定 Agent；
- 目标 Agent 无法处理，并经授权释放到公开池；
- Agent 发起 Handoff，但没有唯一候选 Agent；
- Workspace 管理策略要求集中分派。

公开池中的任务对 Workspace 内符合条件的公开 Agent 可发现。任务载荷采用分层披露：

1. 候选阶段只显示脱敏摘要、所需能力、期限和敏感级别；
2. Agent 接受前再次校验负责人、项目和 Runtime 权限；
3. 接受后只下发完成任务所需的最少上下文；
4. 不因进入公开池而公开本地文件、密钥或完整历史会话。

### 11.3 Handoff

Handoff 不直接移动原任务，而是创建子任务：

```text
parent_task_id
source_agent_id
target_agent_id | open_pool
reason
required_capabilities
shared_context_manifest
```

系统必须限制：

- 最大 Handoff 深度；
- 重复 Agent 和环路；
- 跨 Workspace 路由；
- 总执行预算；
- 敏感数据传播；
- 未经授权的项目访问。

## 12. Runtime 设计

### 12.1 Runtime 所有权

Runtime 归平台注册用户负责，可以是：

- 用户电脑上的 Runtime Connector；
- 用户负责的服务端托管隔离环境。

“服务端托管”只改变部署位置，不改变责任关系。平台不能把员工负责的托管 Runtime 自动变成公共执行资源。

### 12.2 Provider 抽象

第一版仅实现：

```text
provider = codex_app_server
```

后续可增加：

```text
provider = claude
provider = other
```

建议的 Provider Driver 接口：

```text
initialize
get_capabilities
start_run
resume_run
steer_run
cancel_run
resolve_interaction
read_run
shutdown
```

平台任务模型不得直接依赖 Codex Thread 字段。Provider 特有标识保存在 Run 的扩展字段中。

### 12.3 本地 Runtime

本地 Runtime Connector：

- 由员工登录桌面端后启动；
- 主动向服务端建立出站安全连接；
- 在本机通过 stdio 连接 Codex App Server；
- 不将 Codex App Server 端口直接暴露给服务端或公网；
- 只允许访问配置过的项目 ID 和允许根目录；
- 对未知项目、越界路径和符号链接逃逸失败关闭；
- 不上报原始密钥、认证文件或无关文件内容。

### 12.4 托管 Runtime

托管 Runtime 必须：

- 运行在独立隔离环境；
- 绑定明确的 `owner_user_id`；
- 使用独立凭据和工作目录；
- 记录创建、启动、停止和执行审计；
- 支持负责人随时暂停和撤销；
- 在负责人离开 Workspace 后撤销该 Workspace 的访问；
- 不自动继承其他用户、Workspace 或 Agent 的权限。

### 12.5 Agent 与 Runtime 绑定

一个 Agent 可以配置多个 Runtime 候选：

```text
数据库诊断 Agent
├─ 优先：李四的本地 MacBook Runtime
└─ 降级：李四负责的托管 Runtime
```

第一版建议只启用一个主要 Runtime，以降低调度和状态一致性复杂度；数据模型保留多绑定和优先级。

## 13. Run 与 Provider 映射

每次 Task 执行产生一个或多个 Run：

```text
Run
- task_id
- agent_id
- runtime_id
- provider
- provider_run_id
- status
- started_at
- finished_at
```

Codex App Server Provider 中：

- 一个 Run 对应一个 Codex Thread 或 Thread 中的受控执行段；
- `provider_run_id` 保存 Codex Thread ID；
- 澄清、审批和执行事件映射到统一 Run Event；
- 平台不读取或展示隐藏分析和思维链。

## 14. 第三方集成

第三方 IM 是可选入口，不是 Agent 的必要组成部分。

```text
桌面端 / Web 端 ──────────┐
                          ├→ Conversation / Task → Agent → Runtime
企业微信 / 飞书 Channel ──┘
```

### 14.1 连接模型

```text
integration_connections
- id
- workspace_id
- provider
- name
- encrypted_credentials
- status
- created_by

agent_channel_bindings
- id
- workspace_id
- agent_id
- integration_connection_id
- external_bot_id
- enabled
```

约束：

- 外部机器人身份只能绑定一个 Agent；
- Agent 可以没有 Channel；
- Agent 可以绑定多个不同 Channel；
- 外部身份必须映射到平台用户或受限访客身份；
- 外部消息必须解析出唯一 Workspace 和目标 Agent；
- 无法确认身份、Workspace 或 Agent 时拒绝执行；
- Channel 不能绕过 Workspace、Agent、项目或 Runtime 权限。

## 15. 权限与审批

权限判断至少包含：

```text
平台身份有效
AND Workspace 成员有效
AND Agent 属于 Workspace
AND Agent 负责人有效
AND Agent 可见且允许调用
AND Runtime 归负责人控制
AND Runtime 具备所需能力
AND 项目授权有效
AND 操作风险策略允许
```

Agent 负责人可以设置：

- 自动接受低风险只读任务；
- 所有任务都需确认；
- 指定能力必须确认；
- 指定项目禁止远程委托；
- 指定时间段不接受任务；
- 本地 Runtime 离线时是否允许托管 Runtime 降级。

高风险操作必须展示精确目标、范围、影响和恢复方式，并使用一次性审批。审批不能转换成永久权限。

## 16. 项目与文件模型

Project 属于 Workspace，但实际文件可以位于不同员工的 Runtime 中。

```text
Project
- workspace_id
- stable_project_id
- name
- description

Runtime Project Mapping
- runtime_id
- project_id
- canonical_local_path | hosted_workspace_ref
- access_level
- validation_status
```

服务端只维护稳定 Project ID 和必要元数据。对于本地 Runtime：

- 规范路径保存在员工本地配置；
- 服务端不得将模型输出作为文件系统权限；
- Connector 根据 Project ID 解析本地路径；
- 路径必须位于允许根目录；
- 未知 Project ID 和越界路径必须拒绝。

## 17. 数据模型草案

### 17.1 身份与 Workspace

```text
users
- id
- email
- password_hash
- display_name
- avatar_url
- status
- created_at

workspaces
- id
- name
- description
- created_by
- status
- created_at

workspace_members
- workspace_id
- user_id
- role: owner | admin | member
- status
- joined_at

workspace_invitations
- id
- workspace_id
- inviter_user_id
- invitee_user_id nullable
- invite_code_hash
- role
- status
- expires_at
```

### 17.2 Agent 与 Runtime

```text
agents
- id
- workspace_id
- owner_user_id
- name
- description
- visibility: private | public
- invocation_policy
- approval_policy
- status: active | paused | suspended | archived
- created_at

agent_capabilities
- id
- agent_id
- capability_key
- display_name
- description
- sensitivity

runtimes
- id
- owner_user_id
- runtime_type: local | hosted
- provider
- device_or_instance_id
- display_name
- status
- last_seen_at

agent_runtime_bindings
- agent_id
- runtime_id
- priority
- enabled
```

### 17.3 会话与任务

```text
conversations
- id
- workspace_id
- creator_user_id
- title
- status
- created_at

conversation_participants
- conversation_id
- participant_type: user | agent
- participant_id
- role

conversation_messages
- id
- conversation_id
- sender_type: user | agent | system
- sender_id
- content_ref
- reply_to_message_id nullable
- created_at

tasks
- id
- workspace_id
- conversation_id
- creator_user_id
- target_agent_id nullable
- parent_task_id nullable
- task_type: targeted | open | handoff
- required_capabilities
- sensitivity
- status
- created_at

runs
- id
- task_id
- agent_id
- runtime_id
- provider
- provider_run_id
- status
- started_at
- finished_at
```

`conversation_participants` 中可以有多个 Agent，但普通 User 参与者第一版只能是会话创建者。系统禁止将另一个 User 添加为聊天对象，从数据层避免员工聊天能力逐步渗入。

### 17.4 集成与审计

```text
integration_connections
agent_channel_bindings
external_identity_bindings
approval_requests
audit_events
```

敏感凭据必须加密保存，日志和审计中只能出现脱敏标识。

## 18. 状态机

### 18.1 Task

```text
created
  → queued
  → awaiting_acceptance
  → assigned
  → running
  → awaiting_input | awaiting_approval
  → completed | failed | rejected | cancelled | expired
```

### 18.2 Agent

```text
active
  ↔ paused
  → suspended
  → archived
```

- `paused`：负责人主动暂停，不接收新任务；
- `suspended`：负责人失效、权限异常或管理员停用；
- `archived`：历史只读，不再执行。

### 18.3 Runtime

```text
offline → connecting → online → busy
                    ↘ degraded
                    ↘ revoked
```

Runtime 在线不代表某个 Agent 一定可执行；仍需检查绑定、能力和项目权限。

## 19. 桌面端信息架构

### 19.1 我的 Agent

- 查看和切换自己在当前 Workspace 负责的多个 Agent；
- 与自己的 Agent 对话；
- 创建、编辑、暂停和归档 Agent；
- 配置能力、Runtime、权限和接受策略。

### 19.2 Agent 目录

- 展示当前 Workspace 的公开 Agent；
- 按能力、负责人和可用状态筛选；
- 直接打开 Agent 私聊；
- 不暴露本地路径、凭据和完整执行上下文。

### 19.3 会话

- 员工与 Agent 的会话列表；
- Agent 回复、执行状态、澄清和结果；
- 在消息中 `@` 其他 Agent；
- 不支持将另一个员工加入会话或发送员工私聊。

### 19.4 委托与审批

- 我发起的任务；
- 我的 Agent 收到的任务；
- Workspace 公开任务池；
- 待接受、待审批和待补充信息；
- Handoff 链路与最终结果。

### 19.5 Runtime

- 本地 Codex App Server 连接状态；
- 托管 Runtime 状态；
- Project ID 与本地路径映射；
- 可用能力和安全策略；
- 最近执行与异常。

### 19.6 Workspace 设置

- 成员和邀请；
- 角色；
- Agent 与任务策略；
- 项目；
- 第三方集成；
- 审计。

## 20. 服务架构

```text
Desktop / Web Client
        │
        ├───────────────┐
        │               │
WeCom / Feishu      Control Plane
Channel Adapter     ├─ Identity
        │            ├─ Workspace
        └───────────→├─ Agent Directory
                     ├─ Conversation
                     ├─ Task Pool / Router
                     ├─ Approval
                     └─ Audit
                            │
                   outbound secure connection
                            │
                Runtime Connector / Hosted Runtime
                            │
                    Runtime Provider Driver
                            │
                  Codex App Server（首版）
```

Control Plane 不直接假设 Runtime 位于员工电脑还是服务端。它只根据 Runtime 所有权、在线状态、能力和策略进行调度。

## 21. 安全原则

1. Workspace 是强隔离边界，所有核心表都必须携带并校验 `workspace_id`。
2. 用户可被平台搜索不意味着其 Agent、Runtime 或任务全平台公开。
3. Agent 的公开范围最多是所属 Workspace 全体成员。
4. Runtime 能力描述不能包含本地路径、文件内容、密钥或认证材料。
5. 本地 App Server 通过本机 stdio 使用，不直接暴露到网络。
6. Runtime Connector 主动建立出站连接，服务端不能任意扫描员工电脑。
7. 所有外部消息、引用、日志和模型输出均视为不可信输入。
8. Project ID 必须由可信配置解析，未知 ID 和越界路径失败关闭。
9. Handoff 只传递最少必要上下文，不复制完整源会话和全部文件。
10. 负责人、Agent、Runtime、项目和审批的每次关键变化都写入审计日志。
11. 账户停用后，用户会话、Runtime Token 和 Agent 执行权限必须立即撤销。
12. UI 登录会话与 Runtime 设备令牌必须分离。

## 22. 第一版范围

### 22.1 必须完成

- 邮箱注册、验证、登录和密码找回；
- 用户公开目录的最小资料；
- 创建多个 Workspace；
- 邀请用户加入 Workspace；
- Owner/Admin/Member 三种角色；
- 一个成员创建和负责多个 Agent；
- Agent `private/public` 可见性；
- Workspace 内 Agent 目录；
- 员工私聊 Agent；
- 在 Agent 会话中 `@` 另一个 Agent；
- 定向 Agent 队列；
- Workspace 公开任务池；
- Handoff 子任务和基本防环；
- 本地 Runtime Connector；
- Codex App Server Provider；
- 每个 Agent 一个主要 Runtime；
- 项目目录与允许根目录校验；
- 自动接受与人工确认；
- 基础审批和审计；
- 企业微信、飞书作为可选 Channel。

### 22.2 数据模型预留、首版可不开放

- 服务端托管 Runtime；
- 一个 Agent 绑定多个 Runtime；
- Runtime 自动故障切换；
- Claude 等其他 Provider；
- 复杂能力匹配；
- 公开池自动竞标和负载均衡；
- 跨 Workspace 协作；
- 自定义角色；
- 企业组织树和部门同步；
- 自动多 Agent DAG 编排。

## 23. 验收场景

### 场景 A：直接调用其他员工的 Agent

1. 张三和李四属于同一个 Workspace；
2. 李四创建公开的数据库诊断 Agent；
3. 张三从 Agent 目录打开私聊并提交只读检查请求；
4. 服务端创建定向 Task；
5. 李四确认授权；
6. 李四负责的 Runtime 执行；
7. 结果返回张三与该 Agent 的原会话；
8. 张三和李四之间没有创建聊天会话。

### 场景 B：会话中 @另一个 Agent

1. 张三正在与代码 Agent 对话；
2. 张三 `@` 数据库 Agent；
3. 服务端校验两个 Agent 属于同一个 Workspace；
4. 创建 Handoff 子任务；
5. 只向数据库 Agent 传递最小必要上下文；
6. 子任务结果回到父会话并由代码 Agent 汇总。

### 场景 C：员工负责多个 Agent

1. 李四在同一个 Workspace 创建数据库诊断 Agent 和报表 Agent；
2. 两个 Agent 具有不同能力和审批策略；
3. 两个 Agent 可以共用李四的 Runtime，也可以使用不同 Runtime；
4. Workspace 成员能够分别发现和调用两个 Agent。

### 场景 D：本地 Runtime 离线

1. 目标 Agent 的主要本地 Runtime 离线；
2. 服务端保留任务并展示等待状态；
3. 未配置托管降级时不得在其他员工 Runtime 上执行；
4. Runtime 恢复在线后继续调度，或由请求方取消任务。

### 场景 E：负责人退出 Workspace

1. 管理员尝试移除负责多个 Agent 的员工；
2. 系统提示先转移或停用这些 Agent；
3. 未完成转移时，成员移除后 Agent 自动 `suspended`；
4. 本地与托管 Runtime 均不再代表该 Workspace 执行任务。

## 24. 与 Multica 类产品的边界

本产品会共享 Workspace、Agent、Runtime 和任务路由等通用技术概念，但产品约束是：

- Agent 必须有 Workspace 员工负责人；
- 一个员工可以负责多个不同能力的 Agent；
- Runtime 无论本地还是托管，都必须有员工负责人；
- 产品重点是员工向同事开放受控 AI 工作能力；
- 员工可以直接与其他员工负责的 Agent 对话；
- 员工之间不在产品内聊天；
- 不以无人负责的 Agent 团队、Issue 看板或通用编排为中心。

如果未来允许没有员工负责人的 Agent，或者将所有 Runtime 变成可由平台任意调度的公共资源，产品将偏离本设计，并重新接近通用多 Agent 工作平台。

## 25. 后续设计工作

下一阶段需要分别补充：

1. 服务端 API 与事件协议；
2. Workspace、Agent、Runtime、Conversation、Task 的数据库 DDL；
3. 桌面端关键页面和交互原型；
4. Runtime Connector 注册、心跳和任务领取协议；
5. Agent Handoff 协议与上下文清单格式；
6. 本地与托管 Runtime 的凭据和隔离方案；
7. 现有单机 Bridge 向 Control Plane + Runtime Connector 的演进计划。
