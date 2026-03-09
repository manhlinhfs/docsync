use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::config::AppPaths;
use crate::discovery::discover_source;
use crate::fetch::fetch_snapshot_pages;
use crate::git_sync::sync_git_source;
use crate::headless::resolve_browser_cmd;
use crate::incremental::{find_previous_snapshot, previous_page_index};
use crate::models::{
    AppConfig, DiffSummary, PageChangeStatus, SnapshotManifest, SourceDefinition, SourceKind,
};
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
    pub previous_snapshot_label: Option<String>,
    pub discovered_pages: usize,
    pub fetched_pages: usize,
    pub skipped_pages: usize,
    pub reused_pages: usize,
    pub changed_pages: usize,
    pub unchanged_pages: usize,
    pub removed_pages: usize,
    pub strategy_summary: String,
}

pub fn sync_source(
    config: &AppConfig,
    paths: &AppPaths,
    name: &str,
    reference: Option<String>,
    dry_run: bool,
    cli_proxy: Option<&str>,
    cli_browser_cmd: Option<&str>,
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
    let browser_cmd = resolve_browser_cmd(cli_browser_cmd, Some(config), Some(source));
    let previous_snapshot = find_previous_snapshot(paths, &source.name, &snapshot_label)?;
    let previous_page_index = previous_snapshot
        .as_ref()
        .map(previous_page_index)
        .transpose()?;

    match source.source_kind {
        SourceKind::GitDocs => sync_git_mode(
            source,
            &raw_label,
            &snapshot_label,
            &snapshot_dir,
            &manifest_path,
            &discovery_manifest_path,
            previous_snapshot
                .as_ref()
                .map(|snapshot| snapshot.label.as_str()),
            previous_page_index.as_ref(),
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
            previous_snapshot
                .as_ref()
                .map(|snapshot| snapshot.label.as_str()),
            previous_page_index.as_ref(),
            dry_run,
            proxy_url.as_deref(),
            browser_cmd.as_deref(),
        ),
    }
}

fn sync_http_mode(
    source: &SourceDefinition,
    raw_label: &str,
    snapshot_label: &str,
    snapshot_dir: &PathBuf,
    manifest_path: &PathBuf,
    discovery_manifest_path: &PathBuf,
    previous_snapshot_label: Option<&str>,
    previous_page_index: Option<
        &std::collections::BTreeMap<String, crate::incremental::PreviousPageState>,
    >,
    dry_run: bool,
    proxy_url: Option<&str>,
    browser_cmd: Option<&str>,
) -> Result<SyncResult> {
    let discovery = discover_source(&source.entry_url, &source.name, raw_label, proxy_url)?;
    let discovery_summary = discovery.summary(discovery_manifest_path.clone());
    let strategy_summary = format!(
        "kind={} version_strategy={} discovery={} fetch=http",
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
    let mut reused_pages = 0usize;
    let mut diff = diff_summary_from_pages(&[], previous_snapshot_label);

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
            previous_page_index,
            proxy_url,
            browser_cmd,
        )?;
        fetched_pages = fetch_outcome.summary.stored_pages;
        skipped_pages = fetch_outcome.summary.skipped_pages;
        reused_pages = fetch_outcome.summary.reused_pages;
        diff = diff_summary_from_pages(&fetch_outcome.pages, previous_snapshot_label);

        let manifest = SnapshotManifest {
            schema_version: 6,
            created_at: now_utc_rfc3339(),
            source_name: source.name.clone(),
            entry_url: source.entry_url.clone(),
            source_kind: source.source_kind,
            version_strategy: source.version_strategy,
            source_ref: raw_label.to_string(),
            snapshot_label: snapshot_label.to_string(),
            snapshot_dir: snapshot_dir.clone(),
            status: if fetched_pages > 0 {
                "fetched".to_string()
            } else if discovery_summary.frontier_count > 0 {
                "discovered".to_string()
            } else {
                "scaffolded".to_string()
            },
            previous_snapshot_label: previous_snapshot_label.map(ToOwned::to_owned),
            detected_input_kind: discovery.detected_input_kind,
            suggested_mode: discovery.suggested_mode,
            discovery: discovery_summary,
            git: None,
            fetch: Some(fetch_outcome.summary),
            diff: Some(diff.clone()),
            pages: fetch_outcome.pages,
            notes: {
                let mut notes = discovery.notes.clone();
                if previous_snapshot_label.is_some() {
                    notes.push(
                        "Snapshot diffing classified pages as new, changed, unchanged, or removed."
                            .to_string(),
                    );
                }
                if browser_cmd.is_some() {
                    notes.push(
                        "Headless browser fallback is available for dynamic HTML pages when static extraction is too thin."
                            .to_string(),
                    );
                }
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
        previous_snapshot_label: previous_snapshot_label.map(ToOwned::to_owned),
        discovered_pages: discovery.frontier.len(),
        fetched_pages,
        skipped_pages,
        reused_pages,
        changed_pages: diff.changed_pages + diff.new_pages,
        unchanged_pages: diff.unchanged_pages,
        removed_pages: diff.removed_pages,
        strategy_summary,
    })
}

fn sync_git_mode(
    source: &SourceDefinition,
    raw_label: &str,
    snapshot_label: &str,
    snapshot_dir: &PathBuf,
    manifest_path: &PathBuf,
    discovery_manifest_path: &PathBuf,
    previous_snapshot_label: Option<&str>,
    previous_page_index: Option<
        &std::collections::BTreeMap<String, crate::incremental::PreviousPageState>,
    >,
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
        previous_snapshot_label,
        previous_page_index,
        proxy_url,
    )?;
    let discovery_summary = outcome.discovery.summary(discovery_manifest_path.clone());
    let diff = diff_summary_from_pages(&outcome.pages, previous_snapshot_label);
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
            schema_version: 6,
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
            previous_snapshot_label: previous_snapshot_label.map(ToOwned::to_owned),
            detected_input_kind: outcome.discovery.detected_input_kind,
            suggested_mode: outcome.discovery.suggested_mode,
            discovery: discovery_summary,
            git: Some(outcome.git),
            fetch: Some(outcome.fetch.clone()),
            diff: Some(diff.clone()),
            pages: outcome.pages,
            notes: vec![
                "Git-native sync copied markdown files directly from the source repository."
                    .to_string(),
                "Incremental git sync classifies docs pages by content hash and relative path."
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
        previous_snapshot_label: previous_snapshot_label.map(ToOwned::to_owned),
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
        reused_pages: if dry_run {
            0
        } else {
            outcome.fetch.reused_pages
        },
        changed_pages: diff.changed_pages + diff.new_pages,
        unchanged_pages: diff.unchanged_pages,
        removed_pages: diff.removed_pages,
        strategy_summary,
    })
}

fn diff_summary_from_pages(
    pages: &[crate::models::PageManifestEntry],
    previous_snapshot_label: Option<&str>,
) -> DiffSummary {
    let mut new_pages = 0usize;
    let mut changed_pages = 0usize;
    let mut unchanged_pages = 0usize;
    let mut removed_pages = 0usize;

    for page in pages {
        match page.change_status {
            PageChangeStatus::New => new_pages += 1,
            PageChangeStatus::Changed => changed_pages += 1,
            PageChangeStatus::Unchanged => unchanged_pages += 1,
            PageChangeStatus::Removed => removed_pages += 1,
            PageChangeStatus::Unknown => {}
        }
    }

    DiffSummary {
        previous_snapshot_label: previous_snapshot_label.map(ToOwned::to_owned),
        new_pages,
        changed_pages,
        unchanged_pages,
        removed_pages,
        import_candidates: new_pages + changed_pages,
    }
}
