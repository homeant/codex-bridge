use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read bridge config {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid bridge config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid path {path}: {source}")]
    Path {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("codex.cwd must be inside codex.allowed_roots")]
    UnsafeCwd,
    #[error("codex.allowed_roots must not be empty")]
    MissingRoots,
    #[error("projects must not be empty")]
    MissingProjects,
    #[error("project id must not be empty")]
    EmptyProjectId,
    #[error("duplicate project id: {0}")]
    DuplicateProjectId(String),
    #[error("adapter name must not be empty")]
    EmptyAdapterName,
    #[error("duplicate adapter name: {0}")]
    DuplicateAdapterName(String),
    #[error("project {id} path must be inside codex.allowed_roots: {path}")]
    UnsafeProjectPath { id: String, path: PathBuf },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeConfig {
    pub codex: CodexConfig,
    pub state: StateConfig,
    pub projects: Vec<ProjectConfig>,
    #[serde(default)]
    pub adapters: Vec<AdapterConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodexConfig {
    #[serde(default = "default_codex_binary")]
    pub binary: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_provider: Option<String>,
    pub cwd: PathBuf,
    pub allowed_roots: Vec<PathBuf>,
    #[serde(default)]
    pub operator_guardrail: String,
    pub project_router_prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StateConfig {
    pub sqlite_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AccessConfig {
    #[serde(default)]
    pub allow_all_users: bool,
    #[serde(default)]
    pub allow_all_groups: bool,
    #[serde(default)]
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub allowed_groups: Vec<String>,
    #[serde(default)]
    pub approval_users: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterConfig {
    pub name: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub access: AccessConfig,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

fn default_codex_binary() -> String {
    "codex".into()
}
fn enabled() -> bool {
    true
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

impl BridgeConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        let mut config: Self = toml::from_str(&source)?;
        config.resolve_paths(path.parent().unwrap_or_else(|| Path::new(".")))?;
        Ok(config)
    }

    fn resolve_paths(&mut self, config_dir: &Path) -> Result<(), ConfigError> {
        if self.codex.allowed_roots.is_empty() {
            return Err(ConfigError::MissingRoots);
        }
        if self.projects.is_empty() {
            return Err(ConfigError::MissingProjects);
        }
        self.codex.model = normalized_optional(self.codex.model.take());
        self.codex.model_provider = normalized_optional(self.codex.model_provider.take());
        let mut adapter_names = HashSet::with_capacity(self.adapters.len());
        for adapter in &mut self.adapters {
            adapter.name = adapter.name.trim().to_owned();
            if adapter.name.is_empty() {
                return Err(ConfigError::EmptyAdapterName);
            }
            if !adapter_names.insert(adapter.name.clone()) {
                return Err(ConfigError::DuplicateAdapterName(adapter.name.clone()));
            }
        }
        self.codex.cwd = canonical(&expand_home(&self.codex.cwd))?;
        self.codex.allowed_roots = self
            .codex
            .allowed_roots
            .iter()
            .map(|path| canonical(&expand_home(path)))
            .collect::<Result<_, _>>()?;
        if !self
            .codex
            .allowed_roots
            .iter()
            .any(|root| self.codex.cwd.starts_with(root))
        {
            return Err(ConfigError::UnsafeCwd);
        }
        let mut project_ids = HashSet::with_capacity(self.projects.len());
        for project in &mut self.projects {
            project.id = project.id.trim().to_owned();
            if project.id.is_empty() {
                return Err(ConfigError::EmptyProjectId);
            }
            if !project_ids.insert(project.id.clone()) {
                return Err(ConfigError::DuplicateProjectId(project.id.clone()));
            }
            project.path = canonical(&expand_home(&project.path))?;
            if !self
                .codex
                .allowed_roots
                .iter()
                .any(|root| project.path.starts_with(root))
            {
                return Err(ConfigError::UnsafeProjectPath {
                    id: project.id.clone(),
                    path: project.path.clone(),
                });
            }
        }
        self.state.sqlite_path = expand_home(&self.state.sqlite_path);
        if self.state.sqlite_path.is_relative() {
            self.state.sqlite_path = config_dir.join(&self.state.sqlite_path);
        }
        for adapter in &mut self.adapters {
            if let Some(cwd) = &adapter.cwd {
                adapter.cwd = Some(canonical(&expand_home(cwd))?);
            }
        }
        Ok(())
    }

    pub fn access_for_adapter(&self, adapter_name: &str) -> Option<&AccessConfig> {
        self.adapters
            .iter()
            .find(|adapter| adapter.enabled && adapter.name == adapter_name)
            .map(|adapter| &adapter.access)
    }
}

fn expand_home(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    let Some(home) = env::var_os("HOME") else {
        return path.to_owned();
    };
    if raw == "$HOME" || raw == "~" {
        return PathBuf::from(home);
    }
    for prefix in ["$HOME/", "~/"] {
        if let Some(rest) = raw.strip_prefix(prefix) {
            return PathBuf::from(&home).join(rest);
        }
    }
    path.to_owned()
}

fn canonical(path: &Path) -> Result<PathBuf, ConfigError> {
    path.canonicalize().map_err(|source| ConfigError::Path {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_state_path_is_resolved_from_config_directory() {
        let root = tempfile::tempdir().unwrap();
        let adapter_cwd = root.path().join("adapter");
        fs::create_dir(&adapter_cwd).unwrap();
        let config_dir = root.path().join("configuration");
        fs::create_dir(&config_dir).unwrap();

        let mut config = BridgeConfig {
            codex: CodexConfig {
                binary: "codex".into(),
                model: None,
                model_provider: None,
                cwd: root.path().to_owned(),
                allowed_roots: vec![root.path().to_owned()],
                operator_guardrail: String::new(),
                project_router_prompt: "Select the matching project.".into(),
            },
            state: StateConfig {
                sqlite_path: "bridge.sqlite3".into(),
            },
            projects: vec![ProjectConfig {
                id: "bridge".into(),
                name: "Bridge".into(),
                description: "IM bridge".into(),
                path: root.path().to_owned(),
            }],
            adapters: vec![AdapterConfig {
                name: "test".into(),
                enabled: true,
                access: AccessConfig::default(),
                command: "node".into(),
                args: Vec::new(),
                cwd: Some(adapter_cwd.clone()),
            }],
        };

        config.resolve_paths(&config_dir).unwrap();

        assert_eq!(config.state.sqlite_path, config_dir.join("bridge.sqlite3"));
        assert_eq!(
            config.adapters[0].cwd.as_deref(),
            Some(adapter_cwd.canonicalize().unwrap().as_path())
        );
    }

    #[test]
    fn rejects_duplicate_project_ids() {
        let root = tempfile::tempdir().unwrap();
        let mut config = BridgeConfig {
            codex: CodexConfig {
                binary: "codex".into(),
                model: None,
                model_provider: None,
                cwd: root.path().to_owned(),
                allowed_roots: vec![root.path().to_owned()],
                operator_guardrail: String::new(),
                project_router_prompt: "Select the matching project.".into(),
            },
            state: StateConfig {
                sqlite_path: root.path().join("bridge.sqlite3"),
            },
            projects: vec![
                ProjectConfig {
                    id: "same".into(),
                    name: "One".into(),
                    description: "One".into(),
                    path: root.path().to_owned(),
                },
                ProjectConfig {
                    id: "same".into(),
                    name: "Two".into(),
                    description: "Two".into(),
                    path: root.path().to_owned(),
                },
            ],
            adapters: vec![],
        };

        assert!(matches!(
            config.resolve_paths(root.path()),
            Err(ConfigError::DuplicateProjectId(id)) if id == "same"
        ));
    }

    #[test]
    fn adapter_access_is_scoped_by_enabled_adapter_name() {
        let config = BridgeConfig {
            codex: CodexConfig {
                binary: "codex".into(),
                model: None,
                model_provider: None,
                cwd: PathBuf::from("/workspace"),
                allowed_roots: vec![PathBuf::from("/workspace")],
                operator_guardrail: String::new(),
                project_router_prompt: String::new(),
            },
            state: StateConfig {
                sqlite_path: PathBuf::from("/tmp/bridge.sqlite3"),
            },
            projects: vec![ProjectConfig {
                id: "bridge".into(),
                name: "Bridge".into(),
                description: "IM bridge".into(),
                path: PathBuf::from("/workspace/bridge"),
            }],
            adapters: vec![
                AdapterConfig {
                    name: "wecom".into(),
                    enabled: true,
                    access: AccessConfig {
                        allowed_users: vec!["wecom-user".into()],
                        ..AccessConfig::default()
                    },
                    command: "node".into(),
                    args: Vec::new(),
                    cwd: None,
                },
                AdapterConfig {
                    name: "lark".into(),
                    enabled: false,
                    access: AccessConfig {
                        allowed_users: vec!["lark-user".into()],
                        ..AccessConfig::default()
                    },
                    command: "node".into(),
                    args: Vec::new(),
                    cwd: None,
                },
            ],
        };

        assert_eq!(
            config.access_for_adapter("wecom").unwrap().allowed_users,
            ["wecom-user"]
        );
        assert!(config.access_for_adapter("lark").is_none());
        assert!(config.access_for_adapter("unknown").is_none());
    }

    #[test]
    fn rejects_legacy_global_access_table() {
        let source = r#"
            [codex]
            cwd = "/workspace"
            allowed_roots = ["/workspace"]
            project_router_prompt = "route"

            [[projects]]
            id = "bridge"
            name = "Bridge"
            description = "IM bridge"
            path = "/workspace/bridge"

            [state]
            sqlite_path = "bridge.sqlite3"

            [access]
            allowed_users = ["legacy-user"]
        "#;

        let error = toml::from_str::<BridgeConfig>(source).unwrap_err();
        assert!(error.to_string().contains("unknown field `access`"));
    }
}
