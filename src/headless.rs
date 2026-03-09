use std::env;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::models::{AppConfig, SourceDefinition};

const BROWSER_CANDIDATES: &[&str] = &[
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
    "microsoft-edge",
];

pub fn resolve_browser_cmd(
    cli_override: Option<&str>,
    config: Option<&AppConfig>,
    source: Option<&SourceDefinition>,
) -> Option<String> {
    cli_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| source.and_then(|value| value.browser_cmd.clone()))
        .or_else(|| config.and_then(|value| value.default_browser_cmd.clone()))
        .or_else(detect_browser_in_path)
}

pub fn should_try_headless(html: &str, markdown_preview: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    let script_count = lower.matches("<script").count();
    let dynamic_markers = [
        "id=\"__next\"",
        "id=\"__docusaurus\"",
        "data-reactroot",
        "window.__next_data__",
        "type=\"module\"",
        "vite",
    ];
    let has_dynamic_marker = dynamic_markers.iter().any(|marker| lower.contains(marker));
    let thin_markdown = markdown_preview.trim().len() < 400;

    (script_count >= 5 && thin_markdown) || (has_dynamic_marker && thin_markdown)
}

pub fn render_url(browser_cmd: &str, url: &str, proxy_url: Option<&str>) -> Result<String> {
    let mut command = Command::new(browser_cmd);
    command.arg("--headless");
    command.arg("--disable-gpu");
    command.arg("--dump-dom");
    command.arg("--hide-scrollbars");
    command.arg("--no-first-run");
    command.arg("--no-default-browser-check");
    command.arg("--virtual-time-budget=20000");
    if let Some(proxy_url) = proxy_url {
        command.arg(format!("--proxy-server={proxy_url}"));
    }
    command.arg(url);

    let output = command
        .output()
        .with_context(|| format!("failed to execute browser command `{browser_cmd}` for {url}"))?;
    if !output.status.success() {
        bail!(
            "browser command `{browser_cmd}` exited with status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn detect_browser_in_path() -> Option<String> {
    let path = env::var_os("PATH")?;
    for candidate in BROWSER_CANDIDATES {
        for dir in env::split_paths(&path) {
            let full = dir.join(candidate);
            if full.is_file() {
                return Some(full.to_string_lossy().to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Result;

    use super::{render_url, should_try_headless};

    #[test]
    fn detects_dynamic_shell_html() {
        let html = r#"
            <html>
              <body>
                <div id="__next"></div>
                <script type="module"></script>
                <script></script>
                <script></script>
                <script></script>
                <script></script>
                <script></script>
              </body>
            </html>
        "#;
        assert!(should_try_headless(html, "tiny"));
        assert!(!should_try_headless(
            "<html><body><article>full text</article></body></html>",
            "This page has enough extracted markdown to avoid browser rendering."
        ));
    }

    #[test]
    fn renders_dom_through_external_browser_command() -> Result<()> {
        let temp_dir = std::env::temp_dir().join(format!(
            "docsync-headless-test-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir_all(&temp_dir)?;
        let script_path = temp_dir.join("fake-browser.sh");
        fs::write(
            &script_path,
            "#!/usr/bin/env bash\nprintf '<html><body><main>Rendered</main></body></html>'\n",
        )?;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;

        let rendered = render_url(&script_path.to_string_lossy(), "https://example.com", None)?;
        assert!(rendered.contains("Rendered"));
        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }
}
