use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use html2md::parse_html;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Serialize;
use sha2::{Digest, Sha256};
use url::Url;

use crate::models::{
    DiscoveredPage, FetchSummary, PageManifestEntry, PageMetadata, SourceDefinition,
};
use crate::network::build_http_client;
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
    proxy_url: Option<&str>,
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

    for page in frontier {
        let fetched = fetch_one_page(
            &client,
            &pages_root,
            &raw_root,
            source,
            source_ref,
            snapshot_label,
            page,
        )?;

        *method_counts
            .entry(fetched.fetch_method.clone())
            .or_insert(0usize) += 1;

        if fetched.page_path.is_some() {
            stored_pages += 1;
        } else {
            skipped_pages += 1;
        }

        manifest_pages.push(fetched);
    }

    Ok(FetchOutcome {
        summary: FetchSummary {
            attempted: frontier.len(),
            stored_pages,
            skipped_pages,
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
) -> Result<PageManifestEntry> {
    let requested_url = Url::parse(&page.url)
        .with_context(|| format!("failed to parse discovered page URL {}", page.url))?;

    let response = client
        .get(requested_url.clone())
        .header(
            ACCEPT,
            HeaderValue::from_static("text/markdown, text/plain;q=0.9, */*;q=0.1"),
        )
        .send()
        .with_context(|| format!("failed to fetch {}", page.url))?;

    let status_code = response.status().as_u16();
    let final_url = response.url().clone();
    let headers = response.headers().clone();
    let content_type = header_value(&headers, CONTENT_TYPE);
    let body = response
        .bytes()
        .with_context(|| format!("failed to read {}", page.url))?;
    let body_vec = body.to_vec();
    let byte_size = body_vec.len() as u64;
    let sha256 = sha256_hex(&body_vec);

    let stem = storage_stem(&final_url);
    let raw_path = raw_root.join(format!("{stem}.body"));
    write_bytes(&raw_path, &body_vec)?;

    let markdown_supported = is_markdown_response(&final_url, content_type.as_deref());

    if markdown_supported && (200..300).contains(&status_code) {
        let page_path = markdown_page_path(pages_root, &stem);
        write_bytes(&page_path, &body_vec)?;
        let metadata_path = metadata_path_for(&page_path);
        let metadata = PageMetadata {
            schema_version: 1,
            fetched_at: now_utc_rfc3339(),
            source_name: source.name.clone(),
            snapshot_label: snapshot_label.to_string(),
            source_ref: source_ref.to_string(),
            requested_url: requested_url.to_string(),
            final_url: final_url.to_string(),
            fetch_method: "markdown_negotiation".to_string(),
            discovered_from: page.discovered_from,
            content_type: content_type.clone(),
            status_code,
            byte_size,
            sha256: sha256.clone(),
            x_markdown_tokens: parse_u32_header(&headers, "x-markdown-tokens"),
            x_original_tokens: parse_u32_header(&headers, "x-original-tokens"),
            content_signal: header_value_str(&headers, "content-signal"),
            page_path: page_path.clone(),
            raw_path: raw_path.clone(),
        };
        write_json(&metadata_path, &metadata)?;

        return Ok(PageManifestEntry {
            url: requested_url.to_string(),
            final_url: final_url.to_string(),
            fetch_method: "markdown_negotiation".to_string(),
            status: "stored".to_string(),
            page_path: Some(page_path),
            metadata_path: Some(metadata_path),
            raw_path,
            content_type,
            sha256: Some(sha256),
            byte_size,
        });
    }

    if is_html_response(content_type.as_deref()) && (200..300).contains(&status_code) {
        let page_path = markdown_page_path(pages_root, &stem);
        let converted_markdown = html_to_markdown(&body_vec);
        write_bytes(&page_path, converted_markdown.as_bytes())?;
        let metadata_path = metadata_path_for(&page_path);
        let metadata = PageMetadata {
            schema_version: 1,
            fetched_at: now_utc_rfc3339(),
            source_name: source.name.clone(),
            snapshot_label: snapshot_label.to_string(),
            source_ref: source_ref.to_string(),
            requested_url: requested_url.to_string(),
            final_url: final_url.to_string(),
            fetch_method: "html_fallback".to_string(),
            discovered_from: page.discovered_from,
            content_type: content_type.clone(),
            status_code,
            byte_size,
            sha256: sha256.clone(),
            x_markdown_tokens: parse_u32_header(&headers, "x-markdown-tokens"),
            x_original_tokens: parse_u32_header(&headers, "x-original-tokens"),
            content_signal: header_value_str(&headers, "content-signal"),
            page_path: page_path.clone(),
            raw_path: raw_path.clone(),
        };
        write_json(&metadata_path, &metadata)?;

        return Ok(PageManifestEntry {
            url: requested_url.to_string(),
            final_url: final_url.to_string(),
            fetch_method: "html_fallback".to_string(),
            status: "stored".to_string(),
            page_path: Some(page_path),
            metadata_path: Some(metadata_path),
            raw_path,
            content_type,
            sha256: Some(sha256),
            byte_size,
        });
    }

    Ok(PageManifestEntry {
        url: requested_url.to_string(),
        final_url: final_url.to_string(),
        fetch_method: "markdown_negotiation".to_string(),
        status: if (200..300).contains(&status_code) {
            "skipped_non_markdown".to_string()
        } else {
            format!("skipped_http_{status_code}")
        },
        page_path: None,
        metadata_path: None,
        raw_path,
        content_type,
        sha256: Some(sha256),
        byte_size,
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
    let digest = Sha256::digest(bytes);
    hex_encode(&digest[..4])
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
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
            path.to_string_lossy(),
            "/tmp/pages/docs.example.com/intro.md"
        );
    }

    #[test]
    fn metadata_path_uses_page_filename() {
        let metadata = metadata_path_for(Path::new("/tmp/pages/docs.example.com/intro.md"));
        assert_eq!(
            metadata.to_string_lossy(),
            "/tmp/pages/docs.example.com/intro.md.json"
        );
    }
}
