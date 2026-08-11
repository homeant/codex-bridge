use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use bridge_core::{AdapterConfig, BridgeConfig, BridgeService, StateStore};
use channel_protocol::{AdapterCommand, AdapterEvent};
use codex_app_server_client::CodexAppServer;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
    time::timeout,
};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

mod shell_env;

struct AdapterProcess {
    name: String,
    child: Child,
}

struct Cli {
    config_path: PathBuf,
    check: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = parse_cli()?;
    let config_path = if cli.config_path.is_absolute() {
        cli.config_path
    } else {
        std::env::current_dir()
            .context("resolve current directory")?
            .join(cli.config_path)
    };
    let config = Arc::new(BridgeConfig::load(&config_path)?);
    let state = Arc::new(StateStore::open(&config.state.sqlite_path)?);
    info!(
        model = config.codex.model.as_deref().unwrap_or("<default>"),
        model_provider = config
            .codex
            .model_provider
            .as_deref()
            .unwrap_or("<default>"),
        "Codex model selection configured"
    );
    let codex_environment = shell_env::codex_environment().await;
    let codex = CodexAppServer::spawn_with_model_and_environment(
        &config.codex.binary,
        config.codex.model.clone(),
        config.codex.model_provider.clone(),
        &codex_environment,
    )
    .await?;
    if cli.check {
        println!(
            "configuration, state, project catalog, and Codex App Server initialization are valid"
        );
        return Ok(());
    }
    let media_dir = tempfile::Builder::new()
        .prefix("im-codex-bridge-media-")
        .tempdir()
        .context("create private inbound media directory")?;
    let media_root = media_dir
        .path()
        .canonicalize()
        .context("resolve private inbound media directory")?;
    let server_requests = codex.take_server_requests().await?;
    let service = Arc::new(BridgeService::new(
        config.clone(),
        config_path,
        codex,
        state,
        media_root.clone(),
    ));
    tokio::spawn(service.clone().run_server_requests(server_requests));

    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let mut writers = HashMap::new();
    let mut processes = Vec::new();
    for adapter in config.adapters.iter().filter(|adapter| adapter.enabled) {
        let (writer, process) = spawn_adapter(adapter, &media_root, events_tx.clone()).await?;
        writers.insert(adapter.name.clone(), writer);
        processes.push(process);
    }
    if writers.is_empty() {
        bail!("no enabled adapters configured");
    }

    info!("bridge is ready");
    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal.context("listen for shutdown signal")?;
                info!("shutdown requested");
                break;
            }
            event = events_rx.recv() => {
                let Some(event) = event else { break };
                match &event {
                    AdapterEvent::Ready { platform } => info!(platform, "adapter ready"),
                    AdapterEvent::Error { platform, message } => warn!(platform, message, "adapter error"),
                    AdapterEvent::Message { platform, .. } => {
                        let Some(replies) = writers.get(platform).cloned() else {
                            warn!(platform, "no writer for adapter event");
                            continue;
                        };
                        let service = service.clone();
                        tokio::spawn(async move { service.handle(event, replies).await; });
                    }
                }
            }
        }
    }

    writers.clear();
    for process in &mut processes {
        match timeout(Duration::from_secs(5), process.child.wait()).await {
            Ok(Ok(status)) => info!(adapter = process.name, %status, "adapter stopped"),
            Ok(Err(error)) => warn!(adapter = process.name, %error, "adapter wait failed"),
            Err(_) => {
                warn!(
                    adapter = process.name,
                    "adapter did not stop gracefully; terminating"
                );
                let _ = process.child.start_kill();
            }
        }
    }
    Ok(())
}

fn parse_cli() -> Result<Cli> {
    let mut args = std::env::args().skip(1);
    let mut path = std::env::var_os("BRIDGE_CONFIG").map(PathBuf::from);
    let mut check = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => path = args.next().map(PathBuf::from),
            "--check" => check = true,
            "--help" | "-h" => {
                println!("Usage: bridge-daemon [--config ~/.codex-bridge/bridge.toml] [--check]");
                std::process::exit(0);
            }
            _ => bail!("unknown argument: {arg}"),
        }
    }
    let config_path = match path {
        Some(path) => path,
        None => PathBuf::from(
            std::env::var_os("HOME").context("HOME is not set; pass --config explicitly")?,
        )
        .join(".codex-bridge/bridge.toml"),
    };
    Ok(Cli { config_path, check })
}

async fn spawn_adapter(
    config: &AdapterConfig,
    media_root: &Path,
    events: mpsc::UnboundedSender<AdapterEvent>,
) -> Result<(mpsc::UnboundedSender<AdapterCommand>, AdapterProcess)> {
    let mut command = Command::new(&config.command);
    command.args(&config.args);
    if let Some(cwd) = &config.cwd {
        command.current_dir(cwd);
    }
    let mut child = command
        .env("IM_CODEX_BRIDGE_MEDIA_ROOT", media_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("start {} adapter", config.name))?;
    let mut stdin = child.stdin.take().context("adapter stdin unavailable")?;
    let stdout = child.stdout.take().context("adapter stdout unavailable")?;
    let stderr = child.stderr.take().context("adapter stderr unavailable")?;
    let (commands_tx, mut commands_rx) = mpsc::unbounded_channel::<AdapterCommand>();
    let writer_name = config.name.clone();
    tokio::spawn(async move {
        while let Some(command) = commands_rx.recv().await {
            let mut line = match serde_json::to_vec(&command) {
                Ok(line) => line,
                Err(error) => {
                    error!(adapter = writer_name, %error, "serialize adapter command");
                    continue;
                }
            };
            line.push(b'\n');
            if stdin.write_all(&line).await.is_err() || stdin.flush().await.is_err() {
                break;
            }
        }
        let _ = stdin.shutdown().await;
    });

    let reader_name = config.name.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            match serde_json::from_str::<AdapterEvent>(&line) {
                Ok(event) => {
                    if events.send(event).is_err() {
                        break;
                    }
                }
                Err(error) => warn!(adapter = reader_name, %error, "invalid adapter protocol line"),
            }
        }
    });

    let log_name = config.name.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            info!(adapter = log_name, "{line}");
        }
    });

    Ok((
        commands_tx,
        AdapterProcess {
            name: config.name.clone(),
            child,
        },
    ))
}
