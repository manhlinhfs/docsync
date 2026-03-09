use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::config::AppPaths;
use crate::models::{PageManifestEntry, SnapshotManifest};
use crate::normalize::NormalizationSummary;

#[derive(Debug, Clone)]
pub struct PreviousSnapshot {
    pub label: String,
    pub dir: PathBuf,
    pub manifest: SnapshotManifest,
}

#[derive(Debug, Clone)]
pub struct PreviousPageState {
    pub snapshot_label: String,
    pub page_key: String,
    pub content_hash: Option<String>,
    pub raw_hash: Option<String>,
    pub fetch_method: String,
    pub page_path: Option<PathBuf>,
    pub raw_path: Option<PathBuf>,
    pub rendered_raw_path: Option<PathBuf>,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub normalization: Option<NormalizationSummary>,
}

pub fn find_previous_snapshot(
    paths: &AppPaths,
    source_name: &str,
    current_label: &str,
) -> Result<Option<PreviousSnapshot>> {
    let source_dir = paths.snapshots_dir.join(source_name);
    if !source_dir.is_dir() {
        return Ok(None);
    }

    let mut labels = fs::read_dir(&source_dir)
        .with_context(|| format!("failed to read {}", source_dir.display()))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                entry.file_name().into_string().ok()
            } else {
                None
            }
        })
        .filter(|label| label != current_label)
        .collect::<Vec<_>>();
    labels.sort();

    let Some(label) = labels.pop() else {
        return Ok(None);
    };
    let dir = source_dir.join(&label);
    let manifest_path = dir.join("manifest.json");
    if !manifest_path.is_file() {
        return Ok(None);
    }

    let body = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest = serde_json::from_str::<SnapshotManifest>(&body)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    Ok(Some(PreviousSnapshot {
        label,
        dir,
        manifest,
    }))
}

pub fn previous_page_index(
    snapshot: &PreviousSnapshot,
) -> Result<BTreeMap<String, PreviousPageState>> {
    let mut index = BTreeMap::new();

    for entry in &snapshot.manifest.pages {
        let key = manifest_page_key(entry, Some(&snapshot.dir));
        let state = PreviousPageState {
            snapshot_label: snapshot.label.clone(),
            page_key: key.clone(),
            content_hash: manifest_content_hash(entry)?,
            raw_hash: entry.raw_sha256.clone(),
            fetch_method: entry.fetch_method.clone(),
            page_path: entry.page_path.clone(),
            raw_path: entry.raw_path.clone(),
            rendered_raw_path: entry.rendered_raw_path.clone(),
            content_type: entry.content_type.clone(),
            etag: entry.etag.clone(),
            last_modified: entry.last_modified.clone(),
            normalization: entry.normalization.clone(),
        };
        index.insert(key.clone(), state.clone());
        if !entry.url.is_empty() {
            index.insert(entry.url.clone(), state.clone());
        }
        if !entry.final_url.is_empty() {
            index.insert(entry.final_url.clone(), state);
        }
    }

    Ok(index)
}

pub fn manifest_page_key(entry: &PageManifestEntry, snapshot_dir: Option<&Path>) -> String {
    if !entry.page_key.is_empty() {
        return entry.page_key.clone();
    }

    if entry.fetch_method == "git_checkout" {
        if let Some(page_path) = entry.page_path.as_deref() {
            if let Some(root) = snapshot_dir {
                let pages_root = root.join("pages");
                if let Ok(relative) = page_path.strip_prefix(&pages_root) {
                    return relative.to_string_lossy().replace('\\', "/");
                }
            }
        }
    }

    if !entry.final_url.is_empty() {
        entry.final_url.clone()
    } else {
        entry.url.clone()
    }
}

pub fn manifest_content_hash(entry: &PageManifestEntry) -> Result<Option<String>> {
    if let Some(hash) = entry.sha256.clone() {
        return Ok(Some(hash));
    }

    if let Some(path) = entry.page_path.as_deref() {
        if path.is_file() {
            return Ok(Some(sha256_hex(
                &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
            )));
        }
    }

    Ok(None)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
