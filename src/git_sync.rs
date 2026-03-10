use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::chunking::{ChunkingConfig, summarize_chunks, write_markdown_chunks};
use crate::incremental::PreviousPageState;
use crate::models::{
    DiscoveredPage, DiscoveryManifest, DiscoveryOrigin, FetchSummary, GitSummary, PageChangeStatus,
    PageManifestEntry, PageMetadata, SourceDefinition,
};
use crate::network::apply_proxy_to_git_command;
use crate::normalize::normalize_markdown;
use crate::probe::{DetectedInputKind, SuggestedMode};
use crate::quality::{score_markdown_quality, summarize_quality};
use crate::util::{ensure_directory, now_utc_rfc3339};

const NAV_MANIFEST_NAMES: &[&str] = &["meta.json", "docs.json", "mint.json"];
const DOCS_ROOT_CANDIDATES: &[&str] = &[
    "docs",
    "apps/docs/content",
    "content/docs",
    "apps/v4/content/docs",
    "website/content",
];

#[derive(Debug, Serialize)]
pub struct GitSyncOutcome {
    pub discovery: DiscoveryManifest,
    pub git: GitSummary,
    pub fetch: FetchSummary,
    pub pages: Vec<PageManifestEntry>,
}

pub fn sync_git_source(
    snapshot_dir: &Path,
    source: &SourceDefinition,
    source_ref: &str,
    snapshot_label: &str,
    write_snapshot: bool,
    previous_snapshot_label: Option<&str>,
    previous_pages: Option<&BTreeMap<String, PreviousPageState>>,
    proxy_url: Option<&str>,
    chunking: ChunkingConfig,
) -> Result<GitSyncOutcome> {
    let repo_url = source.repo_url.clone().with_context(|| {
        format!(
            "source `{}` is missing --repo for git-docs sync",
            source.name
        )
    })?;

    let checkout = GitCheckout::clone(&repo_url, proxy_url)?;
    checkout.checkout_ref(source_ref, proxy_url)?;
    let resolved_ref = checkout.rev_parse_head()?;

    let docs_root = detect_docs_root(&checkout.path, source.docs_path.as_deref())?;
    let detected_docs_path = source.docs_path.is_none();
    let repo_relative_docs_path = relative_to(&checkout.path, &docs_root)?;

    let nav_manifests = find_nav_manifests(&docs_root)?
        .into_iter()
        .map(|path| relative_to(&checkout.path, &path))
        .collect::<Result<Vec<_>>>()?;

    let markdown_files = find_markdown_files(&docs_root)?;
    let frontier = markdown_files
        .iter()
        .map(|path| {
            let rel = relative_to(&docs_root, path)?;
            Ok(DiscoveredPage {
                url: git_page_id(&repo_url, &resolved_ref, &rel),
                discovered_from: DiscoveryOrigin::SeedPage,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let discovery = DiscoveryManifest {
        schema_version: 1,
        created_at: now_utc_rfc3339(),
        source_name: source.name.clone(),
        entry_url: source.entry_url.clone(),
        final_url: source.entry_url.clone(),
        source_ref: source_ref.to_string(),
        detected_input_kind: DetectedInputKind::SiteRoot,
        suggested_mode: SuggestedMode::DiscoveryRoot,
        adapters: vec!["git_docs".to_string()],
        llms_index_url: None,
        llms_full_index_url: None,
        sitemap_urls: Vec::new(),
        frontier,
        notes: vec![
            format!("Git-native sync resolved ref {resolved_ref}."),
            format!("Docs root: {repo_relative_docs_path}"),
        ],
    };

    let git = GitSummary {
        repo_url: repo_url.clone(),
        requested_ref: source_ref.to_string(),
        resolved_ref: resolved_ref.clone(),
        docs_path: repo_relative_docs_path,
        detected_docs_path,
        nav_manifests,
    };

    let (fetch, pages) = if write_snapshot {
        snapshot_git_pages(
            snapshot_dir,
            source,
            source_ref,
            snapshot_label,
            &resolved_ref,
            &repo_url,
            &docs_root,
            &markdown_files,
            previous_snapshot_label,
            previous_pages,
            chunking,
        )?
    } else {
        (
            FetchSummary {
                attempted: markdown_files.len(),
                stored_pages: 0,
                skipped_pages: 0,
                reused_pages: 0,
                normalized_pages: 0,
                normalization_changed_pages: 0,
                quality: None,
                chunking: None,
                method_counts: BTreeMap::from([("git_checkout".to_string(), markdown_files.len())]),
            },
            Vec::new(),
        )
    };

    Ok(GitSyncOutcome {
        discovery,
        git,
        fetch,
        pages,
    })
}

struct GitCheckout {
    path: PathBuf,
}

impl GitCheckout {
    fn clone(repo_url: &str, proxy_url: Option<&str>) -> Result<Self> {
        let path = make_checkout_dir()?;
        let path_str = path.to_string_lossy().to_string();
        let args = [
            "clone",
            "--quiet",
            "--no-checkout",
            repo_url,
            path_str.as_str(),
        ];
        run_git(None, &args, proxy_url).with_context(|| format!("failed to clone `{repo_url}`"))?;
        Ok(Self { path })
    }

    fn checkout_ref(&self, reference: &str, proxy_url: Option<&str>) -> Result<()> {
        let args = ["checkout", "--quiet", reference];
        run_git(Some(&self.path), &args, proxy_url)
            .with_context(|| format!("failed to checkout ref `{reference}`"))?;
        Ok(())
    }

    fn rev_parse_head(&self) -> Result<String> {
        let output = run_git_capture(Some(&self.path), &["rev-parse", "HEAD"])?;
        Ok(output.trim().to_string())
    }
}

impl Drop for GitCheckout {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn snapshot_git_pages(
    snapshot_dir: &Path,
    source: &SourceDefinition,
    source_ref: &str,
    snapshot_label: &str,
    resolved_ref: &str,
    repo_url: &str,
    docs_root: &Path,
    markdown_files: &[PathBuf],
    previous_snapshot_label: Option<&str>,
    previous_pages: Option<&BTreeMap<String, PreviousPageState>>,
    chunking: ChunkingConfig,
) -> Result<(FetchSummary, Vec<PageManifestEntry>)> {
    let pages_root = snapshot_dir.join("pages");
    let chunks_root = snapshot_dir.join("chunks");
    let raw_root = snapshot_dir.join("raw");
    ensure_directory(&pages_root)?;
    ensure_directory(&chunks_root)?;
    ensure_directory(&raw_root)?;

    let mut pages = Vec::new();
    let mut reused_pages = 0usize;
    let mut normalization_changed_pages = 0usize;
    let mut quality_pages = Vec::new();
    let mut chunk_entries = Vec::new();
    let mut seen_keys = std::collections::BTreeSet::new();

    for file_path in markdown_files {
        let relative = relative_to(docs_root, file_path)?;
        seen_keys.insert(relative.clone());
        let page_path = pages_root.join(&relative);
        let raw_path = raw_root.join(&relative);
        copy_file(file_path, &raw_path)?;

        let bytes = fs::read(file_path)
            .with_context(|| format!("failed to read source docs file {}", file_path.display()))?;
        let normalized = normalize_markdown(&String::from_utf8_lossy(&bytes));
        let quality = score_markdown_quality(&normalized.markdown);
        write_bytes(&page_path, normalized.markdown.as_bytes())?;
        let sha256 = sha256_hex(normalized.markdown.as_bytes());
        let page_url = git_page_id(repo_url, resolved_ref, &relative);
        let stem = relative
            .strip_suffix(".md")
            .or_else(|| relative.strip_suffix(".mdx"))
            .unwrap_or(&relative);
        let chunks = write_markdown_chunks(
            &chunks_root,
            stem,
            &relative,
            &normalized.markdown,
            chunking,
        )?;
        let change_status = match previous_pages.and_then(|pages| pages.get(&relative)) {
            None => PageChangeStatus::New,
            Some(previous) if previous.content_hash.as_deref() == Some(sha256.as_str()) => {
                PageChangeStatus::Unchanged
            }
            Some(_) => PageChangeStatus::Changed,
        };
        if change_status == PageChangeStatus::Unchanged {
            reused_pages += 1;
        }
        if normalized.summary.changed {
            normalization_changed_pages += 1;
        }
        let metadata_path = metadata_path_for(&page_path);
        let metadata = PageMetadata {
            schema_version: 3,
            fetched_at: now_utc_rfc3339(),
            source_name: source.name.clone(),
            snapshot_label: snapshot_label.to_string(),
            source_ref: source_ref.to_string(),
            page_key: relative.clone(),
            requested_url: page_url.clone(),
            final_url: page_url.clone(),
            fetch_method: "git_checkout".to_string(),
            discovered_from: DiscoveryOrigin::SeedPage,
            content_type: Some(content_type_for(file_path)),
            status_code: 200,
            byte_size: normalized.markdown.len() as u64,
            sha256: sha256.clone(),
            raw_sha256: Some(bytes_hash_for_raw(&bytes)),
            etag: None,
            last_modified: None,
            x_markdown_tokens: None,
            x_original_tokens: None,
            content_signal: None,
            page_path: page_path.clone(),
            raw_path: raw_path.clone(),
            rendered_raw_path: None,
            normalization: Some(normalized.summary.clone()),
            quality: Some(quality.clone()),
        };
        write_json(&metadata_path, &metadata)?;
        quality_pages.push(quality.clone());
        chunk_entries.extend(chunks.clone());

        pages.push(PageManifestEntry {
            page_key: relative.clone(),
            url: page_url.clone(),
            final_url: page_url,
            fetch_method: "git_checkout".to_string(),
            status: "stored".to_string(),
            change_status,
            reused_from_snapshot: previous_snapshot_label
                .filter(|_| change_status == PageChangeStatus::Unchanged)
                .map(ToOwned::to_owned),
            page_path: Some(page_path),
            metadata_path: Some(metadata_path),
            raw_path: Some(raw_path),
            rendered_raw_path: None,
            content_type: Some(content_type_for(file_path)),
            sha256: Some(sha256),
            raw_sha256: Some(bytes_hash_for_raw(&bytes)),
            etag: None,
            last_modified: None,
            byte_size: normalized.markdown.len() as u64,
            normalization: Some(normalized.summary),
            quality: Some(quality),
            chunks,
        });
    }

    if let Some(previous_pages) = previous_pages {
        for previous in previous_pages.values() {
            if !seen_keys.contains(&previous.page_key) {
                seen_keys.insert(previous.page_key.clone());
                pages.push(PageManifestEntry {
                    page_key: previous.page_key.clone(),
                    url: String::new(),
                    final_url: String::new(),
                    fetch_method: previous.fetch_method.clone(),
                    status: "removed".to_string(),
                    change_status: PageChangeStatus::Removed,
                    reused_from_snapshot: Some(previous.snapshot_label.clone()),
                    page_path: None,
                    metadata_path: None,
                    raw_path: None,
                    rendered_raw_path: None,
                    content_type: previous.content_type.clone(),
                    sha256: previous.content_hash.clone(),
                    raw_sha256: previous.raw_hash.clone(),
                    etag: None,
                    last_modified: None,
                    byte_size: 0,
                    normalization: previous.normalization.clone(),
                    quality: previous.quality.clone(),
                    chunks: Vec::new(),
                });
            }
        }
    }

    Ok((
        FetchSummary {
            attempted: markdown_files.len(),
            stored_pages: markdown_files.len(),
            skipped_pages: 0,
            reused_pages,
            normalized_pages: markdown_files.len(),
            normalization_changed_pages,
            quality: Some(summarize_quality(quality_pages.iter())),
            chunking: Some(summarize_chunks(&chunk_entries, chunking)),
            method_counts: BTreeMap::from([("git_checkout".to_string(), markdown_files.len())]),
        },
        pages,
    ))
}

fn bytes_hash_for_raw(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_directory(parent)?;
    }
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
}

fn detect_docs_root(repo_root: &Path, configured: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = configured {
        let candidate = repo_root.join(path);
        if candidate.is_dir() {
            return Ok(candidate);
        }
        bail!("configured docs path `{path}` does not exist in cloned repo");
    }

    for candidate in DOCS_ROOT_CANDIDATES {
        let candidate_path = repo_root.join(candidate);
        if candidate_path.is_dir() && contains_markdown_files(&candidate_path)? {
            return Ok(candidate_path);
        }
    }

    let mut markdown_dirs = Vec::new();
    collect_markdown_dirs(repo_root, repo_root, &mut markdown_dirs)?;
    markdown_dirs.sort_by_key(|path| path.components().count());
    markdown_dirs
        .into_iter()
        .next()
        .with_context(|| "failed to detect a docs root containing markdown files")
}

fn contains_markdown_files(path: &Path) -> Result<bool> {
    Ok(!find_markdown_files(path)?.is_empty())
}

fn collect_markdown_dirs(root: &Path, current: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    let mut has_markdown = false;
    for entry in fs::read_dir(current)
        .with_context(|| format!("failed to read directory {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_dirs(root, &path, output)?;
        } else if is_markdown_file(&path) {
            has_markdown = true;
        }
    }

    if has_markdown && current != root {
        output.push(current.to_path_buf());
    }

    Ok(())
}

fn find_markdown_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk_files(root, &mut |path| {
        if is_markdown_file(path) {
            files.push(path.to_path_buf());
        }
        Ok(())
    })?;
    files.sort();
    Ok(files)
}

fn find_nav_manifests(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk_files(root, &mut |path| {
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if NAV_MANIFEST_NAMES.contains(&file_name) {
            files.push(path.to_path_buf());
        }
        Ok(())
    })?;
    files.sort();
    Ok(files)
}

fn walk_files(path: &Path, visit: &mut dyn FnMut(&Path) -> Result<()>) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            walk_files(&child, visit)?;
        } else {
            visit(&child)?;
        }
    }
    Ok(())
}

fn is_markdown_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("md") | Some("mdx")
    )
}

fn git_page_id(repo_url: &str, resolved_ref: &str, relative_path: &str) -> String {
    format!("git+{repo_url}#{resolved_ref}:{relative_path}")
}

fn relative_to(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("{} is not under {}", path.display(), root.display()))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn metadata_path_for(page_path: &Path) -> PathBuf {
    let file_name = page_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("page.md");
    page_path.with_file_name(format!("{file_name}.json"))
}

fn content_type_for(path: &Path) -> String {
    match path.extension().and_then(|value| value.to_str()) {
        Some("mdx") => "text/mdx".to_string(),
        _ => "text/markdown".to_string(),
    }
}

fn copy_file(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        ensure_directory(parent)?;
    }
    fs::copy(from, to)
        .with_context(|| format!("failed to copy {} to {}", from.display(), to.display()))?;
    Ok(())
}

fn write_json<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        ensure_directory(parent)?;
    }
    let body = serde_json::to_string_pretty(value)?;
    fs::write(path, body).with_context(|| format!("failed to write {}", path.display()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn make_checkout_dir() -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock drifted before unix epoch")?
        .as_nanos();
    let path = std::env::temp_dir().join(format!("docsync-git-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).with_context(|| format!("failed to create {}", path.display()))?;
    Ok(path)
}

fn run_git(cwd: Option<&Path>, args: &[&str], proxy_url: Option<&str>) -> Result<()> {
    let mut command = base_git_command(cwd);
    command.args(args);
    apply_proxy_to_git_command(&mut command, proxy_url);
    let output = command
        .output()
        .with_context(|| format!("failed to execute git {}", args.join(" ")))?;

    if output.status.success() {
        return Ok(());
    }

    bail!(
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

fn run_git_capture(cwd: Option<&Path>, args: &[&str]) -> Result<String> {
    let output = base_git_command(cwd)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute git {}", args.join(" ")))?;

    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn base_git_command(cwd: Option<&Path>) -> Command {
    let mut command = Command::new("git");
    // Force checkout content to stay LF-normalized so incremental hashes are stable across OSes.
    command
        .args(["-c", "core.autocrlf=false", "-c", "core.eol=lf"])
        .current_dir(cwd.unwrap_or_else(|| Path::new(".")));
    command
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Result;

    use super::{detect_docs_root, find_nav_manifests, relative_to, sha256_hex, sync_git_source};
    use crate::chunking::ChunkingConfig;
    use crate::incremental::PreviousPageState;
    use crate::models::{SourceDefinition, SourceKind, VersionStrategy};

    #[test]
    fn detects_common_docs_root_candidate() -> Result<()> {
        let repo = make_temp_dir("docs-root");
        fs::create_dir_all(repo.join("apps/docs/content"))?;
        fs::write(repo.join("apps/docs/content/intro.md"), "# Intro\n")?;

        let detected = detect_docs_root(&repo, None)?;
        assert_eq!(
            relative_to(&repo, &detected)?,
            "apps/docs/content".to_string()
        );
        fs::remove_dir_all(repo)?;
        Ok(())
    }

    #[test]
    fn finds_nav_manifests_under_docs_root() -> Result<()> {
        let root = make_temp_dir("nav-manifests");
        fs::create_dir_all(root.join("docs/guides"))?;
        fs::write(root.join("docs/meta.json"), "{}")?;
        fs::write(root.join("docs/guides/docs.json"), "{}")?;

        let manifests = find_nav_manifests(&root.join("docs"))?;
        let names = manifests
            .iter()
            .map(|path| relative_to(&root, path))
            .collect::<Result<Vec<_>>>()?;
        assert_eq!(names, vec!["docs/guides/docs.json", "docs/meta.json"]);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn syncs_git_docs_from_local_repo_and_detects_docs_path() -> Result<()> {
        let repo = init_repo_fixture()?;
        let snapshot = make_temp_dir("snapshot");
        let source = SourceDefinition {
            name: "fixture".to_string(),
            entry_url: "https://example.com/docs".to_string(),
            proxy_url: None,
            browser_cmd: None,
            auto_import: false,
            omnimem_cmd: None,
            omnimem_direct: false,
            omnimem_include_low_signal: false,
            source_kind: SourceKind::GitDocs,
            repo_url: Some(repo.path.to_string_lossy().to_string()),
            docs_path: None,
            default_ref: Some("main".to_string()),
            version_strategy: VersionStrategy::GitRef,
            tags: vec!["docs".to_string()],
            created_at: "2026-03-09T00:00:00Z".to_string(),
            updated_at: "2026-03-09T00:00:00Z".to_string(),
        };

        let outcome = sync_git_source(
            &snapshot,
            &source,
            "main",
            "main",
            true,
            None,
            None,
            None,
            ChunkingConfig::default(),
        )?;
        assert_eq!(outcome.git.docs_path, "docs");
        assert!(outcome.git.detected_docs_path);
        assert_eq!(outcome.fetch.stored_pages, 2);
        assert_eq!(outcome.git.nav_manifests, vec!["docs/meta.json"]);
        assert_eq!(
            outcome.discovery.frontier.len(),
            2,
            "should discover the markdown corpus from git"
        );
        assert!(snapshot.join("pages/guide/intro.md").exists());
        assert!(snapshot.join("pages/guide/intro.md.json").exists());
        assert!(snapshot.join("raw/guide/intro.md").exists());

        fs::remove_dir_all(snapshot)?;
        fs::remove_dir_all(repo.path)?;
        Ok(())
    }

    #[test]
    fn syncs_git_docs_from_specific_commit() -> Result<()> {
        let repo = init_repo_fixture()?;
        let snapshot = make_temp_dir("snapshot-commit");
        let source = SourceDefinition {
            name: "fixture".to_string(),
            entry_url: "https://example.com/docs".to_string(),
            proxy_url: None,
            browser_cmd: None,
            auto_import: false,
            omnimem_cmd: None,
            omnimem_direct: false,
            omnimem_include_low_signal: false,
            source_kind: SourceKind::GitDocs,
            repo_url: Some(repo.path.to_string_lossy().to_string()),
            docs_path: Some("docs".to_string()),
            default_ref: None,
            version_strategy: VersionStrategy::GitRef,
            tags: vec![],
            created_at: "2026-03-09T00:00:00Z".to_string(),
            updated_at: "2026-03-09T00:00:00Z".to_string(),
        };

        let outcome = sync_git_source(
            &snapshot,
            &source,
            &repo.first_commit,
            "commit",
            true,
            None,
            None,
            None,
            ChunkingConfig::default(),
        )?;
        assert_eq!(outcome.git.resolved_ref, repo.first_commit);
        assert_eq!(outcome.fetch.stored_pages, 1);
        assert!(snapshot.join("pages/guide/intro.md").exists());
        assert!(!snapshot.join("pages/api/reference.mdx").exists());

        fs::remove_dir_all(snapshot)?;
        fs::remove_dir_all(repo.path)?;
        Ok(())
    }

    #[test]
    fn marks_git_pages_as_changed_unchanged_and_removed() -> Result<()> {
        let repo = init_repo_fixture()?;
        let snapshot = make_temp_dir("snapshot-diff");
        let source = SourceDefinition {
            name: "fixture".to_string(),
            entry_url: "https://example.com/docs".to_string(),
            proxy_url: None,
            browser_cmd: None,
            auto_import: false,
            omnimem_cmd: None,
            omnimem_direct: false,
            omnimem_include_low_signal: false,
            source_kind: SourceKind::GitDocs,
            repo_url: Some(repo.path.to_string_lossy().to_string()),
            docs_path: Some("docs".to_string()),
            default_ref: Some("main".to_string()),
            version_strategy: VersionStrategy::GitRef,
            tags: vec![],
            created_at: "2026-03-09T00:00:00Z".to_string(),
            updated_at: "2026-03-09T00:00:00Z".to_string(),
        };

        let previous_pages = BTreeMap::from([
            (
                "guide/intro.md".to_string(),
                PreviousPageState {
                    snapshot_label: "prev".to_string(),
                    page_key: "guide/intro.md".to_string(),
                    content_hash: Some(sha256_hex(b"# Intro\n")),
                    raw_hash: Some(sha256_hex(b"# Intro\n")),
                    fetch_method: "git_checkout".to_string(),
                    page_path: None,
                    raw_path: None,
                    rendered_raw_path: None,
                    content_type: Some("text/markdown".to_string()),
                    etag: None,
                    last_modified: None,
                    normalization: None,
                    quality: None,
                    chunks: Vec::new(),
                },
            ),
            (
                "guide/removed.md".to_string(),
                PreviousPageState {
                    snapshot_label: "prev".to_string(),
                    page_key: "guide/removed.md".to_string(),
                    content_hash: Some("deadbeef".to_string()),
                    raw_hash: Some("deadbeef".to_string()),
                    fetch_method: "git_checkout".to_string(),
                    page_path: None,
                    raw_path: None,
                    rendered_raw_path: None,
                    content_type: Some("text/markdown".to_string()),
                    etag: None,
                    last_modified: None,
                    normalization: None,
                    quality: None,
                    chunks: Vec::new(),
                },
            ),
        ]);

        let outcome = sync_git_source(
            &snapshot,
            &source,
            "main",
            "main",
            true,
            Some("prev"),
            Some(&previous_pages),
            None,
            ChunkingConfig::default(),
        )?;
        let intro = outcome
            .pages
            .iter()
            .find(|page| page.page_key == "guide/intro.md")
            .expect("intro page");
        let api = outcome
            .pages
            .iter()
            .find(|page| page.page_key == "api/reference.mdx")
            .expect("api page");
        let removed = outcome
            .pages
            .iter()
            .find(|page| page.page_key == "guide/removed.md")
            .expect("removed page");

        assert_eq!(
            intro.change_status,
            crate::models::PageChangeStatus::Unchanged
        );
        assert_eq!(api.change_status, crate::models::PageChangeStatus::New);
        assert_eq!(
            removed.change_status,
            crate::models::PageChangeStatus::Removed
        );

        fs::remove_dir_all(snapshot)?;
        fs::remove_dir_all(repo.path)?;
        Ok(())
    }

    struct RepoFixture {
        path: PathBuf,
        first_commit: String,
    }

    fn init_repo_fixture() -> Result<RepoFixture> {
        let repo = make_temp_dir("repo");
        git(&repo, &["init", "-b", "main"])?;
        git(&repo, &["config", "user.name", "Docsync Test"])?;
        git(&repo, &["config", "user.email", "docsync@example.com"])?;
        git(&repo, &["config", "core.autocrlf", "false"])?;

        fs::create_dir_all(repo.join("docs/guide"))?;
        fs::write(repo.join("docs/meta.json"), "{\"title\":\"Docs\"}")?;
        fs::write(repo.join("docs/guide/intro.md"), "# Intro\n")?;
        git(&repo, &["add", "."])?;
        git(&repo, &["commit", "-m", "first"])?;
        let first_commit = git_capture(&repo, &["rev-parse", "HEAD"])?
            .trim()
            .to_string();

        fs::create_dir_all(repo.join("docs/api"))?;
        fs::write(repo.join("docs/api/reference.mdx"), "# API\n")?;
        git(&repo, &["add", "."])?;
        git(&repo, &["commit", "-m", "second"])?;

        Ok(RepoFixture {
            path: repo,
            first_commit,
        })
    }

    fn git(repo: &Path, args: &[&str]) -> Result<()> {
        let output = Command::new("git").args(args).current_dir(repo).output()?;
        if output.status.success() {
            return Ok(());
        }
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
    }

    fn git_capture(repo: &Path, args: &[&str]) -> Result<String> {
        let output = Command::new("git").args(args).current_dir(repo).output()?;
        if !output.status.success() {
            anyhow::bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            )
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("current time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "docsync-test-{}-{}-{nanos}",
            std::process::id(),
            prefix
        ));
        fs::create_dir_all(&path).expect("temporary directory");
        path
    }
}
