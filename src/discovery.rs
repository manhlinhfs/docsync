use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use quick_xml::Reader;
use quick_xml::events::Event;
use reqwest::header::{ACCEPT, HeaderValue};
use url::Url;

use crate::models::{DiscoveredPage, DiscoveryManifest, DiscoveryOrigin};
use crate::network::build_http_client;
use crate::probe::{self, DetectedInputKind, SuggestedMode};
use crate::util::{normalize_url, now_utc_rfc3339};

const MAX_SITEMAP_VISITS: usize = 32;

pub fn discover_source(
    entry_url: &str,
    source_name: &str,
    source_ref: &str,
    proxy_url: Option<&str>,
) -> Result<DiscoveryManifest> {
    let requested = normalize_url(entry_url)?;
    let client = build_client(proxy_url)?;
    let lower_path = requested.path().to_ascii_lowercase();

    let mut final_url = requested.clone();
    let mut llms_index_url: Option<String> = None;
    let mut llms_full_index_url: Option<String> = None;
    let mut sitemap_urls: Vec<String> = Vec::new();
    let mut notes = Vec::new();
    let mut frontier = Vec::new();
    let (detected_input_kind, suggested_mode);

    if is_llms_path(&lower_path) {
        detected_input_kind =
            if lower_path.ends_with("/llms-full.txt") || lower_path == "/llms-full.txt" {
                DetectedInputKind::LlmsFullTxt
            } else {
                DetectedInputKind::LlmsTxt
            };
        suggested_mode = SuggestedMode::DiscoveryRoot;
        if matches!(detected_input_kind, DetectedInputKind::LlmsFullTxt) {
            llms_full_index_url = Some(requested.to_string());
            frontier.extend(fetch_llms_frontier(
                &client,
                &requested,
                DiscoveryOrigin::LlmsFullTxt,
            )?);
        } else {
            llms_index_url = Some(requested.to_string());
            frontier.extend(fetch_llms_frontier(
                &client,
                &requested,
                DiscoveryOrigin::LlmsTxt,
            )?);
        }
    } else if is_sitemap_path(&lower_path) {
        detected_input_kind = DetectedInputKind::Sitemap;
        suggested_mode = SuggestedMode::DiscoveryRoot;
        sitemap_urls.push(requested.to_string());
        frontier.extend(fetch_sitemap_frontier(&client, vec![requested.clone()])?);
    } else if lower_path.ends_with("/robots.txt") || lower_path == "/robots.txt" {
        detected_input_kind = DetectedInputKind::RobotsTxt;
        suggested_mode = SuggestedMode::DiscoveryRoot;
        sitemap_urls = fetch_robots_sitemaps(&client, &requested)?;
        if sitemap_urls.is_empty() {
            notes.push("robots.txt did not advertise any sitemap URLs.".to_string());
        } else {
            frontier.extend(fetch_sitemap_frontier(
                &client,
                parse_urls(&sitemap_urls)
                    .context("failed to parse sitemap URLs from robots.txt")?,
            )?);
        }
    } else {
        let report = probe::probe_url_with_proxy(requested.as_str(), proxy_url)?;
        detected_input_kind = report.detected_input_kind;
        suggested_mode = report.suggested_mode;
        final_url = Url::parse(&report.final_url)
            .with_context(|| format!("failed to parse probed URL {}", report.final_url))?;
        llms_index_url = report.llms.index_url.clone();
        llms_full_index_url = report.llms.full_index_url.clone();
        sitemap_urls = report.robots.sitemaps.clone();

        if matches!(suggested_mode, SuggestedMode::DiscoveryRoot) {
            if report.llms.available {
                if let Some(url) = llms_index_url.as_deref() {
                    frontier.extend(fetch_llms_frontier(
                        &client,
                        &normalize_url(url)?,
                        DiscoveryOrigin::LlmsTxt,
                    )?);
                }
            }

            if let Some(url) = llms_full_index_url.as_deref() {
                match fetch_llms_frontier(
                    &client,
                    &normalize_url(url)?,
                    DiscoveryOrigin::LlmsFullTxt,
                ) {
                    Ok(pages) => frontier.extend(pages),
                    Err(error) => notes.push(format!(
                        "Skipped optional llms-full.txt discovery: {error:#}"
                    )),
                }
            }

            if !sitemap_urls.is_empty() {
                frontier.extend(fetch_sitemap_frontier(
                    &client,
                    parse_urls(&sitemap_urls)
                        .context("failed to parse sitemap URLs from probe results")?,
                )?);
            }
        } else {
            frontier.push(DiscoveredPage {
                url: canonicalize_url(&final_url),
                discovered_from: DiscoveryOrigin::SeedPage,
            });
            if report.llms.available || !sitemap_urls.is_empty() {
                notes.push(
                    "Discovery kept this entry as a direct page seed instead of expanding root indexes."
                        .to_string(),
                );
            } else {
                notes.push(
                    "No llms.txt or sitemap frontier was available, so discovery kept the probed page as a seed."
                        .to_string(),
                );
            }
        }
    }

    let frontier = dedupe_pages(frontier);
    let adapters = detect_adapters(
        &frontier,
        &llms_index_url,
        &llms_full_index_url,
        &sitemap_urls,
    );

    if frontier.is_empty() {
        notes.push("Discovery completed without any importable page URLs.".to_string());
    }

    Ok(DiscoveryManifest {
        schema_version: 1,
        created_at: now_utc_rfc3339(),
        source_name: source_name.to_string(),
        entry_url: requested.to_string(),
        final_url: final_url.to_string(),
        source_ref: source_ref.to_string(),
        detected_input_kind,
        suggested_mode,
        adapters,
        llms_index_url,
        llms_full_index_url,
        sitemap_urls: dedupe_strings(sitemap_urls),
        frontier,
        notes,
    })
}

fn build_client(proxy_url: Option<&str>) -> Result<reqwest::blocking::Client> {
    build_http_client(20, proxy_url)
}

fn fetch_llms_frontier(
    client: &reqwest::blocking::Client,
    url: &Url,
    origin: DiscoveryOrigin,
) -> Result<Vec<DiscoveredPage>> {
    let response = client
        .get(url.clone())
        .header(
            ACCEPT,
            HeaderValue::from_static("text/plain, text/markdown;q=0.9"),
        )
        .send()
        .with_context(|| format!("failed to fetch llms index {}", url))?;

    if !response.status().is_success() {
        bail!("llms index {} returned HTTP {}", url, response.status());
    }

    let body = response
        .text()
        .with_context(|| format!("failed to read llms index {}", url))?;

    Ok(parse_llms_urls(url, &body)
        .into_iter()
        .map(|value| DiscoveredPage {
            url: value,
            discovered_from: origin,
        })
        .collect())
}

fn fetch_sitemap_frontier(
    client: &reqwest::blocking::Client,
    roots: Vec<Url>,
) -> Result<Vec<DiscoveredPage>> {
    let mut visited = BTreeSet::new();
    let mut queue = roots;
    let mut pages = Vec::new();

    while let Some(sitemap_url) = queue.pop() {
        let canonical = canonicalize_url(&sitemap_url);
        if !visited.insert(canonical.clone()) {
            continue;
        }
        if visited.len() > MAX_SITEMAP_VISITS {
            bail!(
                "aborted sitemap discovery after visiting more than {MAX_SITEMAP_VISITS} sitemap documents"
            );
        }

        let response = client
            .get(sitemap_url.clone())
            .send()
            .with_context(|| format!("failed to fetch sitemap {}", sitemap_url))?;

        if !response.status().is_success() {
            bail!(
                "sitemap {} returned HTTP {}",
                sitemap_url,
                response.status()
            );
        }

        let body = response
            .text()
            .with_context(|| format!("failed to read sitemap {}", sitemap_url))?;
        let parsed = parse_sitemap_document(&body)
            .with_context(|| format!("failed to parse sitemap {}", sitemap_url))?;

        pages.extend(parsed.page_urls.into_iter().map(|value| DiscoveredPage {
            url: value,
            discovered_from: DiscoveryOrigin::Sitemap,
        }));

        for nested in parsed.nested_sitemaps {
            let parsed_url = normalize_url(&nested)
                .with_context(|| format!("invalid nested sitemap URL `{nested}`"))?;
            let nested_canonical = canonicalize_url(&parsed_url);
            if !visited.contains(&nested_canonical) {
                queue.push(parsed_url);
            }
        }
    }

    Ok(pages)
}

fn fetch_robots_sitemaps(
    client: &reqwest::blocking::Client,
    robots_url: &Url,
) -> Result<Vec<String>> {
    let response = client
        .get(robots_url.clone())
        .send()
        .with_context(|| format!("failed to fetch robots.txt {}", robots_url))?;

    if !response.status().is_success() {
        bail!(
            "robots.txt {} returned HTTP {}",
            robots_url,
            response.status()
        );
    }

    let body = response
        .text()
        .with_context(|| format!("failed to read robots.txt {}", robots_url))?;

    Ok(body
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.len() >= 8 && trimmed[..8].eq_ignore_ascii_case("sitemap:") {
                Some(trimmed[8..].trim().to_string())
            } else {
                None
            }
        })
        .collect())
}

fn parse_llms_urls(base_url: &Url, body: &str) -> Vec<String> {
    let mut urls = BTreeSet::new();

    for line in body.lines() {
        for value in extract_markdown_link_targets(line) {
            if let Some(url) = resolve_llms_link(base_url, &value) {
                urls.insert(canonicalize_url(&url));
            }
        }

        for token in line.split_whitespace() {
            let trimmed = token.trim_matches(|ch: char| "\"'()[]<>{},".contains(ch));
            if let Some(url) = parse_absolute_http_url(trimmed) {
                urls.insert(canonicalize_url(&url));
            }
        }
    }

    urls.into_iter().collect()
}

struct ParsedSitemap {
    page_urls: Vec<String>,
    nested_sitemaps: Vec<String>,
}

fn parse_sitemap_document(body: &str) -> Result<ParsedSitemap> {
    let mut reader = Reader::from_str(body);
    reader.config_mut().trim_text(true);

    let mut buffer = Vec::new();
    let mut section: Option<&'static [u8]> = None;
    let mut in_loc = false;
    let mut page_urls = Vec::new();
    let mut nested_sitemaps = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) => match event.name().as_ref() {
                b"url" => section = Some(b"url"),
                b"sitemap" => section = Some(b"sitemap"),
                b"loc" => in_loc = true,
                _ => {}
            },
            Event::End(event) => match event.name().as_ref() {
                b"url" | b"sitemap" => section = None,
                b"loc" => in_loc = false,
                _ => {}
            },
            Event::Text(event) if in_loc => {
                let value = event.xml_content()?.into_owned();
                if let Ok(url) = normalize_url(&value) {
                    match section {
                        Some(b"url") => page_urls.push(canonicalize_url(&url)),
                        Some(b"sitemap") => nested_sitemaps.push(canonicalize_url(&url)),
                        _ => {}
                    }
                }
            }
            Event::CData(event) if in_loc => {
                let value = event.xml_content()?.into_owned();
                if let Ok(url) = normalize_url(&value) {
                    match section {
                        Some(b"url") => page_urls.push(canonicalize_url(&url)),
                        Some(b"sitemap") => nested_sitemaps.push(canonicalize_url(&url)),
                        _ => {}
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }

        buffer.clear();
    }

    Ok(ParsedSitemap {
        page_urls: dedupe_strings(page_urls),
        nested_sitemaps: dedupe_strings(nested_sitemaps),
    })
}

fn extract_markdown_link_targets(line: &str) -> Vec<String> {
    let mut rest = line;
    let mut links = Vec::new();

    while let Some(start) = rest.find("](") {
        let after = &rest[start + 2..];
        let Some(end) = after.find(')') else {
            break;
        };
        let target = after[..end].trim();
        if !target.is_empty() {
            links.push(target.to_string());
        }
        rest = &after[end + 1..];
    }

    links
}

fn resolve_llms_link(base_url: &Url, value: &str) -> Option<Url> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    if let Some(url) = parse_absolute_http_url(trimmed) {
        return Some(url);
    }

    if trimmed.starts_with('/') || trimmed.starts_with("./") || trimmed.starts_with("../") {
        return base_url.join(trimmed).ok();
    }

    None
}

fn parse_absolute_http_url(value: &str) -> Option<Url> {
    let url = Url::parse(value).ok()?;
    match url.scheme() {
        "http" | "https" => Some(url),
        _ => None,
    }
}

fn dedupe_pages(pages: Vec<DiscoveredPage>) -> Vec<DiscoveredPage> {
    let mut unique = BTreeMap::new();
    for page in pages {
        unique.entry(page.url.clone()).or_insert(page);
    }
    unique.into_values().collect()
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn parse_urls(values: &[String]) -> Result<Vec<Url>> {
    values
        .iter()
        .map(|value| normalize_url(value))
        .collect::<Result<Vec<_>>>()
}

fn detect_adapters(
    frontier: &[DiscoveredPage],
    _llms_index_url: &Option<String>,
    _llms_full_index_url: &Option<String>,
    _sitemap_urls: &[String],
) -> Vec<String> {
    let mut adapters = BTreeSet::new();

    if frontier
        .iter()
        .any(|page| page.discovered_from == DiscoveryOrigin::LlmsTxt)
    {
        adapters.insert("llms_txt".to_string());
    }
    if frontier
        .iter()
        .any(|page| page.discovered_from == DiscoveryOrigin::LlmsFullTxt)
    {
        adapters.insert("llms_full_txt".to_string());
    }
    if frontier
        .iter()
        .any(|page| page.discovered_from == DiscoveryOrigin::Sitemap)
    {
        adapters.insert("sitemap".to_string());
    }
    if frontier
        .iter()
        .any(|page| page.discovered_from == DiscoveryOrigin::SeedPage)
    {
        adapters.insert("seed_page".to_string());
    }

    adapters.into_iter().collect()
}

fn is_llms_path(path: &str) -> bool {
    path.ends_with("/llms.txt")
        || path == "/llms.txt"
        || path.ends_with("/llms-full.txt")
        || path == "/llms-full.txt"
}

fn is_sitemap_path(path: &str) -> bool {
    path.ends_with("sitemap.xml") || (path.contains("sitemap") && path.ends_with(".xml"))
}

fn canonicalize_url(url: &Url) -> String {
    let mut canonical = url.clone();
    canonical.set_fragment(None);
    if canonical.path().is_empty() {
        canonical.set_path("/");
    }
    canonical.to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        DiscoveryOrigin, extract_markdown_link_targets, parse_llms_urls, parse_sitemap_document,
        resolve_llms_link,
    };
    use url::Url;

    #[test]
    fn extracts_markdown_targets() {
        let links = extract_markdown_link_targets(
            "- [Intro](https://example.com/intro) [API](https://example.com/api)",
        );
        assert_eq!(
            links,
            vec!["https://example.com/intro", "https://example.com/api"]
        );
    }

    #[test]
    fn parses_llms_urls_from_markdown_and_plain_urls() {
        let base = Url::parse("https://example.com/llms.txt").expect("base URL");
        let body = "\
- [Intro](https://example.com/intro)\n\
- https://example.com/api#section\n\
- [Intro](https://example.com/intro)\n";

        let urls = parse_llms_urls(&base, body);
        assert_eq!(
            urls,
            vec![
                "https://example.com/api".to_string(),
                "https://example.com/intro".to_string()
            ]
        );
    }

    #[test]
    fn resolves_relative_links_in_llms_indexes() {
        let base = Url::parse("https://docs.example.com/llms.txt").expect("base URL");
        let body = "- [Guide](/guide/install)\n";
        let urls = parse_llms_urls(&base, body);
        assert_eq!(
            urls,
            vec!["https://docs.example.com/guide/install".to_string()]
        );
    }

    #[test]
    fn ignores_non_http_llms_targets() {
        let base = Url::parse("https://docs.example.com/llms.txt").expect("base URL");
        assert!(resolve_llms_link(&base, "mailto:test@example.com").is_none());
        assert!(resolve_llms_link(&base, "accesstoken:/").is_none());
    }

    #[test]
    fn parses_urlset_and_nested_sitemap_entries() {
        let xml = r#"
            <sitemapindex>
              <sitemap><loc>https://example.com/sitemap-guides.xml</loc></sitemap>
              <url><loc>https://example.com/intro</loc></url>
              <url><loc>https://example.com/api#details</loc></url>
            </sitemapindex>
        "#;

        let parsed = parse_sitemap_document(xml).expect("parsed sitemap");
        assert_eq!(
            parsed.page_urls,
            vec![
                "https://example.com/api".to_string(),
                "https://example.com/intro".to_string()
            ]
        );
        assert_eq!(
            parsed.nested_sitemaps,
            vec!["https://example.com/sitemap-guides.xml".to_string()]
        );
    }

    #[test]
    fn discovery_origin_serialization_order_is_stable() {
        assert!(DiscoveryOrigin::LlmsTxt < DiscoveryOrigin::Sitemap);
    }
}
