use anyhow::{Context, Result};
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue, LINK};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::network::build_http_client;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeReport {
    pub requested_url: String,
    pub final_url: String,
    pub detected_input_kind: DetectedInputKind,
    pub suggested_mode: SuggestedMode,
    pub markdown_supported: bool,
    pub markdown: MarkdownReport,
    pub llms: LlmsReport,
    pub robots: RobotsReport,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownReport {
    pub content_type: Option<String>,
    pub x_markdown_tokens: Option<u32>,
    pub x_original_tokens: Option<u32>,
    pub content_signal: Option<String>,
    pub link_header: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmsReport {
    pub discovered_from: Option<String>,
    pub index_url: Option<String>,
    pub full_index_url: Option<String>,
    pub available: bool,
    pub page_links: usize,
    pub preview: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotsReport {
    pub robots_url: String,
    pub robots_status: Option<u16>,
    pub sitemaps: Vec<String>,
    pub first_sitemap_url_count: Option<usize>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectedInputKind {
    SiteRoot,
    ContentPage,
    MarkdownEndpoint,
    LlmsTxt,
    LlmsFullTxt,
    Sitemap,
    RobotsTxt,
    MarkdownFile,
    HtmlFile,
    OpenApiSpec,
    JsonFile,
    PdfFile,
    OfficeDocument,
    Unknown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestedMode {
    DiscoveryRoot,
    SingleDocument,
    SingleFile,
    HybridSeed,
}

pub fn probe_url_with_proxy(input: &str, proxy_url: Option<&str>) -> Result<ProbeReport> {
    let requested = crate::util::normalize_url(input)?;
    let client = build_http_client(20, proxy_url)?;

    let markdown_response = client
        .get(requested.clone())
        .header(
            ACCEPT,
            HeaderValue::from_static("text/markdown, text/plain;q=0.9, */*;q=0.1"),
        )
        .send()
        .with_context(|| format!("failed to probe {}", requested))?;

    let final_url = markdown_response.url().clone();
    let headers = markdown_response.headers().clone();
    let content_type = header_value(&headers, CONTENT_TYPE);
    let markdown_supported = content_type
        .as_deref()
        .is_some_and(|value| value.starts_with("text/markdown"));
    let detected_input_kind = detect_input_kind(
        &final_url,
        requested.path() == "/" || requested.path().is_empty(),
        content_type.as_deref(),
        markdown_supported,
    );
    let suggested_mode = suggest_mode(&detected_input_kind);

    let root = site_root(&final_url)?;
    let llms = fetch_llms(&client, &root, &headers)?;
    let robots = fetch_robots(&client, &root)?;
    let recommendations =
        build_recommendations(&detected_input_kind, markdown_supported, &llms, &robots);

    Ok(ProbeReport {
        requested_url: requested.to_string(),
        final_url: final_url.to_string(),
        detected_input_kind,
        suggested_mode,
        markdown_supported,
        markdown: MarkdownReport {
            content_type,
            x_markdown_tokens: parse_u32_header(&headers, "x-markdown-tokens"),
            x_original_tokens: parse_u32_header(&headers, "x-original-tokens"),
            content_signal: header_value_str(&headers, "content-signal"),
            link_header: header_value(&headers, LINK),
        },
        llms,
        robots,
        recommendations,
    })
}

fn fetch_llms(
    client: &reqwest::blocking::Client,
    root: &Url,
    headers: &HeaderMap,
) -> Result<LlmsReport> {
    let link_header = header_value(headers, LINK);
    let header_llms = header_value_str(headers, "x-llms-txt");
    let discovered_from_header = header_llms.is_some();
    let discovered_from_link = parse_link_rel(root, link_header.as_deref(), "llms-txt").is_some();
    let index_url = header_llms
        .as_deref()
        .and_then(|path| root.join(path).ok())
        .or_else(|| parse_link_rel(root, link_header.as_deref(), "llms-txt"))
        .or_else(|| root.join("llms.txt").ok());
    let full_index_url = parse_link_rel(root, link_header.as_deref(), "llms-full-txt")
        .or_else(|| root.join("llms-full.txt").ok());

    if let Some(url) = &index_url {
        let response = client
            .get(url.clone())
            .header(
                ACCEPT,
                HeaderValue::from_static("text/plain, text/markdown;q=0.9"),
            )
            .send();

        if let Ok(response) = response {
            let status = response.status();
            if status.is_success() {
                let body = response.text().unwrap_or_default();
                return Ok(LlmsReport {
                    discovered_from: if discovered_from_header {
                        Some("x-llms-txt".to_string())
                    } else if discovered_from_link {
                        Some("link rel=llms-txt".to_string())
                    } else {
                        Some("guessed /llms.txt".to_string())
                    },
                    index_url: Some(url.to_string()),
                    full_index_url: full_index_url.map(|value| value.to_string()),
                    available: true,
                    page_links: count_llms_links(&body),
                    preview: body
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .take(8)
                        .map(ToOwned::to_owned)
                        .collect(),
                });
            }
        }
    }

    Ok(LlmsReport {
        discovered_from: None,
        index_url: index_url.map(|value| value.to_string()),
        full_index_url: full_index_url.map(|value| value.to_string()),
        available: false,
        page_links: 0,
        preview: Vec::new(),
    })
}

fn fetch_robots(client: &reqwest::blocking::Client, root: &Url) -> Result<RobotsReport> {
    let robots_url = root.join("robots.txt")?;
    let response = client.get(robots_url.clone()).send().ok();
    let (status, body) = if let Some(response) = response {
        let status = response.status().as_u16();
        let body = response.text().unwrap_or_default();
        (Some(status), body)
    } else {
        (None, String::new())
    };

    let sitemaps = body
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("sitemap:") {
                Some(trimmed[8..].trim().to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let first_sitemap_url_count = sitemaps
        .first()
        .and_then(|sitemap| client.get(sitemap).send().ok())
        .filter(|response| response.status().is_success())
        .and_then(|response| response.text().ok())
        .map(|body| body.matches("<loc>").count());

    Ok(RobotsReport {
        robots_url: robots_url.to_string(),
        robots_status: status,
        sitemaps,
        first_sitemap_url_count,
    })
}

fn build_recommendations(
    detected_input_kind: &DetectedInputKind,
    markdown_supported: bool,
    llms: &LlmsReport,
    robots: &RobotsReport,
) -> Vec<String> {
    let mut recommendations = Vec::new();

    match detected_input_kind {
        DetectedInputKind::LlmsTxt | DetectedInputKind::LlmsFullTxt => recommendations.push(
            "Treat this URL as a discovery index and expand it into a page frontier before syncing content."
                .to_string(),
        ),
        DetectedInputKind::Sitemap | DetectedInputKind::RobotsTxt => recommendations.push(
            "Treat this URL as a discovery seed rather than a content page.".to_string(),
        ),
        DetectedInputKind::PdfFile
        | DetectedInputKind::OfficeDocument
        | DetectedInputKind::MarkdownFile
        | DetectedInputKind::OpenApiSpec
        | DetectedInputKind::JsonFile => recommendations.push(
            "Treat this URL as a single importable asset, not a site crawl root.".to_string(),
        ),
        DetectedInputKind::ContentPage | DetectedInputKind::MarkdownEndpoint => {
            recommendations.push(
                "Treat this URL as a page seed: import it directly and optionally expand nearby navigation links."
                    .to_string(),
            )
        }
        DetectedInputKind::SiteRoot | DetectedInputKind::HtmlFile | DetectedInputKind::Unknown => {}
    }

    if markdown_supported {
        recommendations.push(
            "Use Accept: text/markdown as the primary content fetch adapter for this site."
                .to_string(),
        );
    } else {
        recommendations.push(
            "Fallback to HTML extraction or headless rendering because direct markdown negotiation is unavailable."
                .to_string(),
        );
    }

    if llms.available {
        recommendations.push(
            "Use llms.txt as a discovery index before sitemap or internal link crawling."
                .to_string(),
        );
    }

    if !robots.sitemaps.is_empty() {
        recommendations
            .push("Seed crawl jobs from sitemap URLs instead of blind root-page BFS.".to_string());
    }

    if !llms.available && robots.sitemaps.is_empty() {
        recommendations.push(
            "Expect partial coverage from root-link crawling alone; keep a fallback internal link frontier."
                .to_string(),
        );
    }

    recommendations
}

fn parse_link_rel(root: &Url, header: Option<&str>, target_rel: &str) -> Option<Url> {
    header.and_then(|value| {
        value.split(',').find_map(|segment| {
            let rel_matches = segment.contains(&format!("rel=\"{target_rel}\""))
                || segment.contains(&format!("rel={target_rel}"));
            if !rel_matches {
                return None;
            }

            let start = segment.find('<')?;
            let end = segment[start + 1..].find('>')?;
            root.join(segment[start + 1..start + 1 + end].trim()).ok()
        })
    })
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

pub(crate) fn site_root(url: &Url) -> Result<Url> {
    let mut root = url.clone();
    root.set_path("/");
    root.set_query(None);
    root.set_fragment(None);
    Ok(root)
}

pub(crate) fn detect_input_kind(
    final_url: &Url,
    requested_was_root: bool,
    content_type: Option<&str>,
    markdown_supported: bool,
) -> DetectedInputKind {
    let path = final_url.path().to_ascii_lowercase();
    let content_type = content_type.unwrap_or_default().to_ascii_lowercase();

    if path.ends_with("/llms.txt") || path == "/llms.txt" {
        return DetectedInputKind::LlmsTxt;
    }
    if path.ends_with("/llms-full.txt") || path == "/llms-full.txt" {
        return DetectedInputKind::LlmsFullTxt;
    }
    if path.ends_with("/robots.txt") || path == "/robots.txt" {
        return DetectedInputKind::RobotsTxt;
    }
    if path.ends_with("sitemap.xml") || (path.contains("sitemap") && path.ends_with(".xml")) {
        return DetectedInputKind::Sitemap;
    }
    if requested_was_root {
        return DetectedInputKind::SiteRoot;
    }
    if path.ends_with(".pdf") || content_type.starts_with("application/pdf") {
        return DetectedInputKind::PdfFile;
    }
    if path.ends_with(".doc")
        || path.ends_with(".docx")
        || path.ends_with(".ppt")
        || path.ends_with(".pptx")
        || path.ends_with(".xls")
        || path.ends_with(".xlsx")
    {
        return DetectedInputKind::OfficeDocument;
    }
    if path.ends_with(".md") || path.ends_with(".mdx") {
        return DetectedInputKind::MarkdownFile;
    }
    if path.ends_with("openapi.json")
        || path.ends_with("swagger.json")
        || path.ends_with("openapi.yaml")
        || path.ends_with("openapi.yml")
    {
        return DetectedInputKind::OpenApiSpec;
    }
    if path.ends_with(".json") || content_type.starts_with("application/json") {
        return DetectedInputKind::JsonFile;
    }
    if path.ends_with(".html") || path.ends_with(".htm") {
        return DetectedInputKind::HtmlFile;
    }
    if path == "/" || path.is_empty() {
        return DetectedInputKind::SiteRoot;
    }
    if markdown_supported {
        return DetectedInputKind::MarkdownEndpoint;
    }
    if content_type.starts_with("text/html") || content_type.is_empty() {
        return DetectedInputKind::ContentPage;
    }

    DetectedInputKind::Unknown
}

pub(crate) fn suggest_mode(kind: &DetectedInputKind) -> SuggestedMode {
    match kind {
        DetectedInputKind::LlmsTxt
        | DetectedInputKind::LlmsFullTxt
        | DetectedInputKind::Sitemap
        | DetectedInputKind::RobotsTxt
        | DetectedInputKind::SiteRoot => SuggestedMode::DiscoveryRoot,
        DetectedInputKind::ContentPage | DetectedInputKind::MarkdownEndpoint => {
            SuggestedMode::HybridSeed
        }
        DetectedInputKind::MarkdownFile
        | DetectedInputKind::PdfFile
        | DetectedInputKind::OfficeDocument
        | DetectedInputKind::OpenApiSpec
        | DetectedInputKind::JsonFile
        | DetectedInputKind::HtmlFile => SuggestedMode::SingleFile,
        DetectedInputKind::Unknown => SuggestedMode::SingleDocument,
    }
}

fn count_llms_links(body: &str) -> usize {
    body.lines()
        .filter(|line| line.contains("](") || line.contains("https://"))
        .count()
}

#[cfg(test)]
mod tests {
    use super::{DetectedInputKind, count_llms_links, detect_input_kind, parse_link_rel};
    use url::Url;

    #[test]
    fn parses_link_rel_headers() {
        let root = Url::parse("https://example.com/").expect("root URL");
        let link = r#"</llms.txt>; rel="llms-txt", </llms-full.txt>; rel="llms-full-txt""#;
        let llms = parse_link_rel(&root, Some(link), "llms-txt").expect("llms.txt link");
        let full = parse_link_rel(&root, Some(link), "llms-full-txt").expect("llms-full link");

        assert_eq!(llms.as_str(), "https://example.com/llms.txt");
        assert_eq!(full.as_str(), "https://example.com/llms-full.txt");
    }

    #[test]
    fn counts_links_in_llms_body() {
        let body = "\
# Example\n\
\n\
- [Intro](https://example.com/intro.md)\n\
- [API](https://example.com/api.md)\n";
        assert_eq!(count_llms_links(body), 2);
    }

    #[test]
    fn detects_llms_index_kind() {
        let url = Url::parse("https://example.com/llms.txt").expect("URL");
        let kind = detect_input_kind(&url, false, Some("text/plain"), false);
        assert!(matches!(kind, DetectedInputKind::LlmsTxt));
    }

    #[test]
    fn detects_markdown_endpoint_kind() {
        let url = Url::parse("https://docs.example.com/guide/install").expect("URL");
        let kind = detect_input_kind(&url, false, Some("text/markdown; charset=utf-8"), true);
        assert!(matches!(kind, DetectedInputKind::MarkdownEndpoint));
    }

    #[test]
    fn detects_site_root_before_markdown_negotiation() {
        let url = Url::parse("https://docs.example.com/").expect("URL");
        let kind = detect_input_kind(&url, true, Some("text/markdown; charset=utf-8"), true);
        assert!(matches!(kind, DetectedInputKind::SiteRoot));
    }

    #[test]
    fn keeps_requested_root_as_site_root_after_redirect() {
        let url = Url::parse("https://docs.example.com/introduction").expect("URL");
        let kind = detect_input_kind(&url, true, Some("text/markdown; charset=utf-8"), true);
        assert!(matches!(kind, DetectedInputKind::SiteRoot));
    }
}
