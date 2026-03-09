use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::config::AppPaths;
use crate::models::{PageChangeStatus, PageManifestEntry, SnapshotManifest};
use crate::util::now_utc_rfc3339;

const DEFAULT_OMNIMEM_PATH: &str = "/root/omnimem/omnimem";

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub source_name: String,
    pub snapshot_label: String,
    pub selected_pages: usize,
    pub imported_pages: usize,
    pub failed_pages: usize,
    pub skipped_unchanged_pages: usize,
    pub skipped_duplicate_pages: usize,
    pub summary_path: PathBuf,
    pub omnimem_cmd: String,
    pub dry_run: bool,
}

#[derive(Debug, Serialize)]
pub struct VerifyResult {
    pub source_name: String,
    pub snapshot_label: String,
    pub query: String,
    pub success: bool,
    pub summary_path: PathBuf,
    pub omnimem_cmd: String,
}

#[derive(Debug, Serialize)]
struct OmniMemImportSummary {
    schema_version: u32,
    started_at: String,
    finished_at: String,
    source_name: String,
    snapshot_label: String,
    omnimem_cmd: String,
    dry_run: bool,
    selected_pages: usize,
    imported_pages: usize,
    failed_pages: usize,
    skipped_unchanged_pages: usize,
    skipped_duplicate_pages: usize,
    items: Vec<OmniMemImportItem>,
}

#[derive(Debug, Serialize)]
struct OmniMemImportItem {
    page_path: String,
    status: String,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Serialize)]
struct OmniMemVerifySummary {
    schema_version: u32,
    started_at: String,
    finished_at: String,
    source_name: String,
    snapshot_label: String,
    query: String,
    omnimem_cmd: String,
    success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

pub fn import_snapshot(
    paths: &AppPaths,
    source_name: &str,
    reference: Option<String>,
    omnimem_cmd: Option<String>,
    direct: bool,
    dry_run: bool,
    all_pages: bool,
) -> Result<ImportResult> {
    let snapshot_dir = resolve_snapshot_dir(paths, source_name, reference.as_deref())?;
    let snapshot_label = snapshot_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown")
        .to_string();
    let manifest: SnapshotManifest = read_snapshot_manifest(&snapshot_dir)?;
    let omnimem_cmd = resolve_omnimem_cmd(omnimem_cmd);
    let summary_path = snapshot_dir.join("omnimem-import.json");
    let started_at = now_utc_rfc3339();

    let importable_pages_from_manifest = manifest
        .pages
        .iter()
        .filter(|page| {
            should_import_page(page, all_pages, manifest.previous_snapshot_label.is_some())
        })
        .collect::<Vec<_>>();
    let skipped_unchanged_pages = manifest
        .pages
        .iter()
        .filter(|page| {
            !all_pages
                && manifest.previous_snapshot_label.is_some()
                && page.change_status == PageChangeStatus::Unchanged
        })
        .count();
    let mut selected_pages = 0usize;
    let mut skipped_duplicate_pages = 0usize;
    let mut importable_pages = Vec::new();
    let mut items = Vec::new();

    let mut duplicate_groups: BTreeMap<String, Vec<(usize, &PageManifestEntry)>> = BTreeMap::new();
    let mut ungrouped_pages = Vec::new();

    for (index, page) in importable_pages_from_manifest.iter().enumerate() {
        if let Some(hash) = page.sha256.as_deref() {
            duplicate_groups
                .entry(hash.to_string())
                .or_default()
                .push((index, *page));
        } else {
            ungrouped_pages.push((index, *page));
        }
    }

    let mut selected_entries = Vec::new();
    for (index, page) in ungrouped_pages {
        selected_entries.push((index, page));
    }

    for group in duplicate_groups.into_values() {
        if group.len() == 1 {
            selected_entries.push(group[0]);
            continue;
        }

        let canonical_index = choose_canonical_page(&group);
        for (index, page) in group {
            let Some(page_path) = page.page_path.clone() else {
                continue;
            };
            if index == canonical_index {
                selected_entries.push((index, page));
                continue;
            }
            skipped_duplicate_pages += 1;
            items.push(OmniMemImportItem {
                page_path: page_path.display().to_string(),
                status: "skipped_duplicate".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
            });
        }
    }

    selected_entries.sort_by_key(|(index, _)| *index);

    for (_, page) in selected_entries {
        let Some(page_path) = page.page_path.clone() else {
            continue;
        };
        selected_pages += 1;
        importable_pages.push(page_path);
    }
    let mut imported_pages = 0usize;
    let mut failed_pages = 0usize;

    for page_path in &importable_pages {
        if dry_run {
            items.push(OmniMemImportItem {
                page_path: page_path.display().to_string(),
                status: "planned".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
            });
            continue;
        }

        let output = run_omnimem(
            &omnimem_cmd,
            &build_import_args(&page_path, direct)
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )?;
        let success = output.status.success();
        if success {
            imported_pages += 1;
        } else {
            failed_pages += 1;
        }
        items.push(OmniMemImportItem {
            page_path: page_path.display().to_string(),
            status: if success {
                "imported".to_string()
            } else {
                "failed".to_string()
            },
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let summary = OmniMemImportSummary {
        schema_version: 1,
        started_at,
        finished_at: now_utc_rfc3339(),
        source_name: source_name.to_string(),
        snapshot_label: snapshot_label.clone(),
        omnimem_cmd: omnimem_cmd.clone(),
        dry_run,
        selected_pages,
        imported_pages,
        failed_pages,
        skipped_unchanged_pages,
        skipped_duplicate_pages,
        items,
    };
    write_json(&summary_path, &summary)?;

    Ok(ImportResult {
        source_name: source_name.to_string(),
        snapshot_label,
        selected_pages,
        imported_pages,
        failed_pages,
        skipped_unchanged_pages,
        skipped_duplicate_pages,
        summary_path,
        omnimem_cmd,
        dry_run,
    })
}

pub fn verify_snapshot(
    paths: &AppPaths,
    source_name: &str,
    reference: Option<String>,
    query: &str,
    omnimem_cmd: Option<String>,
    direct: bool,
) -> Result<VerifyResult> {
    let snapshot_dir = resolve_snapshot_dir(paths, source_name, reference.as_deref())?;
    let snapshot_label = snapshot_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown")
        .to_string();
    let summary_path = snapshot_dir.join("omnimem-verify.json");
    let started_at = now_utc_rfc3339();
    let omnimem_cmd = resolve_omnimem_cmd(omnimem_cmd);
    let output = run_omnimem(
        &omnimem_cmd,
        &build_verify_args(query, direct)
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    )?;
    let success = output.status.success();

    let summary = OmniMemVerifySummary {
        schema_version: 1,
        started_at,
        finished_at: now_utc_rfc3339(),
        source_name: source_name.to_string(),
        snapshot_label: snapshot_label.clone(),
        query: query.to_string(),
        omnimem_cmd: omnimem_cmd.clone(),
        success,
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    };
    write_json(&summary_path, &summary)?;

    Ok(VerifyResult {
        source_name: source_name.to_string(),
        snapshot_label,
        query: query.to_string(),
        success,
        summary_path,
        omnimem_cmd,
    })
}

fn resolve_snapshot_dir(
    paths: &AppPaths,
    source_name: &str,
    reference: Option<&str>,
) -> Result<PathBuf> {
    let source_dir = paths.snapshots_dir.join(source_name);
    if !source_dir.is_dir() {
        bail!("no snapshots found for source `{source_name}`");
    }

    if let Some(reference) = reference {
        let snapshot_dir = source_dir.join(reference);
        if snapshot_dir.is_dir() {
            return Ok(snapshot_dir);
        }
        bail!("snapshot `{reference}` not found for source `{source_name}`");
    }

    let mut entries = fs::read_dir(&source_dir)
        .with_context(|| format!("failed to read {}", source_dir.display()))?
        .filter_map(|entry| entry.ok().map(|value| value.path()))
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    entries.sort();
    entries
        .into_iter()
        .last()
        .with_context(|| format!("no snapshots found for source `{source_name}`"))
}

fn read_snapshot_manifest(snapshot_dir: &Path) -> Result<SnapshotManifest> {
    let manifest_path = snapshot_dir.join("manifest.json");
    let body = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    serde_json::from_str(&body)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))
}

fn resolve_omnimem_cmd(explicit: Option<String>) -> String {
    if let Some(value) = explicit {
        return value;
    }
    if Path::new(DEFAULT_OMNIMEM_PATH).exists() {
        DEFAULT_OMNIMEM_PATH.to_string()
    } else {
        "omnimem".to_string()
    }
}

fn build_import_args(page_path: &Path, direct: bool) -> Vec<String> {
    let mut args = vec!["import".to_string(), page_path.display().to_string()];
    if direct {
        args.push("--direct".to_string());
    }
    args
}

fn build_verify_args(query: &str, direct: bool) -> Vec<String> {
    let mut args = vec![
        "search".to_string(),
        query.to_string(),
        "--full".to_string(),
        "--json".to_string(),
    ];
    if direct {
        args.push("--direct".to_string());
    }
    args
}

fn should_import_page(
    page: &crate::models::PageManifestEntry,
    all_pages: bool,
    has_previous_snapshot: bool,
) -> bool {
    if page.page_path.is_none() {
        return false;
    }
    if all_pages || !has_previous_snapshot {
        return true;
    }

    matches!(
        page.change_status,
        PageChangeStatus::New | PageChangeStatus::Changed | PageChangeStatus::Unknown
    )
}

fn choose_canonical_page(group: &[(usize, &PageManifestEntry)]) -> usize {
    group
        .iter()
        .min_by(|left, right| compare_canonical_pages(left.1, right.1).then(left.0.cmp(&right.0)))
        .map(|(index, _)| *index)
        .unwrap_or(0)
}

fn compare_canonical_pages(left: &PageManifestEntry, right: &PageManifestEntry) -> Ordering {
    canonical_sort_key(left).cmp(&canonical_sort_key(right))
}

fn canonical_sort_key(page: &PageManifestEntry) -> (u8, u8, u8, usize, usize, String) {
    let candidate = canonical_candidate(page);
    let normalized = candidate.trim_end_matches('/').to_string();
    let lower = normalized.to_ascii_lowercase();
    let has_query = normalized.contains('?') || normalized.contains('#');
    let is_index = lower.ends_with("/index") || lower.ends_with("/index.md");
    let has_markdown_extension = lower.ends_with(".md");
    let depth = normalized
        .split('/')
        .filter(|segment| !segment.is_empty() && !segment.contains(':'))
        .count();

    (
        u8::from(has_query),
        u8::from(has_markdown_extension),
        u8::from(is_index),
        depth,
        normalized.len(),
        normalized,
    )
}

fn canonical_candidate(page: &PageManifestEntry) -> String {
    if !page.final_url.is_empty() {
        return page.final_url.clone();
    }
    if !page.url.is_empty() {
        return page.url.clone();
    }
    page.page_path
        .as_deref()
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

fn run_omnimem(cmd: &str, args: &[&str]) -> Result<std::process::Output> {
    Command::new(cmd).args(args).output().with_context(|| {
        format!(
            "failed to execute OmniMem command `{cmd} {}`",
            args.join(" ")
        )
    })
}

fn write_json<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    let body = serde_json::to_string_pretty(value)?;
    fs::write(path, body).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Result;

    use super::{import_snapshot, verify_snapshot};
    use crate::config::AppPaths;
    use crate::models::{DiscoverySummary, SnapshotManifest, SourceKind, VersionStrategy};
    use crate::probe::{DetectedInputKind, SuggestedMode};

    #[test]
    fn imports_snapshot_with_fake_omnimem() -> Result<()> {
        let root = make_temp_dir("omnimem-import");
        let paths = make_app_paths(&root);
        let snapshot_dir = create_snapshot_fixture(&paths, "demo", "snap-1")?;
        let page_path = snapshot_dir.join("pages/intro.md");
        fs::create_dir_all(page_path.parent().expect("page parent"))?;
        fs::write(&page_path, "# Intro\n")?;
        write_manifest(&snapshot_dir, vec![page_path.clone()])?;
        let fake = fake_omnimem_script(&root, "import-ok")?;

        let result = import_snapshot(
            &paths,
            "demo",
            Some("snap-1".to_string()),
            Some(fake.to_string_lossy().to_string()),
            false,
            false,
            false,
        )?;

        assert_eq!(result.imported_pages, 1);
        assert_eq!(result.failed_pages, 0);
        assert!(result.summary_path.exists());
        let log = fs::read_to_string(root.join("omnimem-invocations.log"))?;
        assert!(log.contains("import"));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn skips_unchanged_pages_by_default_for_incremental_snapshot() -> Result<()> {
        let root = make_temp_dir("omnimem-import-incremental");
        let paths = make_app_paths(&root);
        let snapshot_dir = create_snapshot_fixture(&paths, "demo", "snap-2")?;
        let new_page = snapshot_dir.join("pages/new.md");
        let unchanged_page = snapshot_dir.join("pages/unchanged.md");
        fs::create_dir_all(new_page.parent().expect("page parent"))?;
        fs::write(&new_page, "# New\n")?;
        fs::write(&unchanged_page, "# Same\n")?;

        let manifest = SnapshotManifest {
            schema_version: 7,
            created_at: "2026-03-09T00:00:00Z".to_string(),
            source_name: "demo".to_string(),
            entry_url: "https://example.com".to_string(),
            source_kind: SourceKind::Website,
            version_strategy: VersionStrategy::DateSnapshot,
            source_ref: "snap-2".to_string(),
            snapshot_label: "snap-2".to_string(),
            snapshot_dir: snapshot_dir.clone(),
            status: "fetched".to_string(),
            previous_snapshot_label: Some("snap-1".to_string()),
            detected_input_kind: DetectedInputKind::ContentPage,
            suggested_mode: SuggestedMode::HybridSeed,
            discovery: DiscoverySummary {
                manifest_path: snapshot_dir.join("discovery.json"),
                adapters: vec!["seed_page".to_string()],
                frontier_count: 2,
                llms_index_url: None,
                llms_full_index_url: None,
                sitemap_count: 0,
            },
            git: None,
            fetch: None,
            diff: None,
            pages: vec![
                page_entry(&new_page, crate::models::PageChangeStatus::New),
                page_entry(&unchanged_page, crate::models::PageChangeStatus::Unchanged),
            ],
            notes: Vec::new(),
        };
        fs::write(
            snapshot_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;

        let fake = fake_omnimem_script(&root, "import-ok")?;
        let result = import_snapshot(
            &paths,
            "demo",
            Some("snap-2".to_string()),
            Some(fake.to_string_lossy().to_string()),
            false,
            false,
            false,
        )?;

        assert_eq!(result.selected_pages, 1);
        assert_eq!(result.skipped_unchanged_pages, 1);
        let log = fs::read_to_string(root.join("omnimem-invocations.log"))?;
        assert!(log.contains("new.md"));
        assert!(!log.contains("unchanged.md"));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn skips_duplicate_hash_pages_within_snapshot() -> Result<()> {
        let root = make_temp_dir("omnimem-import-dedup");
        let paths = make_app_paths(&root);
        let snapshot_dir = create_snapshot_fixture(&paths, "demo", "snap-3")?;
        let first_page = snapshot_dir.join("pages/first.md");
        let second_page = snapshot_dir.join("pages/second.md");
        fs::create_dir_all(first_page.parent().expect("page parent"))?;
        fs::write(&first_page, "# Same\n")?;
        fs::write(&second_page, "# Same\n")?;

        let mut first_entry = page_entry(&first_page, crate::models::PageChangeStatus::New);
        let mut second_entry = page_entry(&second_page, crate::models::PageChangeStatus::New);
        first_entry.sha256 = Some("dup-hash".to_string());
        second_entry.sha256 = Some("dup-hash".to_string());

        let manifest = SnapshotManifest {
            schema_version: 7,
            created_at: "2026-03-09T00:00:00Z".to_string(),
            source_name: "demo".to_string(),
            entry_url: "https://example.com".to_string(),
            source_kind: SourceKind::Website,
            version_strategy: VersionStrategy::DateSnapshot,
            source_ref: "snap-3".to_string(),
            snapshot_label: "snap-3".to_string(),
            snapshot_dir: snapshot_dir.clone(),
            status: "fetched".to_string(),
            previous_snapshot_label: None,
            detected_input_kind: DetectedInputKind::ContentPage,
            suggested_mode: SuggestedMode::HybridSeed,
            discovery: DiscoverySummary {
                manifest_path: snapshot_dir.join("discovery.json"),
                adapters: vec!["seed_page".to_string()],
                frontier_count: 2,
                llms_index_url: None,
                llms_full_index_url: None,
                sitemap_count: 0,
            },
            git: None,
            fetch: None,
            diff: None,
            pages: vec![first_entry, second_entry],
            notes: Vec::new(),
        };
        fs::write(
            snapshot_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;

        let fake = fake_omnimem_script(&root, "import-ok")?;
        let result = import_snapshot(
            &paths,
            "demo",
            Some("snap-3".to_string()),
            Some(fake.to_string_lossy().to_string()),
            false,
            false,
            false,
        )?;

        assert_eq!(result.selected_pages, 1);
        assert_eq!(result.skipped_duplicate_pages, 1);
        let log = fs::read_to_string(root.join("omnimem-invocations.log"))?;
        assert!(
            (log.contains("first.md") && !log.contains("second.md"))
                || (!log.contains("first.md") && log.contains("second.md"))
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn prefers_cleaner_canonical_page_when_hashes_match() -> Result<()> {
        let root = make_temp_dir("omnimem-import-canonical");
        let paths = make_app_paths(&root);
        let snapshot_dir = create_snapshot_fixture(&paths, "demo", "snap-4")?;
        let clean_page = snapshot_dir.join("pages/guide.md");
        let generated_page = snapshot_dir.join("pages/guide-index.md");
        fs::create_dir_all(clean_page.parent().expect("page parent"))?;
        fs::write(&clean_page, "# Guide\n")?;
        fs::write(&generated_page, "# Guide\n")?;

        let mut clean_entry = page_entry(&clean_page, crate::models::PageChangeStatus::New);
        clean_entry.url = "https://docs.example.com/guide".to_string();
        clean_entry.final_url = clean_entry.url.clone();
        clean_entry.page_key = clean_entry.url.clone();
        clean_entry.sha256 = Some("dup-guide".to_string());

        let mut generated_entry = page_entry(&generated_page, crate::models::PageChangeStatus::New);
        generated_entry.url = "https://docs.example.com/guide/index.md".to_string();
        generated_entry.final_url = generated_entry.url.clone();
        generated_entry.page_key = generated_entry.url.clone();
        generated_entry.sha256 = Some("dup-guide".to_string());

        let manifest = SnapshotManifest {
            schema_version: 7,
            created_at: "2026-03-09T00:00:00Z".to_string(),
            source_name: "demo".to_string(),
            entry_url: "https://example.com".to_string(),
            source_kind: SourceKind::Website,
            version_strategy: VersionStrategy::DateSnapshot,
            source_ref: "snap-4".to_string(),
            snapshot_label: "snap-4".to_string(),
            snapshot_dir: snapshot_dir.clone(),
            status: "fetched".to_string(),
            previous_snapshot_label: None,
            detected_input_kind: DetectedInputKind::ContentPage,
            suggested_mode: SuggestedMode::HybridSeed,
            discovery: DiscoverySummary {
                manifest_path: snapshot_dir.join("discovery.json"),
                adapters: vec!["seed_page".to_string()],
                frontier_count: 2,
                llms_index_url: None,
                llms_full_index_url: None,
                sitemap_count: 0,
            },
            git: None,
            fetch: None,
            diff: None,
            pages: vec![generated_entry, clean_entry],
            notes: Vec::new(),
        };
        fs::write(
            snapshot_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;

        let fake = fake_omnimem_script(&root, "import-ok")?;
        let result = import_snapshot(
            &paths,
            "demo",
            Some("snap-4".to_string()),
            Some(fake.to_string_lossy().to_string()),
            false,
            false,
            false,
        )?;

        assert_eq!(result.selected_pages, 1);
        assert_eq!(result.skipped_duplicate_pages, 1);
        let log = fs::read_to_string(root.join("omnimem-invocations.log"))?;
        assert!(log.contains("guide.md"));
        assert!(!log.contains("guide-index.md"));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn verifies_snapshot_with_fake_omnimem() -> Result<()> {
        let root = make_temp_dir("omnimem-verify");
        let paths = make_app_paths(&root);
        let snapshot_dir = create_snapshot_fixture(&paths, "demo", "snap-1")?;
        write_manifest(&snapshot_dir, Vec::new())?;
        let fake = fake_omnimem_script(&root, "search-ok")?;

        let result = verify_snapshot(
            &paths,
            "demo",
            Some("snap-1".to_string()),
            "release notes",
            Some(fake.to_string_lossy().to_string()),
            false,
        )?;

        assert!(result.success);
        assert!(result.summary_path.exists());
        let log = fs::read_to_string(root.join("omnimem-invocations.log"))?;
        assert!(log.contains("search"));
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
        Ok(snapshot_dir)
    }

    fn write_manifest(snapshot_dir: &Path, page_paths: Vec<PathBuf>) -> Result<()> {
        let manifest = SnapshotManifest {
            schema_version: 7,
            created_at: "2026-03-09T00:00:00Z".to_string(),
            source_name: "demo".to_string(),
            entry_url: "https://example.com".to_string(),
            source_kind: SourceKind::Website,
            version_strategy: VersionStrategy::DateSnapshot,
            source_ref: "snap-1".to_string(),
            snapshot_label: "snap-1".to_string(),
            snapshot_dir: snapshot_dir.to_path_buf(),
            status: "fetched".to_string(),
            previous_snapshot_label: None,
            detected_input_kind: DetectedInputKind::ContentPage,
            suggested_mode: SuggestedMode::HybridSeed,
            discovery: DiscoverySummary {
                manifest_path: snapshot_dir.join("discovery.json"),
                adapters: vec!["seed_page".to_string()],
                frontier_count: page_paths.len(),
                llms_index_url: None,
                llms_full_index_url: None,
                sitemap_count: 0,
            },
            git: None,
            fetch: None,
            diff: None,
            pages: page_paths
                .into_iter()
                .map(|path| crate::models::PageManifestEntry {
                    page_key: format!("file://{}", path.display()),
                    url: format!("file://{}", path.display()),
                    final_url: format!("file://{}", path.display()),
                    fetch_method: "markdown_negotiation".to_string(),
                    status: "stored".to_string(),
                    change_status: crate::models::PageChangeStatus::Unknown,
                    reused_from_snapshot: None,
                    page_path: Some(path.clone()),
                    metadata_path: None,
                    raw_path: Some(path),
                    rendered_raw_path: None,
                    content_type: Some("text/markdown".to_string()),
                    sha256: None,
                    raw_sha256: None,
                    etag: None,
                    last_modified: None,
                    byte_size: 8,
                    normalization: None,
                })
                .collect(),
            notes: Vec::new(),
        };
        fs::write(
            snapshot_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        Ok(())
    }

    fn page_entry(
        path: &Path,
        change_status: crate::models::PageChangeStatus,
    ) -> crate::models::PageManifestEntry {
        crate::models::PageManifestEntry {
            page_key: format!("file://{}", path.display()),
            url: format!("file://{}", path.display()),
            final_url: format!("file://{}", path.display()),
            fetch_method: "markdown_negotiation".to_string(),
            status: "stored".to_string(),
            change_status,
            reused_from_snapshot: None,
            page_path: Some(path.to_path_buf()),
            metadata_path: None,
            raw_path: Some(path.to_path_buf()),
            rendered_raw_path: None,
            content_type: Some("text/markdown".to_string()),
            sha256: None,
            raw_sha256: None,
            etag: None,
            last_modified: None,
            byte_size: 8,
            normalization: None,
        }
    }

    fn make_app_paths(root: &Path) -> AppPaths {
        AppPaths {
            home: root.to_path_buf(),
            config_file: root.join("config.json"),
            sources_dir: root.join("sources"),
            snapshots_dir: root.join("snapshots"),
        }
    }

    fn fake_omnimem_script(root: &Path, mode: &str) -> Result<PathBuf> {
        let script_path = root.join("fake-omnimem.sh");
        let body = format!(
            "#!/usr/bin/env bash\n\
set -euo pipefail\n\
printf '%s\\n' \"$*\" >> \"{log}\"\n\
if [[ \"$1\" == \"search\" ]]; then\n\
  printf '%s\\n' '[{{\"content\":\"ok\"}}]'\n\
else\n\
  printf '%s\\n' 'imported'\n\
fi\n",
            log = root.join("omnimem-invocations.log").display()
        );
        fs::write(&script_path, body)?;
        let mut permissions = fs::metadata(&script_path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions)?;
        let _ = mode;
        Ok(script_path)
    }

    fn make_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("current time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "docsync-omnimem-test-{}-{}-{nanos}",
            std::process::id(),
            label
        ));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }
}
