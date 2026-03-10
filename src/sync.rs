use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::chunking::ChunkingConfig;
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
use crate::omnimem::{ImportResult, import_snapshot};
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
    pub chunk_count: usize,
    pub strategy_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import: Option<ImportResult>,
}

pub fn sync_source(
    config: &AppConfig,
    paths: &AppPaths,
    name: &str,
    reference: Option<String>,
    dry_run: bool,
    cli_proxy: Option<&str>,
    cli_browser_cmd: Option<&str>,
    import_after_sync: bool,
    cli_omnimem_cmd: Option<String>,
    cli_omnimem_direct: bool,
    cli_omnimem_include_low_signal: bool,
    chunking: ChunkingConfig,
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
            paths,
            import_after_sync,
            cli_omnimem_cmd,
            cli_omnimem_direct,
            cli_omnimem_include_low_signal,
            chunking,
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
            paths,
            import_after_sync,
            cli_omnimem_cmd,
            cli_omnimem_direct,
            cli_omnimem_include_low_signal,
            chunking,
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
    paths: &AppPaths,
    import_after_sync: bool,
    cli_omnimem_cmd: Option<String>,
    cli_omnimem_direct: bool,
    cli_omnimem_include_low_signal: bool,
    chunking: ChunkingConfig,
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
    let mut chunk_count = 0usize;
    let mut import = None;

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
            chunking,
        )?;
        fetched_pages = fetch_outcome.summary.stored_pages;
        skipped_pages = fetch_outcome.summary.skipped_pages;
        reused_pages = fetch_outcome.summary.reused_pages;
        chunk_count = fetch_outcome
            .summary
            .chunking
            .as_ref()
            .map(|value| value.chunk_count)
            .unwrap_or(0);
        diff = diff_summary_from_pages(&fetch_outcome.pages, previous_snapshot_label);

        let manifest = SnapshotManifest {
            schema_version: 7,
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

        import = maybe_auto_import(
            paths,
            source,
            snapshot_label,
            import_after_sync,
            cli_omnimem_cmd,
            cli_omnimem_direct,
            cli_omnimem_include_low_signal,
        )?;
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
        chunk_count,
        strategy_summary,
        import,
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
    paths: &AppPaths,
    import_after_sync: bool,
    cli_omnimem_cmd: Option<String>,
    cli_omnimem_direct: bool,
    cli_omnimem_include_low_signal: bool,
    chunking: ChunkingConfig,
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
        chunking,
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
            schema_version: 7,
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

    let import = if dry_run {
        None
    } else {
        maybe_auto_import(
            paths,
            source,
            snapshot_label,
            import_after_sync,
            cli_omnimem_cmd,
            cli_omnimem_direct,
            cli_omnimem_include_low_signal,
        )?
    };
    let chunk_count = if dry_run {
        0
    } else {
        outcome
            .fetch
            .chunking
            .as_ref()
            .map(|value| value.chunk_count)
            .unwrap_or(0)
    };

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
        chunk_count,
        strategy_summary,
        import,
    })
}

fn maybe_auto_import(
    paths: &AppPaths,
    source: &SourceDefinition,
    snapshot_label: &str,
    import_after_sync: bool,
    cli_omnimem_cmd: Option<String>,
    cli_omnimem_direct: bool,
    cli_omnimem_include_low_signal: bool,
) -> Result<Option<ImportResult>> {
    if !import_after_sync && !source.auto_import {
        return Ok(None);
    }

    let omnimem_cmd = cli_omnimem_cmd.or_else(|| source.omnimem_cmd.clone());
    let omnimem_direct = cli_omnimem_direct || source.omnimem_direct;
    let include_low_signal = cli_omnimem_include_low_signal || source.omnimem_include_low_signal;

    let result = import_snapshot(
        paths,
        &source.name,
        Some(snapshot_label.to_string()),
        omnimem_cmd,
        omnimem_direct,
        false,
        false,
        include_low_signal,
    )?;
    Ok(Some(result))
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

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Result;

    use super::maybe_auto_import;
    use crate::config::AppPaths;
    use crate::models::{
        DiscoverySummary, SnapshotManifest, SourceDefinition, SourceKind, VersionStrategy,
    };
    use crate::probe::{DetectedInputKind, SuggestedMode};

    #[test]
    fn auto_import_runs_when_enabled_on_source() -> Result<()> {
        let root = temp_dir("sync-auto-import-source");
        let paths = make_paths(&root);
        create_snapshot_fixture(&paths, "demo", "snap-1")?;
        let fake = fake_omnimem_script(&root)?;
        let source = source_definition(true, Some(fake.to_string_lossy().to_string()), false);

        let result = maybe_auto_import(&paths, &source, "snap-1", false, None, false, false)?;

        let result = result.expect("auto import result");
        assert_eq!(result.imported_pages, 1);
        let log = fs::read_to_string(root.join("omnimem-invocations.log"))?;
        assert!(log.contains("import"));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn explicit_sync_import_overrides_source_policy() -> Result<()> {
        let root = temp_dir("sync-auto-import-cli");
        let paths = make_paths(&root);
        create_snapshot_fixture(&paths, "demo", "snap-1")?;
        let fake = fake_omnimem_script(&root)?;
        let source = source_definition(false, Some(fake.to_string_lossy().to_string()), true);

        let result = maybe_auto_import(&paths, &source, "snap-1", true, None, false, false)?;

        let result = result.expect("explicit import result");
        assert_eq!(result.imported_pages, 1);
        let log = fs::read_to_string(root.join("omnimem-invocations.log"))?;
        assert!(log.contains("--direct"));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    fn create_snapshot_fixture(
        paths: &AppPaths,
        source_name: &str,
        snapshot_label: &str,
    ) -> Result<PathBuf> {
        let snapshot_dir = paths.snapshots_dir.join(source_name).join(snapshot_label);
        fs::create_dir_all(snapshot_dir.join("pages"))?;
        let page_path = snapshot_dir.join("pages/intro.md");
        fs::write(&page_path, "# Intro\n")?;

        let manifest = SnapshotManifest {
            schema_version: 7,
            created_at: "2026-03-09T00:00:00Z".to_string(),
            source_name: source_name.to_string(),
            entry_url: "https://docs.example.com".to_string(),
            source_kind: SourceKind::Website,
            version_strategy: VersionStrategy::DateSnapshot,
            source_ref: snapshot_label.to_string(),
            snapshot_label: snapshot_label.to_string(),
            snapshot_dir: snapshot_dir.clone(),
            status: "fetched".to_string(),
            previous_snapshot_label: None,
            detected_input_kind: DetectedInputKind::ContentPage,
            suggested_mode: SuggestedMode::HybridSeed,
            discovery: DiscoverySummary {
                manifest_path: snapshot_dir.join("discovery.json"),
                adapters: vec!["seed_page".to_string()],
                frontier_count: 1,
                llms_index_url: None,
                llms_full_index_url: None,
                sitemap_count: 0,
            },
            git: None,
            fetch: None,
            diff: None,
            pages: vec![crate::models::PageManifestEntry {
                page_key: "https://docs.example.com/intro".to_string(),
                url: "https://docs.example.com/intro".to_string(),
                final_url: "https://docs.example.com/intro".to_string(),
                fetch_method: "markdown_negotiation".to_string(),
                status: "stored".to_string(),
                change_status: crate::models::PageChangeStatus::New,
                reused_from_snapshot: None,
                page_path: Some(page_path.clone()),
                metadata_path: None,
                raw_path: Some(page_path),
                rendered_raw_path: None,
                content_type: Some("text/markdown".to_string()),
                sha256: None,
                raw_sha256: None,
                etag: None,
                last_modified: None,
                byte_size: 8,
                normalization: None,
                quality: None,
                chunks: Vec::new(),
            }],
            notes: Vec::new(),
        };
        fs::write(
            snapshot_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        Ok(snapshot_dir)
    }

    fn source_definition(
        auto_import: bool,
        omnimem_cmd: Option<String>,
        omnimem_direct: bool,
    ) -> SourceDefinition {
        SourceDefinition {
            name: "demo".to_string(),
            entry_url: "https://docs.example.com".to_string(),
            proxy_url: None,
            browser_cmd: None,
            auto_import,
            omnimem_cmd,
            omnimem_direct,
            omnimem_include_low_signal: false,
            source_kind: SourceKind::Website,
            repo_url: None,
            docs_path: None,
            default_ref: None,
            version_strategy: VersionStrategy::DateSnapshot,
            tags: Vec::new(),
            created_at: "2026-03-09T00:00:00Z".to_string(),
            updated_at: "2026-03-09T00:00:00Z".to_string(),
        }
    }

    fn fake_omnimem_script(root: &Path) -> Result<PathBuf> {
        let script = root.join("fake-omnimem.sh");
        let log = root.join("omnimem-invocations.log");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{}\"\nexit 0\n",
                log.display()
            ),
        )?;
        let mut perms = fs::metadata(&script)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms)?;
        Ok(script)
    }

    fn make_paths(root: &Path) -> AppPaths {
        AppPaths {
            home: root.to_path_buf(),
            config_file: root.join("config.json"),
            sources_dir: root.join("sources"),
            snapshots_dir: root.join("snapshots"),
        }
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("docsync-{prefix}-{stamp}"))
    }
}
