use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::config::AppPaths;
use crate::discovery::discover_source;
use crate::fetch::fetch_snapshot_pages;
use crate::git_sync::sync_git_source;
use crate::models::{AppConfig, SnapshotManifest, SourceKind};
use crate::network::resolve_proxy_url;
use crate::util::{ensure_directory, now_utc_rfc3339, sanitize_ref_label};

#[derive(Debug, Serialize)]
pub struct SyncResult {
    pub source_name: String,
    pub entry_url: String,
    pub dry_run: bool,
    pub snapshot_label: String,
    pub snapshot_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub discovery_manifest_path: PathBuf,
    pub discovered_pages: usize,
    pub fetched_pages: usize,
    pub skipped_pages: usize,
    pub strategy_summary: String,
}

pub fn sync_source(
    config: &AppConfig,
    paths: &AppPaths,
    name: &str,
    reference: Option<String>,
    dry_run: bool,
    cli_proxy: Option<&str>,
) -> Result<SyncResult> {
    let source = config
        .sources
        .get(name)
        .with_context(|| format!("source `{name}` not found"))?;

    let raw_label = reference
        .or_else(|| source.default_ref.clone())
        .unwrap_or_else(|| format!("snapshot-{}", crate::util::today_compact()));
    let snapshot_label = sanitize_ref_label(&raw_label);
    let snapshot_dir = paths.snapshots_dir.join(&source.name).join(&snapshot_label);
    let manifest_path = snapshot_dir.join("manifest.json");
    let discovery_manifest_path = snapshot_dir.join("discovery.json");
    let proxy_url = resolve_proxy_url(cli_proxy, Some(config), Some(source));

    match source.source_kind {
        SourceKind::GitDocs => sync_git_mode(
            source,
            &raw_label,
            &snapshot_label,
            &snapshot_dir,
            &manifest_path,
            &discovery_manifest_path,
            dry_run,
            proxy_url.as_deref(),
        ),
        _ => sync_http_mode(
            source,
            &raw_label,
            &snapshot_label,
            &snapshot_dir,
            &manifest_path,
            &discovery_manifest_path,
            dry_run,
            proxy_url.as_deref(),
        ),
    }
}

fn sync_http_mode(
    source: &crate::models::SourceDefinition,
    raw_label: &str,
    snapshot_label: &str,
    snapshot_dir: &PathBuf,
    manifest_path: &PathBuf,
    discovery_manifest_path: &PathBuf,
    dry_run: bool,
    proxy_url: Option<&str>,
) -> Result<SyncResult> {
    let discovery = discover_source(&source.entry_url, &source.name, raw_label, proxy_url)?;
    let discovery_summary = discovery.summary(discovery_manifest_path.clone());
    let strategy_summary = format!(
        "kind={} version_strategy={} discovery={}",
        source.source_kind,
        source.version_strategy,
        if discovery_summary.adapters.is_empty() {
            "none".to_string()
        } else {
            discovery_summary.adapters.join("+")
        }
    );

    let mut fetched_pages = 0usize;
    let mut skipped_pages = 0usize;

    if !dry_run {
        ensure_directory(snapshot_dir)?;
        ensure_directory(&snapshot_dir.join("pages"))?;
        ensure_directory(&snapshot_dir.join("raw"))?;

        let discovery_body = serde_json::to_string_pretty(&discovery)?;
        fs::write(discovery_manifest_path, discovery_body).with_context(|| {
            format!(
                "failed to write discovery manifest {}",
                discovery_manifest_path.display()
            )
        })?;

        let fetch_outcome = fetch_snapshot_pages(
            snapshot_dir,
            source,
            raw_label,
            snapshot_label,
            &discovery.frontier,
            proxy_url,
        )?;
        fetched_pages = fetch_outcome.summary.stored_pages;
        skipped_pages = fetch_outcome.summary.skipped_pages;

        let manifest = SnapshotManifest {
            schema_version: 4,
            created_at: now_utc_rfc3339(),
            source_name: source.name.clone(),
            entry_url: source.entry_url.clone(),
            source_kind: source.source_kind,
            version_strategy: source.version_strategy,
            source_ref: raw_label.to_string(),
            snapshot_label: snapshot_label.to_string(),
            snapshot_dir: snapshot_dir.clone(),
            status: if fetch_outcome.summary.stored_pages > 0 {
                "fetched".to_string()
            } else if discovery_summary.frontier_count > 0 {
                "discovered".to_string()
            } else {
                "scaffolded".to_string()
            },
            detected_input_kind: discovery.detected_input_kind,
            suggested_mode: discovery.suggested_mode,
            discovery: discovery_summary,
            git: None,
            fetch: Some(fetch_outcome.summary),
            pages: fetch_outcome.pages,
            notes: {
                let mut notes = discovery.notes.clone();
                notes.push(
                    "HTML normalization and OmniMem integration still land in later releases."
                        .to_string(),
                );
                notes
            },
        };

        let body = serde_json::to_string_pretty(&manifest)?;
        fs::write(manifest_path, body)
            .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    }

    Ok(SyncResult {
        source_name: source.name.clone(),
        entry_url: source.entry_url.clone(),
        dry_run,
        snapshot_label: snapshot_label.to_string(),
        snapshot_dir: snapshot_dir.clone(),
        manifest_path: manifest_path.clone(),
        discovery_manifest_path: discovery_manifest_path.clone(),
        discovered_pages: discovery.frontier.len(),
        fetched_pages,
        skipped_pages,
        strategy_summary,
    })
}

fn sync_git_mode(
    source: &crate::models::SourceDefinition,
    raw_label: &str,
    snapshot_label: &str,
    snapshot_dir: &PathBuf,
    manifest_path: &PathBuf,
    discovery_manifest_path: &PathBuf,
    dry_run: bool,
    proxy_url: Option<&str>,
) -> Result<SyncResult> {
    if !dry_run {
        ensure_directory(snapshot_dir)?;
        ensure_directory(&snapshot_dir.join("pages"))?;
        ensure_directory(&snapshot_dir.join("raw"))?;
    }

    let outcome = sync_git_source(
        snapshot_dir,
        source,
        raw_label,
        snapshot_label,
        !dry_run,
        proxy_url,
    )?;
    let discovery_summary = outcome.discovery.summary(discovery_manifest_path.clone());
    let strategy_summary = format!(
        "kind={} version_strategy={} discovery={} fetch=git_checkout",
        source.source_kind,
        source.version_strategy,
        discovery_summary.adapters.join("+")
    );

    if !dry_run {
        let discovery_body = serde_json::to_string_pretty(&outcome.discovery)?;
        fs::write(discovery_manifest_path, discovery_body).with_context(|| {
            format!(
                "failed to write discovery manifest {}",
                discovery_manifest_path.display()
            )
        })?;

        let manifest = SnapshotManifest {
            schema_version: 4,
            created_at: now_utc_rfc3339(),
            source_name: source.name.clone(),
            entry_url: source.entry_url.clone(),
            source_kind: source.source_kind,
            version_strategy: source.version_strategy,
            source_ref: raw_label.to_string(),
            snapshot_label: snapshot_label.to_string(),
            snapshot_dir: snapshot_dir.clone(),
            status: if outcome.fetch.stored_pages > 0 {
                "fetched".to_string()
            } else {
                "scaffolded".to_string()
            },
            detected_input_kind: outcome.discovery.detected_input_kind,
            suggested_mode: outcome.discovery.suggested_mode,
            discovery: discovery_summary,
            git: Some(outcome.git),
            fetch: Some(outcome.fetch.clone()),
            pages: outcome.pages,
            notes: vec![
                "Git-native sync copied markdown files directly from the source repository."
                    .to_string(),
                "HTML normalization and OmniMem integration still land in later releases."
                    .to_string(),
            ],
        };

        let body = serde_json::to_string_pretty(&manifest)?;
        fs::write(manifest_path, body)
            .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    }

    Ok(SyncResult {
        source_name: source.name.clone(),
        entry_url: source.entry_url.clone(),
        dry_run,
        snapshot_label: snapshot_label.to_string(),
        snapshot_dir: snapshot_dir.clone(),
        manifest_path: manifest_path.clone(),
        discovery_manifest_path: discovery_manifest_path.clone(),
        discovered_pages: outcome.discovery.frontier.len(),
        fetched_pages: if dry_run {
            0
        } else {
            outcome.fetch.stored_pages
        },
        skipped_pages: if dry_run {
            0
        } else {
            outcome.fetch.skipped_pages
        },
        strategy_summary,
    })
}
