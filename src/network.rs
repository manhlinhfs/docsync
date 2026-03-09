use std::env;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Proxy;
use reqwest::blocking::Client;
use reqwest::redirect::Policy;

use crate::models::{AppConfig, SourceDefinition};

pub fn resolve_proxy_url(
    cli_override: Option<&str>,
    config: Option<&AppConfig>,
    source: Option<&SourceDefinition>,
) -> Option<String> {
    cli_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| source.and_then(|value| value.proxy_url.clone()))
        .or_else(|| config.and_then(|value| value.default_proxy_url.clone()))
        .or_else(proxy_from_env)
}

pub fn build_http_client(timeout_secs: u64, proxy_url: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(Policy::limited(10));

    if let Some(proxy_url) = proxy_url {
        builder = builder.proxy(
            Proxy::all(proxy_url).with_context(|| format!("invalid proxy URL `{proxy_url}`"))?,
        );
    }

    builder.build().context("failed to build HTTP client")
}

pub fn apply_proxy_to_git_command(command: &mut Command, proxy_url: Option<&str>) {
    if let Some(proxy_url) = proxy_url {
        command.env("ALL_PROXY", proxy_url);
        command.env("HTTPS_PROXY", proxy_url);
        command.env("HTTP_PROXY", proxy_url);
    }
}

fn proxy_from_env() -> Option<String> {
    for key in ["DOCSYNC_PROXY", "HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"] {
        if let Some(value) = env::var_os(key) {
            let value = value.to_string_lossy().trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{build_http_client, resolve_proxy_url};
    use crate::models::{AppConfig, SourceDefinition, SourceKind, VersionStrategy};

    #[test]
    fn prefers_cli_proxy_over_source_and_config() {
        let config = AppConfig {
            schema_version: 1,
            created_at: String::new(),
            updated_at: String::new(),
            default_proxy_url: Some("http://config-proxy:8080".to_string()),
            sources: BTreeMap::new(),
        };
        let source = source_with_proxy(Some("http://source-proxy:8080"));
        let resolved =
            resolve_proxy_url(Some("http://cli-proxy:8080"), Some(&config), Some(&source));
        assert_eq!(resolved.as_deref(), Some("http://cli-proxy:8080"));
    }

    #[test]
    fn falls_back_to_source_then_config() {
        let config = AppConfig {
            schema_version: 1,
            created_at: String::new(),
            updated_at: String::new(),
            default_proxy_url: Some("http://config-proxy:8080".to_string()),
            sources: BTreeMap::new(),
        };
        let source = source_with_proxy(Some("http://source-proxy:8080"));
        assert_eq!(
            resolve_proxy_url(None, Some(&config), Some(&source)).as_deref(),
            Some("http://source-proxy:8080")
        );
        assert_eq!(
            resolve_proxy_url(None, Some(&config), Some(&source_with_proxy(None))).as_deref(),
            Some("http://config-proxy:8080")
        );
    }

    #[test]
    fn accepts_http_and_socks_proxy_urls() {
        build_http_client(5, Some("http://user:pass@127.0.0.1:8080")).expect("http proxy");
        build_http_client(5, Some("socks5://127.0.0.1:1080")).expect("socks5 proxy");
        build_http_client(5, Some("socks5h://user:pass@127.0.0.1:1080"))
            .expect("socks5h proxy");
    }

    fn source_with_proxy(proxy_url: Option<&str>) -> SourceDefinition {
        SourceDefinition {
            name: "demo".to_string(),
            entry_url: "https://example.com".to_string(),
            proxy_url: proxy_url.map(ToOwned::to_owned),
            source_kind: SourceKind::Website,
            repo_url: None,
            docs_path: None,
            default_ref: None,
            version_strategy: VersionStrategy::DateSnapshot,
            tags: Vec::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}
