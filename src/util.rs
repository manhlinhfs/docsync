use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use url::Url;

pub fn now_utc_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub fn today_compact() -> String {
    Utc::now().format("%Y%m%d").to_string()
}

pub fn normalize_url(input: &str) -> Result<Url> {
    let trimmed = input.trim();
    let candidate = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };

    let url = Url::parse(&candidate).with_context(|| format!("invalid URL `{input}`"))?;
    if url.host_str().is_none() {
        bail!("URL `{input}` is missing a host");
    }

    Ok(url)
}

pub fn validate_source_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("source name cannot be empty");
    }

    if !name
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        bail!("source name must use only lowercase ASCII letters, digits, '-' or '_'");
    }

    Ok(())
}

pub fn sanitize_ref_label(value: &str) -> String {
    let mut label = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();

    while label.contains("--") {
        label = label.replace("--", "-");
    }

    label.trim_matches('-').to_string()
}

pub fn dedupe_tags(tags: Vec<String>) -> Vec<String> {
    let mut set = BTreeSet::new();
    for tag in tags {
        let trimmed = tag.trim().to_ascii_lowercase();
        if !trimmed.is_empty() {
            set.insert(trimmed);
        }
    }
    set.into_iter().collect()
}

pub fn ensure_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{normalize_url, sanitize_ref_label, validate_source_name};

    #[test]
    fn normalizes_https_url() {
        let url = normalize_url("docs.postiz.com").expect("normalized URL");
        assert_eq!(url.as_str(), "https://docs.postiz.com/");
    }

    #[test]
    fn sanitizes_ref_labels() {
        assert_eq!(sanitize_ref_label("main@2026/03/07"), "main-2026-03-07");
    }

    #[test]
    fn validates_source_names() {
        validate_source_name("postiz-docs").expect("valid source name");
        assert!(validate_source_name("Postiz Docs").is_err());
    }
}
