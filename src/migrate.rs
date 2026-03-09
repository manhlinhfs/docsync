use std::fs;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::config::{AppPaths, load_config};
use crate::models::{DiffSummary, PageChangeStatus, SnapshotManifest};

#[derive(Debug, Serialize)]
pub struct MigrateResult {
    pub config_updated: bool,
    pub manifests_scanned: usize,
    pub manifests_rewritten: usize,
}

pub fn migrate_runtime(paths: &AppPaths) -> Result<MigrateResult> {
    let mut config = load_config(paths)?;
    let mut config_updated = false;
    if config.schema_version < 2 {
        config.schema_version = 2;
        config.updated_at = crate::util::now_utc_rfc3339();
        config.save(paths)?;
        config_updated = true;
    }

    let mut manifests_scanned = 0usize;
    let mut manifests_rewritten = 0usize;

    if !paths.snapshots_dir.is_dir() {
        return Ok(MigrateResult {
            config_updated,
            manifests_scanned,
            manifests_rewritten,
        });
    }

    for source_entry in fs::read_dir(&paths.snapshots_dir)
        .with_context(|| format!("failed to read {}", paths.snapshots_dir.display()))?
    {
        let source_entry = source_entry?;
        let source_path = source_entry.path();
        if !source_path.is_dir() {
            continue;
        }

        for snapshot_entry in fs::read_dir(&source_path)
            .with_context(|| format!("failed to read {}", source_path.display()))?
        {
            let snapshot_entry = snapshot_entry?;
            let snapshot_path = snapshot_entry.path();
            if !snapshot_path.is_dir() {
                continue;
            }

            let manifest_path = snapshot_path.join("manifest.json");
            if !manifest_path.is_file() {
                continue;
            }

            manifests_scanned += 1;
            let body = fs::read_to_string(&manifest_path)
                .with_context(|| format!("failed to read {}", manifest_path.display()))?;
            let mut manifest = serde_json::from_str::<SnapshotManifest>(&body)
                .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
            let original_schema = manifest.schema_version;
            let original_diff_missing = manifest.diff.is_none();

            if original_diff_missing {
                manifest.diff = Some(diff_summary_from_pages(
                    &manifest.pages,
                    manifest.previous_snapshot_label.as_deref(),
                ));
            }
            if manifest.schema_version < 7 {
                manifest.schema_version = 7;
            }

            if manifest.schema_version != original_schema || original_diff_missing {
                let rewritten = serde_json::to_string_pretty(&manifest)?;
                fs::write(&manifest_path, rewritten)
                    .with_context(|| format!("failed to write {}", manifest_path.display()))?;
                manifests_rewritten += 1;
            }
        }
    }

    Ok(MigrateResult {
        config_updated,
        manifests_scanned,
        manifests_rewritten,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Result;

    use super::migrate_runtime;
    use crate::config::AppPaths;
    use crate::models::{
        DiscoverySummary, PageChangeStatus, PageManifestEntry, SnapshotManifest, SourceKind,
        VersionStrategy,
    };
    use crate::probe::{DetectedInputKind, SuggestedMode};

    #[test]
    fn migrates_legacy_manifest_to_current_schema() -> Result<()> {
        let root = make_temp_dir("migrate");
        let paths = AppPaths {
            home: root.clone(),
            config_file: root.join("config.json"),
            sources_dir: root.join("sources"),
            snapshots_dir: root.join("snapshots"),
        };
        fs::create_dir_all(paths.snapshots_dir.join("demo/snap-1"))?;
        fs::write(
            &paths.config_file,
            r#"{"schema_version":1,"created_at":"2026-03-09T00:00:00Z","updated_at":"2026-03-09T00:00:00Z","default_proxy_url":null,"sources":{}}"#,
        )?;

        let manifest = SnapshotManifest {
            schema_version: 4,
            created_at: "2026-03-09T00:00:00Z".to_string(),
            source_name: "demo".to_string(),
            entry_url: "https://example.com".to_string(),
            source_kind: SourceKind::Website,
            version_strategy: VersionStrategy::DateSnapshot,
            source_ref: "snap-1".to_string(),
            snapshot_label: "snap-1".to_string(),
            snapshot_dir: paths.snapshots_dir.join("demo/snap-1"),
            status: "fetched".to_string(),
            previous_snapshot_label: Some("snap-0".to_string()),
            detected_input_kind: DetectedInputKind::ContentPage,
            suggested_mode: SuggestedMode::HybridSeed,
            discovery: DiscoverySummary {
                manifest_path: paths.snapshots_dir.join("demo/snap-1/discovery.json"),
                adapters: vec!["seed_page".to_string()],
                frontier_count: 1,
                llms_index_url: None,
                llms_full_index_url: None,
                sitemap_count: 0,
            },
            git: None,
            fetch: None,
            diff: None,
            pages: vec![PageManifestEntry {
                page_key: "https://example.com/intro".to_string(),
                url: "https://example.com/intro".to_string(),
                final_url: "https://example.com/intro".to_string(),
                fetch_method: "markdown_negotiation".to_string(),
                status: "stored".to_string(),
                change_status: PageChangeStatus::Changed,
                reused_from_snapshot: None,
                page_path: None,
                metadata_path: None,
                raw_path: None,
                rendered_raw_path: None,
                content_type: Some("text/markdown".to_string()),
                sha256: Some("abc".to_string()),
                raw_sha256: None,
                etag: None,
                last_modified: None,
                byte_size: 10,
                normalization: None,
                quality: None,
            }],
            notes: Vec::new(),
        };
        fs::write(
            paths.snapshots_dir.join("demo/snap-1/manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;

        let result = migrate_runtime(&paths)?;
        assert!(result.config_updated);
        assert_eq!(result.manifests_scanned, 1);
        assert_eq!(result.manifests_rewritten, 1);

        let migrated: SnapshotManifest = serde_json::from_str(&fs::read_to_string(
            paths.snapshots_dir.join("demo/snap-1/manifest.json"),
        )?)?;
        assert_eq!(migrated.schema_version, 7);
        assert_eq!(
            migrated.diff.expect("diff").import_candidates,
            1,
            "changed page should become import candidate"
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    fn make_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("current time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("docsync-{label}-{nanos}"));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }
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
