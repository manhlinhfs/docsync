use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::config::AppPaths;
use crate::normalize::NormalizationSummary;
use crate::probe::{DetectedInputKind, SuggestedMode};
use crate::quality::{PageQualitySummary, QualitySummary};
use crate::util::now_utc_rfc3339;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub default_proxy_url: Option<String>,
    #[serde(default)]
    pub default_browser_cmd: Option<String>,
    pub sources: BTreeMap<String, SourceDefinition>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let now = now_utc_rfc3339();
        Self {
            schema_version: 2,
            created_at: now.clone(),
            updated_at: now,
            default_proxy_url: None,
            default_browser_cmd: None,
            sources: BTreeMap::new(),
        }
    }
}

impl AppConfig {
    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&paths.config_file, json)
            .with_context(|| format!("failed to write {}", paths.config_file.display()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDefinition {
    pub name: String,
    pub entry_url: String,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub browser_cmd: Option<String>,
    pub source_kind: SourceKind,
    pub repo_url: Option<String>,
    pub docs_path: Option<String>,
    pub default_ref: Option<String>,
    pub version_strategy: VersionStrategy,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewSource {
    pub name: String,
    pub entry_url: String,
    pub proxy_url: Option<String>,
    pub browser_cmd: Option<String>,
    pub source_kind: SourceKind,
    pub repo_url: Option<String>,
    pub docs_path: Option<String>,
    pub default_ref: Option<String>,
    pub version_strategy: VersionStrategy,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Auto,
    GitDocs,
    Website,
    Mixed,
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Auto => "auto",
            Self::GitDocs => "git_docs",
            Self::Website => "website",
            Self::Mixed => "mixed",
        };
        write!(f, "{value}")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum VersionStrategy {
    Auto,
    GitRef,
    DocsPrefix,
    DateSnapshot,
}

impl fmt::Display for VersionStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Auto => "auto",
            Self::GitRef => "git_ref",
            Self::DocsPrefix => "docs_prefix",
            Self::DateSnapshot => "date_snapshot",
        };
        write!(f, "{value}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub schema_version: u32,
    pub created_at: String,
    pub source_name: String,
    pub entry_url: String,
    pub source_kind: SourceKind,
    pub version_strategy: VersionStrategy,
    pub source_ref: String,
    pub snapshot_label: String,
    pub snapshot_dir: PathBuf,
    pub status: String,
    #[serde(default)]
    pub previous_snapshot_label: Option<String>,
    pub detected_input_kind: DetectedInputKind,
    pub suggested_mode: SuggestedMode,
    pub discovery: DiscoverySummary,
    pub git: Option<GitSummary>,
    pub fetch: Option<FetchSummary>,
    #[serde(default)]
    pub diff: Option<DiffSummary>,
    pub pages: Vec<PageManifestEntry>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverySummary {
    pub manifest_path: PathBuf,
    pub adapters: Vec<String>,
    pub frontier_count: usize,
    pub llms_index_url: Option<String>,
    pub llms_full_index_url: Option<String>,
    pub sitemap_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchSummary {
    pub attempted: usize,
    pub stored_pages: usize,
    pub skipped_pages: usize,
    #[serde(default)]
    pub reused_pages: usize,
    #[serde(default)]
    pub normalized_pages: usize,
    #[serde(default)]
    pub normalization_changed_pages: usize,
    #[serde(default)]
    pub quality: Option<QualitySummary>,
    pub method_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    pub previous_snapshot_label: Option<String>,
    pub new_pages: usize,
    pub changed_pages: usize,
    pub unchanged_pages: usize,
    pub removed_pages: usize,
    pub import_candidates: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitSummary {
    pub repo_url: String,
    pub requested_ref: String,
    pub resolved_ref: String,
    pub docs_path: String,
    pub detected_docs_path: bool,
    pub nav_manifests: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageManifestEntry {
    #[serde(default)]
    pub page_key: String,
    pub url: String,
    pub final_url: String,
    pub fetch_method: String,
    pub status: String,
    #[serde(default)]
    pub change_status: PageChangeStatus,
    #[serde(default)]
    pub reused_from_snapshot: Option<String>,
    pub page_path: Option<PathBuf>,
    pub metadata_path: Option<PathBuf>,
    #[serde(default)]
    pub raw_path: Option<PathBuf>,
    #[serde(default)]
    pub rendered_raw_path: Option<PathBuf>,
    pub content_type: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub raw_sha256: Option<String>,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub last_modified: Option<String>,
    pub byte_size: u64,
    #[serde(default)]
    pub normalization: Option<NormalizationSummary>,
    #[serde(default)]
    pub quality: Option<PageQualitySummary>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PageChangeStatus {
    #[default]
    Unknown,
    New,
    Changed,
    Unchanged,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryManifest {
    pub schema_version: u32,
    pub created_at: String,
    pub source_name: String,
    pub entry_url: String,
    pub final_url: String,
    pub source_ref: String,
    pub detected_input_kind: DetectedInputKind,
    pub suggested_mode: SuggestedMode,
    pub adapters: Vec<String>,
    pub llms_index_url: Option<String>,
    pub llms_full_index_url: Option<String>,
    pub sitemap_urls: Vec<String>,
    pub frontier: Vec<DiscoveredPage>,
    pub notes: Vec<String>,
}

impl DiscoveryManifest {
    pub fn summary(&self, manifest_path: PathBuf) -> DiscoverySummary {
        DiscoverySummary {
            manifest_path,
            adapters: self.adapters.clone(),
            frontier_count: self.frontier.len(),
            llms_index_url: self.llms_index_url.clone(),
            llms_full_index_url: self.llms_full_index_url.clone(),
            sitemap_count: self.sitemap_urls.len(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPage {
    pub url: String,
    pub discovered_from: DiscoveryOrigin,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryOrigin {
    SeedPage,
    LlmsTxt,
    LlmsFullTxt,
    Sitemap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMetadata {
    pub schema_version: u32,
    pub fetched_at: String,
    pub source_name: String,
    pub snapshot_label: String,
    pub source_ref: String,
    pub page_key: String,
    pub requested_url: String,
    pub final_url: String,
    pub fetch_method: String,
    pub discovered_from: DiscoveryOrigin,
    pub content_type: Option<String>,
    pub status_code: u16,
    pub byte_size: u64,
    pub sha256: String,
    #[serde(default)]
    pub raw_sha256: Option<String>,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub last_modified: Option<String>,
    pub x_markdown_tokens: Option<u32>,
    pub x_original_tokens: Option<u32>,
    pub content_signal: Option<String>,
    pub page_path: PathBuf,
    pub raw_path: PathBuf,
    #[serde(default)]
    pub rendered_raw_path: Option<PathBuf>,
    #[serde(default)]
    pub normalization: Option<NormalizationSummary>,
    #[serde(default)]
    pub quality: Option<PageQualitySummary>,
}
