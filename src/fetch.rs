use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use html2md::parse_html;
use reqwest::StatusCode;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Serialize;
use url::Url;

use crate::headless::{render_url, should_try_headless};
use crate::incremental::{PreviousPageState, sha256_hex};
use crate::models::{
    DiscoveredPage, FetchSummary, PageChangeStatus, PageManifestEntry, PageMetadata,
    SourceDefinition,
};
use crate::network::build_http_client;
use crate::normalize::normalize_markdown;
use crate::util::{ensure_directory, now_utc_rfc3339};

#[derive(Debug, Serialize)]
pub struct FetchOutcome {
    pub summary: FetchSummary,
    pub pages: Vec<PageManifestEntry>,
}

pub fn fetch_snapshot_pages(
    snapshot_dir: &Path,
    source: &SourceDefinition,
    source_ref: &str,
    snapshot_label: &str,
    frontier: &[DiscoveredPage],
    previous_pages: Option<&BTreeMap<String, PreviousPageState>>,
    proxy_url: Option<&str>,
    browser_cmd: Option<&str>,
) -> Result<FetchOutcome> {
    let client = build_http_client(30, proxy_url)?;

    let pages_root = snapshot_dir.join("pages");
    let raw_root = snapshot_dir.join("raw");
    ensure_directory(&pages_root)?;
    ensure_directory(&raw_root)?;

    let mut method_counts = BTreeMap::new();
    let mut manifest_pages = Vec::new();
    let mut stored_pages = 0usize;
    let mut skipped_pages = 0usize;
    let mut reused_pages = 0usize;
    let mut normalized_pages = 0usize;
    let mut normalization_changed_pages = 0usize;
    let mut seen_keys = BTreeSet::new();

    for page in frontier {
        let fetched = fetch_one_page(
            &client,
            &pages_root,
            &raw_root,
            source,
            source_ref,
            snapshot_label,
            page,
            previous_pages.and_then(|pages| pages.get(&page.url)),
            proxy_url,
            browser_cmd,
        )?;
        seen_keys.insert(fetched.page_key.clone());

        *method_counts
            .entry(fetched.fetch_method.clone())
            .or_insert(0usize) += 1;

        if fetched.page_path.is_some() {
            stored_pages += 1;
            if fetched.normalization.is_some() {
                normalized_pages += 1;
            }
            if fetched
                .normalization
                .as_ref()
                .is_some_and(|value| value.changed)
            {
                normalization_changed_pages += 1;
            }
        } else {
            skipped_pages += 1;
        }
        if fetched.reused_from_snapshot.is_some() {
            reused_pages += 1;
        }

        manifest_pages.push(fetched);
    }

    if let Some(previous_pages) = previous_pages {
        for previous in previous_pages.values() {
            if !seen_keys.contains(&previous.page_key) {
                seen_keys.insert(previous.page_key.clone());
                manifest_pages.push(PageManifestEntry {
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
                    etag: previous.etag.clone(),
                    last_modified: previous.last_modified.clone(),
                    byte_size: 0,
                    normalization: previous.normalization.clone(),
                });
            }
        }
    }

    Ok(FetchOutcome {
        summary: FetchSummary {
            attempted: frontier.len(),
            stored_pages,
            skipped_pages,
            reused_pages,
            normalized_pages,
            normalization_changed_pages,
            method_counts,
        },
        pages: manifest_pages,
    })
}

fn fetch_one_page(
    client: &reqwest::blocking::Client,
    pages_root: &Path,
    raw_root: &Path,
    source: &SourceDefinition,
    source_ref: &str,
    snapshot_label: &str,
    page: &DiscoveredPage,
    previous: Option<&PreviousPageState>,
    proxy_url: Option<&str>,
    browser_cmd: Option<&str>,
) -> Result<PageManifestEntry> {
    let requested_url = Url::parse(&page.url)
        .with_context(|| format!("failed to parse discovered page URL {}", page.url))?;

    let mut request = client.get(requested_url.clone()).header(
        ACCEPT,
        HeaderValue::from_static("text/markdown, text/plain;q=0.9, */*;q=0.1"),
    );
    if let Some(previous) = previous {
        if let Some(etag) = previous.etag.as_deref() {
            request = request.header("if-none-match", etag);
        }
        if let Some(last_modified) = previous.last_modified.as_deref() {
            request = request.header("if-modified-since", last_modified);
        }
    }

    let response = request
        .send()
        .with_context(|| format!("failed to fetch {}", page.url))?;
    let status_code = response.status().as_u16();
    let final_url = response.url().clone();
    let headers = response.headers().clone();
    let content_type = header_value(&headers, CONTENT_TYPE);
    let etag = header_value_str(&headers, "etag");
    let last_modified = header_value_str(&headers, "last-modified");
    let page_key = final_url.to_string();
    let stem = storage_stem(&final_url);
    let page_path = markdown_page_path(pages_root, &stem);
    let metadata_path = metadata_path_for(&page_path);
    let raw_path = raw_root.join(format!("{stem}.body"));
    let rendered_raw_path = raw_root.join(format!("{stem}.rendered.html"));

    if response.status() == StatusCode::NOT_MODIFIED {
        if let Some(previous) = previous {
            let content_hash = previous.content_hash.clone().or_else(|| {
                previous
                    .page_path
                    .as_deref()
                    .and_then(|path| fs::read(path).ok())
                    .map(|bytes| sha256_hex(&bytes))
            });
            let byte_size = if let Some(previous_page_path) = previous.page_path.as_deref() {
                copy_artifact(previous_page_path, &page_path)?;
                page_path
                    .metadata()
                    .with_context(|| format!("failed to stat {}", page_path.display()))?
                    .len()
            } else {
                0
            };
            let raw_output_path = if let Some(previous_raw_path) = previous.raw_path.as_deref() {
                copy_artifact(previous_raw_path, &raw_path)?;
                Some(raw_path.clone())
            } else {
                None
            };
            let rendered_raw_output_path =
                if let Some(previous_rendered_raw_path) = previous.rendered_raw_path.as_deref() {
                    copy_artifact(previous_rendered_raw_path, &rendered_raw_path)?;
                    Some(rendered_raw_path.clone())
                } else {
                    None
                };
            if page_path.is_file() {
                let metadata = PageMetadata {
                    schema_version: 3,
                    fetched_at: now_utc_rfc3339(),
                    source_name: source.name.clone(),
                    snapshot_label: snapshot_label.to_string(),
                    source_ref: source_ref.to_string(),
                    page_key: previous.page_key.clone(),
                    requested_url: requested_url.to_string(),
                    final_url: final_url.to_string(),
                    fetch_method: previous.fetch_method.clone(),
                    discovered_from: page.discovered_from,
                    content_type: previous.content_type.clone(),
                    status_code,
                    byte_size,
                    sha256: content_hash.clone().unwrap_or_default(),
                    raw_sha256: previous.raw_hash.clone(),
                    etag: previous.etag.clone(),
                    last_modified: previous.last_modified.clone(),
                    x_markdown_tokens: None,
                    x_original_tokens: None,
                    content_signal: None,
                    page_path: page_path.clone(),
                    raw_path: raw_output_path
                        .clone()
                        .unwrap_or_else(|| raw_root.join(format!("{stem}.missing.body"))),
                    rendered_raw_path: rendered_raw_output_path.clone(),
                    normalization: previous.normalization.clone(),
                };
                write_json(&metadata_path, &metadata)?;
            }

            return Ok(PageManifestEntry {
                page_key: previous.page_key.clone(),
                url: requested_url.to_string(),
                final_url: final_url.to_string(),
                fetch_method: previous.fetch_method.clone(),
                status: "reused_not_modified".to_string(),
                change_status: PageChangeStatus::Unchanged,
                reused_from_snapshot: Some(previous.snapshot_label.clone()),
                page_path: if page_path.is_file() {
                    Some(page_path)
                } else {
                    None
                },
                metadata_path: if metadata_path.is_file() {
                    Some(metadata_path)
                } else {
                    None
                },
                raw_path: raw_output_path,
                rendered_raw_path: rendered_raw_output_path,
                content_type: previous.content_type.clone(),
                sha256: content_hash,
                raw_sha256: previous.raw_hash.clone(),
                etag: previous.etag.clone(),
                last_modified: previous.last_modified.clone(),
                byte_size,
                normalization: previous.normalization.clone(),
            });
        }
    }

    let body = response
        .bytes()
        .with_context(|| format!("failed to read {}", page.url))?;
    let body_vec = body.to_vec();
    let raw_sha256 = sha256_hex(&body_vec);
    write_bytes(&raw_path, &body_vec)?;

    let markdown_supported = is_markdown_response(&final_url, content_type.as_deref());
    if markdown_supported && (200..300).contains(&status_code) {
        let markdown = String::from_utf8_lossy(&body_vec);
        let normalized = normalize_markdown(&markdown);
        let normalized_bytes = normalized.markdown.as_bytes();
        let byte_size = normalized_bytes.len() as u64;
        write_bytes(&page_path, normalized_bytes)?;
        let content_hash = sha256_hex(normalized_bytes);
        let change_status = compare_change(previous, &content_hash);
        let metadata = PageMetadata {
            schema_version: 3,
            fetched_at: now_utc_rfc3339(),
            source_name: source.name.clone(),
            snapshot_label: snapshot_label.to_string(),
            source_ref: source_ref.to_string(),
            page_key: page_key.clone(),
            requested_url: requested_url.to_string(),
            final_url: final_url.to_string(),
            fetch_method: "markdown_negotiation".to_string(),
            discovered_from: page.discovered_from,
            content_type: content_type.clone(),
            status_code,
            byte_size,
            sha256: content_hash.clone(),
            raw_sha256: Some(raw_sha256.clone()),
            etag: etag.clone(),
            last_modified: last_modified.clone(),
            x_markdown_tokens: parse_u32_header(&headers, "x-markdown-tokens"),
            x_original_tokens: parse_u32_header(&headers, "x-original-tokens"),
            content_signal: header_value_str(&headers, "content-signal"),
            page_path: page_path.clone(),
            raw_path: raw_path.clone(),
            rendered_raw_path: None,
            normalization: Some(normalized.summary.clone()),
        };
        write_json(&metadata_path, &metadata)?;

        return Ok(PageManifestEntry {
            page_key,
            url: requested_url.to_string(),
            final_url: final_url.to_string(),
            fetch_method: "markdown_negotiation".to_string(),
            status: "stored".to_string(),
            change_status,
            reused_from_snapshot: None,
            page_path: Some(page_path),
            metadata_path: Some(metadata_path),
            raw_path: Some(raw_path),
            rendered_raw_path: None,
            content_type,
            sha256: Some(content_hash),
            raw_sha256: Some(raw_sha256),
            etag,
            last_modified,
            byte_size,
            normalization: Some(normalized.summary),
        });
    }

    if is_html_response(content_type.as_deref()) && (200..300).contains(&status_code) {
        let html = String::from_utf8_lossy(&body_vec);
        let fallback_markdown = html_to_markdown(&body_vec);
        let mut final_markdown = fallback_markdown.clone();
        let mut fetch_method = "html_fallback".to_string();
        let mut rendered_raw_output_path = None;

        if let Some(browser_cmd) = browser_cmd {
            if should_try_headless(&html, &fallback_markdown) {
                if let Ok(rendered_html) = render_url(browser_cmd, final_url.as_str(), proxy_url) {
                    write_bytes(&rendered_raw_path, rendered_html.as_bytes())?;
                    final_markdown = html_to_markdown(rendered_html.as_bytes());
                    fetch_method = "headless_html_fallback".to_string();
                    rendered_raw_output_path = Some(rendered_raw_path.clone());
                }
            }
        }

        let normalized = normalize_markdown(&final_markdown);
        let byte_size = normalized.markdown.len() as u64;
        write_bytes(&page_path, normalized.markdown.as_bytes())?;
        let content_hash = sha256_hex(normalized.markdown.as_bytes());
        let change_status = compare_change(previous, &content_hash);
        let metadata = PageMetadata {
            schema_version: 3,
            fetched_at: now_utc_rfc3339(),
            source_name: source.name.clone(),
            snapshot_label: snapshot_label.to_string(),
            source_ref: source_ref.to_string(),
            page_key: page_key.clone(),
            requested_url: requested_url.to_string(),
            final_url: final_url.to_string(),
            fetch_method: fetch_method.clone(),
            discovered_from: page.discovered_from,
            content_type: content_type.clone(),
            status_code,
            byte_size,
            sha256: content_hash.clone(),
            raw_sha256: Some(raw_sha256.clone()),
            etag: etag.clone(),
            last_modified: last_modified.clone(),
            x_markdown_tokens: parse_u32_header(&headers, "x-markdown-tokens"),
            x_original_tokens: parse_u32_header(&headers, "x-original-tokens"),
            content_signal: header_value_str(&headers, "content-signal"),
            page_path: page_path.clone(),
            raw_path: raw_path.clone(),
            rendered_raw_path: rendered_raw_output_path.clone(),
            normalization: Some(normalized.summary.clone()),
        };
        write_json(&metadata_path, &metadata)?;

        return Ok(PageManifestEntry {
            page_key,
            url: requested_url.to_string(),
            final_url: final_url.to_string(),
            fetch_method,
            status: "stored".to_string(),
            change_status,
            reused_from_snapshot: None,
            page_path: Some(page_path),
            metadata_path: Some(metadata_path),
            raw_path: Some(raw_path),
            rendered_raw_path: rendered_raw_output_path,
            content_type,
            sha256: Some(content_hash),
            raw_sha256: Some(raw_sha256),
            etag,
            last_modified,
            byte_size,
            normalization: Some(normalized.summary),
        });
    }

    let byte_size = body_vec.len() as u64;
    Ok(PageManifestEntry {
        page_key,
        url: requested_url.to_string(),
        final_url: final_url.to_string(),
        fetch_method: "markdown_negotiation".to_string(),
        status: if (200..300).contains(&status_code) {
            "skipped_non_markdown".to_string()
        } else {
            format!("skipped_http_{status_code}")
        },
        change_status: PageChangeStatus::Unknown,
        reused_from_snapshot: None,
        page_path: None,
        metadata_path: None,
        raw_path: Some(raw_path),
        rendered_raw_path: None,
        content_type,
        sha256: None,
        raw_sha256: Some(raw_sha256),
        etag,
        last_modified,
        byte_size,
        normalization: None,
    })
}

fn storage_stem(url: &Url) -> String {
    let mut stem_parts = vec![sanitize_segment(url.host_str().unwrap_or("unknown-host"))];
    let path = url.path().trim_matches('/');

    if path.is_empty() {
        stem_parts.push("index".to_string());
    } else {
        for segment in path.split('/') {
            stem_parts.push(sanitize_segment(segment));
        }
    }

    let mut stem = stem_parts.join("/");

    if let Some(query) = url.query() {
        let query_hash = short_hash(query.as_bytes());
        stem.push_str(&format!("--q-{query_hash}"));
    }

    stem
}

fn markdown_page_path(root: &Path, stem: &str) -> PathBuf {
    if stem.ends_with(".md") {
        root.join(stem)
    } else if stem.ends_with(".mdx") {
        root.join(format!("{}{}", stem.trim_end_matches(".mdx"), ".md"))
    } else {
        root.join(format!("{stem}.md"))
    }
}

fn metadata_path_for(page_path: &Path) -> PathBuf {
    let file_name = page_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("page.md");
    page_path.with_file_name(format!("{file_name}.json"))
}

fn sanitize_segment(value: &str) -> String {
    let mut cleaned = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();

    while cleaned.contains("--") {
        cleaned = cleaned.replace("--", "-");
    }

    let cleaned = cleaned.trim_matches('-');
    if cleaned.is_empty() {
        "index".to_string()
    } else {
        cleaned.to_string()
    }
}

fn is_markdown_response(final_url: &Url, content_type: Option<&str>) -> bool {
    let content_type = content_type.unwrap_or_default().to_ascii_lowercase();
    let path = final_url.path().to_ascii_lowercase();

    content_type.starts_with("text/markdown")
        || path.ends_with(".md")
        || path.ends_with(".mdx")
        || (content_type.starts_with("text/plain") && !path.ends_with(".txt"))
}

fn is_html_response(content_type: Option<&str>) -> bool {
    content_type
        .unwrap_or_default()
        .to_ascii_lowercase()
        .starts_with("text/html")
}

fn html_to_markdown(bytes: &[u8]) -> String {
    let html = String::from_utf8_lossy(bytes);
    let markdown = parse_html(&html);
    format!("{}\n", markdown.trim())
}

fn short_hash(bytes: &[u8]) -> String {
    sha256_hex(bytes)[..8].to_string()
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_directory(parent)?;
    }
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
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

fn copy_artifact(source: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        ensure_directory(parent)?;
    }
    fs::copy(source, target).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            target.display()
        )
    })?;
    Ok(())
}

fn compare_change(previous: Option<&PreviousPageState>, current_hash: &str) -> PageChangeStatus {
    match previous.and_then(|value| value.content_hash.as_deref()) {
        None => PageChangeStatus::New,
        Some(previous_hash) if previous_hash == current_hash => PageChangeStatus::Unchanged,
        Some(_) => PageChangeStatus::Changed,
    }
}

fn parse_u32_header(headers: &HeaderMap, name: &str) -> Option<u32> {
    header_value_str(headers, name).and_then(|value| value.parse::<u32>().ok())
}

fn header_value(headers: &HeaderMap, key: reqwest::header::HeaderName) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

fn header_value_str(headers: &HeaderMap, key: &str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{
        html_to_markdown, is_html_response, is_markdown_response, markdown_page_path,
        metadata_path_for, storage_stem,
    };
    use std::path::Path;
    use url::Url;

    #[test]
    fn storage_stem_preserves_host_and_path() {
        let url = Url::parse("https://docs.example.com/guides/install").expect("URL");
        assert_eq!(storage_stem(&url), "docs.example.com/guides/install");
    }

    #[test]
    fn storage_stem_hashes_query_strings() {
        let url = Url::parse("https://docs.example.com/search?page=2").expect("URL");
        assert!(storage_stem(&url).starts_with("docs.example.com/search--q-"));
    }

    #[test]
    fn detects_markdown_from_markdown_content_type() {
        let url = Url::parse("https://docs.example.com/install").expect("URL");
        assert!(is_markdown_response(
            &url,
            Some("text/markdown; charset=utf-8")
        ));
    }

    #[test]
    fn detects_markdown_from_mdx_extension() {
        let url = Url::parse("https://docs.example.com/install.mdx").expect("URL");
        assert!(is_markdown_response(&url, Some("text/plain")));
    }

    #[test]
    fn rejects_html_responses() {
        let url = Url::parse("https://docs.example.com/install").expect("URL");
        assert!(!is_markdown_response(
            &url,
            Some("text/html; charset=utf-8")
        ));
    }

    #[test]
    fn detects_html_content_type() {
        assert!(is_html_response(Some("text/html; charset=utf-8")));
        assert!(!is_html_response(Some("text/plain")));
    }

    #[test]
    fn converts_html_to_markdown() {
        let markdown = html_to_markdown(
            br#"<html><body><h1>Install</h1><p>Hello <strong>world</strong>.</p></body></html>"#,
        );
        assert!(markdown.contains("Install"));
        assert!(markdown.contains("world"));
        assert!(!markdown.contains("<h1>"));
    }

    #[test]
    fn preserves_markdown_filenames_without_double_extension() {
        let path = markdown_page_path(Path::new("/tmp/pages"), "docs.example.com/intro.md");
        assert_eq!(
            path,
            Path::new("/tmp/pages").join("docs.example.com/intro.md")
        );
    }

    #[test]
    fn metadata_path_uses_page_filename() {
        let metadata = metadata_path_for(Path::new("/tmp/pages/docs.example.com/intro.md"));
        assert_eq!(
            metadata,
            Path::new("/tmp/pages").join("docs.example.com/intro.md.json")
        );
    }
}
