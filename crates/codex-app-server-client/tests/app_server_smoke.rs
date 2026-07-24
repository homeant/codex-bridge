use std::{env, path::PathBuf, time::Duration};

use codex_app_server_client::{CodexAppServer, TurnEvent};
use serde_json::json;
use tokio::time::timeout;

#[tokio::test]
#[ignore = "requires an explicitly configured model provider and performs a live model turn"]
async fn configured_model_provider_starts_and_resumes_a_thread() {
    let binary = env::var("CODEX_E2E_BINARY").unwrap_or_else(|_| "codex".into());
    let model = env::var("CODEX_E2E_MODEL").expect("CODEX_E2E_MODEL is required");
    let model_provider =
        env::var("CODEX_E2E_MODEL_PROVIDER").expect("CODEX_E2E_MODEL_PROVIDER is required");
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repository root");
    let roots = vec![repository.to_string_lossy().into_owned()];

    let client = CodexAppServer::spawn_with_model(
        &binary,
        Some(model.clone()),
        Some(model_provider.clone()),
    )
    .await
    .expect("start configured app-server");
    let thread_id = client
        .start_project_thread(
            &repository,
            &roots,
            "This is a configured model-provider smoke test. Do not modify files.",
        )
        .await
        .expect("start configured thread");
    let turn = client
        .start_turn(
            &thread_id,
            "只回复 IM_PROVIDER_OK，不调用任何工具。",
            "Configured model-provider smoke test.",
            &[],
        )
        .await
        .expect("start configured turn");
    assert!(completed_text(turn).await.contains("IM_PROVIDER_OK"));
    client
        .unsubscribe(&thread_id)
        .await
        .expect("unsubscribe configured thread");
    drop(client);

    let resumed_client =
        CodexAppServer::spawn_with_model(&binary, Some(model), Some(model_provider))
            .await
            .expect("start configured resume app-server");
    let resumed_id = resumed_client
        .resume_thread(
            &thread_id,
            "This is a configured model-provider smoke test. Do not modify files.",
        )
        .await
        .expect("resume configured thread");
    assert_eq!(resumed_id, thread_id);
}

#[tokio::test]
#[ignore = "requires a locally authenticated Codex CLI and performs live model turns"]
async fn creates_a_thread_returns_text_and_resumes_it_in_a_new_server() {
    let binary = env::var("CODEX_E2E_BINARY").unwrap_or_else(|_| "codex".into());
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repository root");
    let roots = vec![repository.to_string_lossy().into_owned()];

    let client = CodexAppServer::spawn(&binary)
        .await
        .expect("start first app-server");
    let mut server_requests = client
        .take_server_requests()
        .await
        .expect("take server request stream");
    let thread_id = client
        .start_project_thread(
            &repository,
            &roots,
            "This is a protocol smoke test. Do not modify files. Return a concise final answer.",
        )
        .await
        .expect("start thread");
    let first = client
        .start_turn(
            &thread_id,
            "这是本地协议连通性测试，标记为 APP_SERVER_TASK_TOOL_OK_7F31。不要修改文件；请用一句中文确认已收到。",
            "Protocol smoke test from local development.",
            &[],
        )
        .await
        .expect("start first turn");
    client
        .steer(
            &thread_id,
            &first.turn_id,
            "补充协议测试信息：STEER_OK_42。",
            "Protocol smoke test from local development.",
            &[],
        )
        .await
        .expect("steer active turn");
    let first_text = completed_text(first).await;
    assert!(!first_text.trim().is_empty());
    client
        .unsubscribe(&thread_id)
        .await
        .expect("unsubscribe completed source thread");

    let inspector_id = client
        .start_project_thread(
            &repository,
            &roots,
            "This is a read-only protocol smoke test. Do not modify files.",
        )
        .await
        .expect("start inspector thread");
    let inspection = client
        .start_turn(
            &inspector_id,
            &format!(
                "先调用 codex_app.list_threads，再调用 codex_app.read_thread 读取任务 {thread_id}。只回复该任务最近补充消息中的 STEER 测试标记，不要使用 shell 或 skill。"
            ),
            "Protocol smoke test from local development.",
            &[],
        )
        .await
        .expect("start task-tool inspection turn");
    let inspection_text = completed_text(inspection).await;
    assert!(inspection_text.contains("STEER_OK_42"));
    client
        .unsubscribe(&inspector_id)
        .await
        .expect("unsubscribe completed inspector thread");

    let interaction_id = client
        .start_project_thread(
            &repository,
            &roots,
            "This is an interactive protocol smoke test. Do not modify files.",
        )
        .await
        .expect("start interaction thread");
    let interaction = client
        .start_turn(
            &interaction_id,
            "这是审批协议测试。必须调用 codex_app.request_approval，为一次假想的 production_restart 填写完整审批信息；不要运行任何命令或调用其他工具。工具返回未批准后，用一句中文确认未执行。",
            "Protocol smoke test from local development.",
            &[],
        )
        .await
        .expect("start approval turn");
    let request = timeout(Duration::from_secs(120), server_requests.recv())
        .await
        .expect("wait for explicit approval tool")
        .expect("server request stream closed");
    assert_eq!(request.method, "item/tool/call");
    assert_eq!(request.params["namespace"], "codex_app");
    assert_eq!(request.params["tool"], "request_approval");
    request
        .respond(json!({
            "approved": false,
            "message": "Protocol test declined. Do not execute."
        }))
        .expect("decline explicit approval");
    let interaction_text = completed_text(interaction).await;
    assert!(!interaction_text.trim().is_empty());
    client
        .unsubscribe(&interaction_id)
        .await
        .expect("unsubscribe interaction thread");
    drop(client);

    let resumed_client = CodexAppServer::spawn(&binary)
        .await
        .expect("start second app-server");
    let resumed_id = resumed_client
        .resume_thread(
            &thread_id,
            "This is a protocol smoke test. Do not modify files. Return a concise final answer.",
        )
        .await
        .expect("resume thread");
    assert_eq!(resumed_id, thread_id);
    let second = resumed_client
        .start_turn(
            &resumed_id,
            "继续刚才的连通性测试。不要修改文件；请只回复：会话已继续。",
            "Protocol smoke test from local development.",
            &[],
        )
        .await
        .expect("start resumed turn");
    let second_text = completed_text(second).await;
    assert!(!second_text.trim().is_empty());
}

async fn completed_text(mut turn: codex_app_server_client::TurnHandle) -> String {
    timeout(Duration::from_secs(240), async move {
        while let Some(event) = turn.events.recv().await {
            match event {
                TurnEvent::Completed { text, .. } => return text,
                TurnEvent::Failed(error) => panic!("Codex turn failed: {error}"),
                TurnEvent::Progress(_) => {}
            }
        }
        panic!("Codex turn stream closed without a terminal event")
    })
    .await
    .expect("Codex turn timed out")
}
