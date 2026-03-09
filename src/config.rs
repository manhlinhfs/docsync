use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::models::AppConfig;

#[derive(Debug, Clone, Serialize)]
pub struct AppPaths {
    pub home: PathBuf,
    pub config_file: PathBuf,
    pub sources_dir: PathBuf,
    pub snapshots_dir: PathBuf,
}

pub fn resolve_paths(home_override: Option<&Path>) -> Result<AppPaths> {
    let home = match home_override {
        Some(path) => path.to_path_buf(),
        None => resolve_default_home()?,
    };

    Ok(AppPaths {
        config_file: home.join("config.json"),
        sources_dir: home.join("sources"),
        snapshots_dir: home.join("snapshots"),
        home,
    })
}

pub fn ensure_layout(paths: &AppPaths) -> Result<()> {
    fs::create_dir_all(&paths.home)
        .with_context(|| format!("failed to create {}", paths.home.display()))?;
    fs::create_dir_all(&paths.sources_dir)
        .with_context(|| format!("failed to create {}", paths.sources_dir.display()))?;
    fs::create_dir_all(&paths.snapshots_dir)
        .with_context(|| format!("failed to create {}", paths.snapshots_dir.display()))?;

    if !paths.config_file.exists() {
        let config = AppConfig::default();
        config.save(paths)?;
    }

    Ok(())
}

pub fn load_config(paths: &AppPaths) -> Result<AppConfig> {
    if !paths.config_file.exists() {
        return Ok(AppConfig::default());
    }

    let content = fs::read_to_string(&paths.config_file)
        .with_context(|| format!("failed to read {}", paths.config_file.display()))?;
    let config = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", paths.config_file.display()))?;
    Ok(config)
}

fn resolve_default_home() -> Result<PathBuf> {
    if let Some(path) = env::var_os("DOCSYNC_HOME") {
        return Ok(PathBuf::from(path));
    }

    let home = env::var_os("HOME").context("HOME is not set and DOCSYNC_HOME was not provided")?;
    Ok(PathBuf::from(home).join(".docsync"))
}
