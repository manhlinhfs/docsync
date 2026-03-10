use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::Serialize;

use crate::config::AppPaths;
use crate::models::SnapshotManifest;
use crate::network::build_http_client;

const DEFAULT_TELEGRAM_API_BASE: &str = "https://api.telegram.org";

#[derive(Debug, Serialize)]
pub struct TelegramNotifyResult {
    pub source_name: String,
    pub snapshot_label: String,
    pub chat_id: String,
    pub message_length: usize,
    pub api_endpoint: String,
}

pub fn send_telegram_snapshot_summary(
    paths: &AppPaths,
    source_name: &str,
    reference: Option<String>,
    bot_token: Option<String>,
    chat_id: Option<String>,
    proxy_url: Option<&str>,
) -> Result<TelegramNotifyResult> {
    let snapshot_dir = resolve_snapshot_dir(paths, source_name, reference.as_deref())?;
    let manifest = read_snapshot_manifest(&snapshot_dir)?;
    let bot_token = resolve_bot_token(bot_token)?;
    let chat_id = resolve_chat_id(chat_id)?;
    let message = build_snapshot_message(&manifest);
    let client = build_http_client(30, proxy_url)?;
    let api_base = env::var("DOCSYNC_TELEGRAM_API_BASE")
        .unwrap_or_else(|_| DEFAULT_TELEGRAM_API_BASE.to_string());
    let api_endpoint = format!(
        "{}/bot{}/sendMessage",
        api_base.trim_end_matches('/'),
        bot_token
    );
    send_message(&client, &api_endpoint, &chat_id, &message)?;

    Ok(TelegramNotifyResult {
        source_name: source_name.to_string(),
        snapshot_label: manifest.snapshot_label,
        chat_id,
        message_length: message.len(),
        api_endpoint,
    })
}

fn send_message(client: &Client, api_endpoint: &str, chat_id: &str, text: &str) -> Result<()> {
    #[derive(Serialize)]
    struct Payload<'a> {
        chat_id: &'a str,
        text: &'a str,
        disable_web_page_preview: bool,
    }

    let response = client
        .post(api_endpoint)
        .json(&Payload {
            chat_id,
            text,
            disable_web_page_preview: true,
        })
        .send()
        .with_context(|| format!("failed to send Telegram request to `{api_endpoint}`"))?;

    if !response.status().is_success() {
        bail!(
            "telegram API returned {} for `{api_endpoint}`",
            response.status()
        );
    }

    Ok(())
}

fn build_snapshot_message(manifest: &SnapshotManifest) -> String {
    let diff = manifest.diff.as_ref();
    let fetch = manifest.fetch.as_ref();
    let quality = fetch.and_then(|summary| summary.quality.as_ref());

    format!(
        "docsync snapshot\nsource: {source}\nsnapshot: {snapshot}\nstatus: {status}\npages: discovered={discovered} stored={stored} changed={changed} unchanged={unchanged} removed={removed}\nquality: high={high} medium={medium} low={low}\nentry: {entry}",
        source = manifest.source_name,
        snapshot = manifest.snapshot_label,
        status = manifest.status,
        discovered = manifest.discovery.frontier_count,
        stored = fetch.map(|value| value.stored_pages).unwrap_or(0),
        changed = diff
            .map(|value| value.new_pages + value.changed_pages)
            .unwrap_or(0),
        unchanged = diff.map(|value| value.unchanged_pages).unwrap_or(0),
        removed = diff.map(|value| value.removed_pages).unwrap_or(0),
        high = quality.map(|value| value.high_quality_pages).unwrap_or(0),
        medium = quality.map(|value| value.medium_quality_pages).unwrap_or(0),
        low = quality.map(|value| value.low_quality_pages).unwrap_or(0),
        entry = manifest.entry_url,
    )
}

fn resolve_bot_token(explicit: Option<String>) -> Result<String> {
    if let Some(value) = explicit {
        return Ok(value);
    }
    env::var("DOCSYNC_TELEGRAM_BOT_TOKEN")
        .context("Telegram bot token is required via --bot-token or DOCSYNC_TELEGRAM_BOT_TOKEN")
}

fn resolve_chat_id(explicit: Option<String>) -> Result<String> {
    if let Some(value) = explicit {
        return Ok(value);
    }
    env::var("DOCSYNC_TELEGRAM_CHAT_ID")
        .context("Telegram chat ID is required via --chat-id or DOCSYNC_TELEGRAM_CHAT_ID")
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::models::{
        DiscoverySummary, FetchSummary, SnapshotManifest, SourceKind, VersionStrategy,
    };
    use crate::probe::{DetectedInputKind, SuggestedMode};
    use crate::quality::QualitySummary;

    use super::build_snapshot_message;

    #[test]
    fn builds_telegram_summary_from_manifest() {
        let manifest = SnapshotManifest {
            schema_version: 7,
            created_at: "2026-03-09T00:00:00Z".to_string(),
            source_name: "openclaw".to_string(),
            entry_url: "https://docs.openclaw.ai/".to_string(),
            source_kind: SourceKind::Website,
            version_strategy: VersionStrategy::DateSnapshot,
            source_ref: "snapshot-20260309".to_string(),
            snapshot_label: "snapshot-20260309".to_string(),
            snapshot_dir: PathBuf::from("/tmp/demo"),
            status: "fetched".to_string(),
            previous_snapshot_label: None,
            detected_input_kind: DetectedInputKind::SiteRoot,
            suggested_mode: SuggestedMode::DiscoveryRoot,
            discovery: DiscoverySummary {
                manifest_path: PathBuf::from("/tmp/demo/discovery.json"),
                adapters: vec!["llms_txt".to_string()],
                frontier_count: 20,
                llms_index_url: None,
                llms_full_index_url: None,
                sitemap_count: 0,
            },
            git: None,
            fetch: Some(FetchSummary {
                attempted: 20,
                stored_pages: 18,
                skipped_pages: 2,
                reused_pages: 0,
                normalized_pages: 18,
                normalization_changed_pages: 18,
                quality: Some(QualitySummary {
                    pages_scored: 18,
                    high_quality_pages: 12,
                    medium_quality_pages: 4,
                    low_quality_pages: 2,
                    missing_title_pages: 1,
                    residual_markup_pages: 3,
                }),
                chunking: None,
                method_counts: std::collections::BTreeMap::new(),
            }),
            diff: Some(crate::models::DiffSummary {
                previous_snapshot_label: None,
                new_pages: 8,
                changed_pages: 3,
                unchanged_pages: 7,
                removed_pages: 1,
                import_candidates: 11,
            }),
            pages: Vec::new(),
            notes: Vec::new(),
        };

        let message = build_snapshot_message(&manifest);
        assert!(message.contains("source: openclaw"));
        assert!(message.contains("quality: high=12 medium=4 low=2"));
        assert!(message.contains("changed=11"));
    }
}
