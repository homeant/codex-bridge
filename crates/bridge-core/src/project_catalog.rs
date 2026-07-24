use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, RwLock},
};

use thiserror::Error;

use crate::ProjectConfig;

#[derive(Debug, Error)]
pub enum ProjectCatalogError {
    #[error("project id must not be empty")]
    EmptyId,
    #[error("project name must not be empty")]
    EmptyName,
    #[error("project already exists: {0}")]
    DuplicateId(String),
    #[error("unknown project: {0}")]
    UnknownProject(String),
    #[error("at least one project must remain configured")]
    LastProject,
    #[error("update_project requires at least one changed field")]
    EmptyUpdate,
    #[error("invalid project path {path}: {source}")]
    InvalidPath {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("project {id} path must be inside codex.allowed_roots: {path}")]
    UnsafePath { id: String, path: PathBuf },
    #[error("failed to read bridge config {path}: {source}")]
    ReadConfig {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse bridge config: {0}")]
    ParseConfig(#[source] toml::de::Error),
    #[error("failed to serialize bridge config: {0}")]
    SerializeConfig(#[source] toml::ser::Error),
    #[error("failed to persist bridge config {path}: {source}")]
    WriteConfig {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub struct ProjectCatalog {
    config_path: PathBuf,
    allowed_roots: Vec<PathBuf>,
    projects: RwLock<Vec<ProjectConfig>>,
    mutation_lock: Mutex<()>,
}

impl ProjectCatalog {
    pub fn new(
        config_path: PathBuf,
        allowed_roots: Vec<PathBuf>,
        projects: Vec<ProjectConfig>,
    ) -> Self {
        Self {
            config_path,
            allowed_roots,
            projects: RwLock::new(projects),
            mutation_lock: Mutex::new(()),
        }
    }

    pub fn list(&self) -> Vec<ProjectConfig> {
        self.projects
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn get(&self, project_id: &str) -> Option<ProjectConfig> {
        self.projects
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .find(|project| project.id == project_id)
            .cloned()
    }

    pub fn add(
        &self,
        id: String,
        name: String,
        description: String,
        path: PathBuf,
    ) -> Result<ProjectConfig, ProjectCatalogError> {
        let _mutation = self
            .mutation_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut projects = self.list();
        let project = self.validate_project(id, name, description, path)?;
        if projects.iter().any(|existing| existing.id == project.id) {
            return Err(ProjectCatalogError::DuplicateId(project.id));
        }
        projects.push(project.clone());
        self.persist(&projects)?;
        *self
            .projects
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = projects;
        Ok(project)
    }

    pub fn update(
        &self,
        project_id: &str,
        name: Option<String>,
        description: Option<String>,
        path: Option<PathBuf>,
    ) -> Result<ProjectConfig, ProjectCatalogError> {
        if name.is_none() && description.is_none() && path.is_none() {
            return Err(ProjectCatalogError::EmptyUpdate);
        }
        let _mutation = self
            .mutation_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut projects = self.list();
        let Some(index) = projects.iter().position(|project| project.id == project_id) else {
            return Err(ProjectCatalogError::UnknownProject(project_id.into()));
        };
        let current = &projects[index];
        let updated = self.validate_project(
            current.id.clone(),
            name.unwrap_or_else(|| current.name.clone()),
            description.unwrap_or_else(|| current.description.clone()),
            path.unwrap_or_else(|| current.path.clone()),
        )?;
        projects[index] = updated.clone();
        self.persist(&projects)?;
        *self
            .projects
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = projects;
        Ok(updated)
    }

    pub fn delete(&self, project_id: &str) -> Result<ProjectConfig, ProjectCatalogError> {
        let _mutation = self
            .mutation_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut projects = self.list();
        let Some(index) = projects.iter().position(|project| project.id == project_id) else {
            return Err(ProjectCatalogError::UnknownProject(project_id.into()));
        };
        if projects.len() == 1 {
            return Err(ProjectCatalogError::LastProject);
        }
        let deleted = projects.remove(index);
        self.persist(&projects)?;
        *self
            .projects
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = projects;
        Ok(deleted)
    }

    fn validate_project(
        &self,
        id: String,
        name: String,
        description: String,
        path: PathBuf,
    ) -> Result<ProjectConfig, ProjectCatalogError> {
        let id = id.trim().to_owned();
        if id.is_empty() {
            return Err(ProjectCatalogError::EmptyId);
        }
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err(ProjectCatalogError::EmptyName);
        }
        let expanded = expand_home(&path);
        let canonical =
            expanded
                .canonicalize()
                .map_err(|source| ProjectCatalogError::InvalidPath {
                    path: expanded,
                    source,
                })?;
        if !self
            .allowed_roots
            .iter()
            .any(|root| canonical.starts_with(root))
        {
            return Err(ProjectCatalogError::UnsafePath {
                id,
                path: canonical,
            });
        }
        Ok(ProjectConfig {
            id,
            name,
            description: description.trim().to_owned(),
            path: canonical,
        })
    }

    fn persist(&self, projects: &[ProjectConfig]) -> Result<(), ProjectCatalogError> {
        let source = fs::read_to_string(&self.config_path).map_err(|source| {
            ProjectCatalogError::ReadConfig {
                path: self.config_path.clone(),
                source,
            }
        })?;
        let mut document =
            toml::from_str::<toml::Table>(&source).map_err(ProjectCatalogError::ParseConfig)?;
        document.insert(
            "projects".into(),
            toml::Value::Array(projects.iter().map(project_value).collect()),
        );
        let encoded =
            toml::to_string_pretty(&document).map_err(ProjectCatalogError::SerializeConfig)?;
        atomic_write(&self.config_path, encoded.as_bytes())
    }
}

fn project_value(project: &ProjectConfig) -> toml::Value {
    let mut table = toml::Table::new();
    table.insert("id".into(), toml::Value::String(project.id.clone()));
    table.insert("name".into(), toml::Value::String(project.name.clone()));
    table.insert(
        "description".into(),
        toml::Value::String(project.description.clone()),
    );
    table.insert(
        "path".into(),
        toml::Value::String(project.path.to_string_lossy().into_owned()),
    );
    toml::Value::Table(table)
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), ProjectCatalogError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("bridge.toml");
    let temp_path = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let mode = fs::metadata(path)
                .map(|metadata| metadata.permissions().mode())
                .unwrap_or(0o600);
            options.mode(mode);
        }
        let mut file =
            options
                .open(&temp_path)
                .map_err(|source| ProjectCatalogError::WriteConfig {
                    path: path.to_owned(),
                    source,
                })?;
        file.write_all(contents)
            .and_then(|_| file.sync_all())
            .map_err(|source| ProjectCatalogError::WriteConfig {
                path: path.to_owned(),
                source,
            })?;
        fs::rename(&temp_path, path).map_err(|source| ProjectCatalogError::WriteConfig {
            path: path.to_owned(),
            source,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp_path);
    }
    result
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

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(root: &Path) -> ProjectCatalog {
        let first = root.join("first");
        fs::create_dir(&first).unwrap();
        let config_path = root.join("bridge.toml");
        fs::write(
            &config_path,
            format!(
                r#"
[codex]
cwd = "{}"
allowed_roots = ["{}"]
project_router_prompt = "route"

[[projects]]
id = "first"
name = "First"
description = "first project"
path = "{}"

[state]
sqlite_path = "bridge.sqlite3"
"#,
                root.display(),
                root.display(),
                first.display()
            ),
        )
        .unwrap();
        ProjectCatalog::new(
            config_path,
            vec![root.canonicalize().unwrap()],
            vec![ProjectConfig {
                id: "first".into(),
                name: "First".into(),
                description: "first project".into(),
                path: first.canonicalize().unwrap(),
            }],
        )
    }

    #[test]
    fn adds_updates_and_deletes_projects_in_memory_and_on_disk() {
        let root = tempfile::tempdir().unwrap();
        let catalog = catalog(root.path());
        let second = root.path().join("second");
        fs::create_dir(&second).unwrap();

        let added = catalog
            .add(
                "second".into(),
                "Second".into(),
                "second project".into(),
                second.clone(),
            )
            .unwrap();
        assert_eq!(added.id, "second");
        assert_eq!(catalog.list().len(), 2);

        let updated = catalog
            .update("second", Some("Second renamed".into()), None, None)
            .unwrap();
        assert_eq!(updated.name, "Second renamed");

        let deleted = catalog.delete("first").unwrap();
        assert_eq!(deleted.id, "first");
        assert_eq!(catalog.list().len(), 1);
        assert_eq!(catalog.list()[0].id, updated.id);
        assert_eq!(catalog.list()[0].name, updated.name);

        let persisted = fs::read_to_string(root.path().join("bridge.toml")).unwrap();
        let document: toml::Value = toml::from_str(&persisted).unwrap();
        let projects = document["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["id"].as_str(), Some("second"));
        assert_eq!(projects[0]["name"].as_str(), Some("Second renamed"));
    }

    #[test]
    fn rejects_paths_outside_allowed_roots() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let catalog = catalog(root.path());

        assert!(matches!(
            catalog.add(
                "outside".into(),
                "Outside".into(),
                String::new(),
                outside.path().to_owned(),
            ),
            Err(ProjectCatalogError::UnsafePath { .. })
        ));
    }

    #[test]
    fn refuses_to_delete_the_last_project() {
        let root = tempfile::tempdir().unwrap();
        let catalog = catalog(root.path());

        assert!(matches!(
            catalog.delete("first"),
            Err(ProjectCatalogError::LastProject)
        ));
    }

    #[test]
    fn failed_persistence_does_not_change_the_runtime_catalog() {
        let root = tempfile::tempdir().unwrap();
        let catalog = catalog(root.path());
        let second = root.path().join("second");
        fs::create_dir(&second).unwrap();
        fs::remove_file(root.path().join("bridge.toml")).unwrap();

        assert!(
            catalog
                .add("second".into(), "Second".into(), String::new(), second,)
                .is_err()
        );
        assert_eq!(catalog.list().len(), 1);
        assert_eq!(catalog.list()[0].id, "first");
    }
}
