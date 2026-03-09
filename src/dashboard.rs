use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::AppPaths;
use crate::models::{PageManifestEntry, SnapshotManifest};
use crate::quality::PageQualityRating;
use crate::util::now_utc_rfc3339;

#[derive(Debug, Serialize)]
pub struct DashboardResult {
    pub source_name: String,
    pub snapshot_label: String,
    pub output_path: PathBuf,
    pub pages_shown: usize,
    pub high_quality_pages: usize,
    pub medium_quality_pages: usize,
    pub low_quality_pages: usize,
}

#[derive(Debug, Serialize)]
pub struct DashboardServeResult {
    pub source_name: String,
    pub snapshot_label: String,
    pub output_path: PathBuf,
    pub state_path: PathBuf,
    pub host: String,
    pub port: u16,
    pub url: String,
    pub pid: u32,
    pub already_running: bool,
}

#[derive(Debug, Serialize)]
pub struct DashboardServerStatus {
    pub source_name: String,
    pub snapshot_label: String,
    pub running: bool,
    pub host: String,
    pub port: u16,
    pub url: String,
    pub pid: Option<u32>,
    pub state_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DashboardServerState {
    source_name: String,
    snapshot_label: String,
    root_dir: PathBuf,
    output_path: PathBuf,
    host: String,
    port: u16,
    url: String,
    pid: u32,
    started_at: String,
}

pub fn build_dashboard(
    paths: &AppPaths,
    source_name: &str,
    reference: Option<String>,
    output: Option<PathBuf>,
) -> Result<DashboardResult> {
    let snapshot_dir = resolve_snapshot_dir(paths, source_name, reference.as_deref())?;
    let manifest = read_snapshot_manifest(&snapshot_dir)?;
    let output_path = output.unwrap_or_else(|| snapshot_dir.join("dashboard.html"));
    let html = render_dashboard_html(&manifest);
    fs::write(&output_path, html)
        .with_context(|| format!("failed to write {}", output_path.display()))?;

    let mut high_quality_pages = 0usize;
    let mut medium_quality_pages = 0usize;
    let mut low_quality_pages = 0usize;
    let mut pages_shown = 0usize;

    for page in &manifest.pages {
        if page.page_path.is_none() {
            continue;
        }
        pages_shown += 1;
        if let Some(quality) = page.quality.as_ref() {
            match quality.rating {
                PageQualityRating::High => high_quality_pages += 1,
                PageQualityRating::Medium => medium_quality_pages += 1,
                PageQualityRating::Low => low_quality_pages += 1,
            }
        }
    }

    Ok(DashboardResult {
        source_name: source_name.to_string(),
        snapshot_label: manifest.snapshot_label,
        output_path,
        pages_shown,
        high_quality_pages,
        medium_quality_pages,
        low_quality_pages,
    })
}

pub fn serve_dashboard(
    paths: &AppPaths,
    source_name: &str,
    reference: Option<String>,
    output: Option<PathBuf>,
    host: &str,
    port: u16,
) -> Result<DashboardServeResult> {
    let built = build_dashboard(paths, source_name, reference, output)?;
    let state_path = dashboard_state_path(paths, source_name, &built.snapshot_label);

    if let Ok(state) = read_dashboard_state(&state_path) {
        if state.port == port && state.host == host && is_dashboard_running(&state) {
            return Ok(DashboardServeResult {
                source_name: built.source_name,
                snapshot_label: built.snapshot_label,
                output_path: built.output_path,
                state_path,
                host: state.host,
                port: state.port,
                url: state.url,
                pid: state.pid,
                already_running: true,
            });
        }
        let _ = stop_process(state.pid);
        let _ = fs::remove_file(&state_path);
    }

    let exe = std::env::current_exe().context("failed to resolve current executable")?;
    let mut child = Command::new(exe);
    child
        .arg("--home")
        .arg(&paths.home)
        .arg("dashboard-host")
        .arg("--root")
        .arg(
            built
                .output_path
                .parent()
                .context("dashboard output path is missing a parent directory")?,
        )
        .arg("--host")
        .arg(host)
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = child.spawn().context("failed to start dashboard server")?;
    let pid = child.id();

    let state = DashboardServerState {
        source_name: built.source_name.clone(),
        snapshot_label: built.snapshot_label.clone(),
        root_dir: built
            .output_path
            .parent()
            .context("dashboard output path is missing a parent directory")?
            .to_path_buf(),
        output_path: built.output_path.clone(),
        host: host.to_string(),
        port,
        url: dashboard_url(host, port),
        pid,
        started_at: now_utc_rfc3339(),
    };
    write_dashboard_state(&state_path, &state)?;

    Ok(DashboardServeResult {
        source_name: built.source_name,
        snapshot_label: built.snapshot_label,
        output_path: built.output_path,
        state_path,
        host: host.to_string(),
        port,
        url: state.url,
        pid,
        already_running: false,
    })
}

pub fn dashboard_status(
    paths: &AppPaths,
    source_name: &str,
    reference: Option<String>,
) -> Result<DashboardServerStatus> {
    let snapshot_dir = resolve_snapshot_dir(paths, source_name, reference.as_deref())?;
    let manifest = read_snapshot_manifest(&snapshot_dir)?;
    let state_path = dashboard_state_path(paths, source_name, &manifest.snapshot_label);
    let state = read_dashboard_state(&state_path).ok();

    if let Some(state) = state {
        let running = is_dashboard_running(&state);
        return Ok(DashboardServerStatus {
            source_name: source_name.to_string(),
            snapshot_label: manifest.snapshot_label,
            running,
            host: state.host.clone(),
            port: state.port,
            url: state.url.clone(),
            pid: if running { Some(state.pid) } else { None },
            state_path,
        });
    }

    Ok(DashboardServerStatus {
        source_name: source_name.to_string(),
        snapshot_label: manifest.snapshot_label,
        running: false,
        host: "127.0.0.1".to_string(),
        port: 4317,
        url: dashboard_url("127.0.0.1", 4317),
        pid: None,
        state_path,
    })
}

pub fn stop_dashboard(
    paths: &AppPaths,
    source_name: &str,
    reference: Option<String>,
) -> Result<DashboardServerStatus> {
    let status = dashboard_status(paths, source_name, reference)?;
    if let Ok(state) = read_dashboard_state(&status.state_path) {
        let _ = stop_process(state.pid);
        let _ = fs::remove_file(&status.state_path);
    }
    Ok(DashboardServerStatus {
        running: false,
        pid: None,
        ..status
    })
}

pub fn run_dashboard_host(root: &Path, host: &str, port: u16) -> Result<()> {
    let listener = TcpListener::bind((host, port))
        .with_context(|| format!("failed to bind dashboard server on {host}:{port}"))?;

    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            continue;
        };
        let _ = handle_dashboard_request(&mut stream, root);
    }

    Ok(())
}

fn render_dashboard_html(manifest: &SnapshotManifest) -> String {
    let fetch = manifest.fetch.as_ref();
    let quality = fetch.and_then(|summary| summary.quality.as_ref());
    let rows = manifest
        .pages
        .iter()
        .filter(|page| page.page_path.is_some())
        .map(render_page_row)
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>docsync dashboard - {source_name}</title>
  <style>
    :root {{
      --bg: #0b1117;
      --panel: rgba(18, 28, 39, 0.92);
      --panel-2: rgba(13, 20, 28, 0.85);
      --text: #eef3f8;
      --muted: #97a8ba;
      --line: rgba(182, 201, 222, 0.14);
      --accent: #79d2b0;
      --warn: #f5c56c;
      --bad: #ff7b7b;
      --shadow: 0 18px 70px rgba(0, 0, 0, 0.28);
      --font-ui: "IBM Plex Sans", "Segoe UI", sans-serif;
      --font-mono: "IBM Plex Mono", "SFMono-Regular", monospace;
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      font-family: var(--font-ui);
      color: var(--text);
      background:
        radial-gradient(circle at top left, rgba(121, 210, 176, 0.14), transparent 30%),
        radial-gradient(circle at top right, rgba(76, 146, 255, 0.10), transparent 26%),
        linear-gradient(180deg, #0b1117, #0d1620 48%, #0b1117);
    }}
    .shell {{
      max-width: 1320px;
      margin: 0 auto;
      padding: 32px 20px 56px;
    }}
    .hero {{
      background: linear-gradient(180deg, rgba(17, 27, 37, 0.94), rgba(12, 19, 27, 0.88));
      border: 1px solid var(--line);
      border-radius: 26px;
      padding: 28px;
      box-shadow: var(--shadow);
    }}
    .eyebrow {{
      font-family: var(--font-mono);
      font-size: 12px;
      color: var(--accent);
      text-transform: uppercase;
      letter-spacing: 0.18em;
    }}
    h1 {{
      margin: 10px 0 8px;
      font-size: clamp(34px, 5vw, 62px);
      line-height: 0.95;
    }}
    .hero p {{
      margin: 0;
      max-width: 780px;
      color: var(--muted);
      font-size: 16px;
      line-height: 1.6;
    }}
    .stats {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
      gap: 14px;
      margin-top: 22px;
    }}
    .card {{
      background: var(--panel-2);
      border: 1px solid var(--line);
      border-radius: 18px;
      padding: 16px 18px;
    }}
    .label {{
      font-family: var(--font-mono);
      font-size: 11px;
      text-transform: uppercase;
      letter-spacing: 0.14em;
      color: var(--muted);
    }}
    .value {{
      margin-top: 8px;
      font-size: 30px;
      font-weight: 700;
    }}
    .sub {{
      margin-top: 6px;
      color: var(--muted);
      font-size: 13px;
    }}
    .toolbar {{
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      margin: 26px 0 16px;
      align-items: center;
    }}
    .search {{
      flex: 1 1 320px;
      min-width: 0;
      background: rgba(8, 13, 18, 0.6);
      color: var(--text);
      border: 1px solid var(--line);
      border-radius: 14px;
      padding: 14px 16px;
      font-size: 14px;
    }}
    .hint {{
      font-size: 13px;
      color: var(--muted);
    }}
    .table-wrap {{
      overflow: auto;
      border: 1px solid var(--line);
      border-radius: 20px;
      background: rgba(10, 16, 23, 0.84);
      box-shadow: var(--shadow);
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      min-width: 1040px;
    }}
    thead th {{
      position: sticky;
      top: 0;
      background: rgba(12, 19, 27, 0.98);
      text-align: left;
      padding: 14px 16px;
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.12em;
      color: var(--muted);
      border-bottom: 1px solid var(--line);
    }}
    tbody td {{
      padding: 14px 16px;
      border-bottom: 1px solid rgba(182, 201, 222, 0.08);
      vertical-align: top;
      font-size: 14px;
    }}
    tbody tr:hover {{
      background: rgba(121, 210, 176, 0.06);
    }}
    .mono {{
      font-family: var(--font-mono);
      font-size: 12px;
      color: var(--muted);
    }}
    .badge {{
      display: inline-flex;
      align-items: center;
      border-radius: 999px;
      padding: 4px 10px;
      font-size: 12px;
      font-weight: 700;
      border: 1px solid currentColor;
    }}
    .high {{ color: var(--accent); }}
    .medium {{ color: var(--warn); }}
    .low {{ color: var(--bad); }}
    a {{ color: #b9dbff; text-decoration: none; }}
    a:hover {{ text-decoration: underline; }}
    .small {{ font-size: 12px; color: var(--muted); }}
  </style>
</head>
<body>
  <div class="shell">
    <section class="hero">
      <div class="eyebrow">docsync dashboard</div>
      <h1>{source_name}</h1>
      <p>Snapshot <span class="mono">{snapshot_label}</span> with live quality scoring, incremental state, and page-level audit data. This report is generated locally from <span class="mono">manifest.json</span>.</p>
      <div class="stats">
        <div class="card"><div class="label">Stored Pages</div><div class="value">{stored_pages}</div><div class="sub">Snapshot pages written under <span class="mono">pages/</span></div></div>
        <div class="card"><div class="label">Changed Or New</div><div class="value">{changed_pages}</div><div class="sub">Diff candidates for import</div></div>
        <div class="card"><div class="label">High Quality</div><div class="value">{high_quality_pages}</div><div class="sub">Strong import-ready content</div></div>
        <div class="card"><div class="label">Medium Quality</div><div class="value">{medium_quality_pages}</div><div class="sub">Review if output matters</div></div>
        <div class="card"><div class="label">Low Quality</div><div class="value">{low_quality_pages}</div><div class="sub">Likely thin, noisy, or weakly structured</div></div>
        <div class="card"><div class="label">Residual Markup</div><div class="value">{residual_markup_pages}</div><div class="sub">Pages still containing HTML or MDX leftovers</div></div>
      </div>
    </section>
    <div class="toolbar">
      <input id="search" class="search" placeholder="Filter by URL, status, quality, or path">
      <div class="hint">Data-dense local audit view for imported docs snapshots.</div>
    </div>
    <div class="table-wrap">
      <table id="pages">
        <thead>
          <tr>
            <th>Page</th>
            <th>Status</th>
            <th>Quality</th>
            <th>Words</th>
            <th>Method</th>
            <th>Change</th>
            <th>Profile</th>
            <th>Path</th>
          </tr>
        </thead>
        <tbody>
          {rows}
        </tbody>
      </table>
    </div>
  </div>
  <script>
    const input = document.getElementById('search');
    const rows = Array.from(document.querySelectorAll('#pages tbody tr'));
    input.addEventListener('input', () => {{
      const query = input.value.trim().toLowerCase();
      for (const row of rows) {{
        row.style.display = row.textContent.toLowerCase().includes(query) ? '' : 'none';
      }}
    }});
  </script>
</body>
</html>"#,
        source_name = escape_html(&manifest.source_name),
        snapshot_label = escape_html(&manifest.snapshot_label),
        stored_pages = fetch.map(|summary| summary.stored_pages).unwrap_or(0),
        changed_pages = manifest
            .diff
            .as_ref()
            .map(|diff| diff.new_pages + diff.changed_pages)
            .unwrap_or(0),
        high_quality_pages = quality.map(|value| value.high_quality_pages).unwrap_or(0),
        medium_quality_pages = quality.map(|value| value.medium_quality_pages).unwrap_or(0),
        low_quality_pages = quality.map(|value| value.low_quality_pages).unwrap_or(0),
        residual_markup_pages = quality
            .map(|value| value.residual_markup_pages)
            .unwrap_or(0),
        rows = rows,
    )
}

fn render_page_row(page: &PageManifestEntry) -> String {
    let url = if page.final_url.is_empty() {
        page.url.as_str()
    } else {
        page.final_url.as_str()
    };
    let page_path = page
        .page_path
        .as_deref()
        .map(|value| value.display().to_string())
        .unwrap_or_default();
    let quality = page.quality.as_ref();
    let (rating_label, rating_class, score, words) = if let Some(quality) = quality {
        (
            match quality.rating {
                PageQualityRating::High => "high",
                PageQualityRating::Medium => "medium",
                PageQualityRating::Low => "low",
            },
            match quality.rating {
                PageQualityRating::High => "high",
                PageQualityRating::Medium => "medium",
                PageQualityRating::Low => "low",
            },
            quality.score.to_string(),
            quality.word_count.to_string(),
        )
    } else {
        ("n/a", "medium", "-".to_string(), "-".to_string())
    };
    let profile = page
        .normalization
        .as_ref()
        .and_then(|value| value.profiles_applied.first())
        .cloned()
        .unwrap_or_else(|| "generic".to_string());

    format!(
        r#"<tr>
  <td><a href="{url}" target="_blank" rel="noreferrer">{url}</a><div class="small">{status}</div></td>
  <td><span class="badge {rating_class}">{status}</span></td>
  <td><span class="badge {rating_class}">{rating_label}</span><div class="small">score {score}</div></td>
  <td>{words}</td>
  <td><span class="mono">{method}</span></td>
  <td><span class="mono">{change}</span></td>
  <td><span class="mono">{profile}</span></td>
  <td><span class="mono">{page_path}</span></td>
</tr>"#,
        url = escape_html(url),
        status = escape_html(&page.status),
        rating_class = rating_class,
        rating_label = escape_html(rating_label),
        score = escape_html(&score),
        words = escape_html(&words),
        method = escape_html(&page.fetch_method),
        change = escape_html(&format!("{:?}", page.change_status).to_ascii_lowercase()),
        profile = escape_html(&profile),
        page_path = escape_html(&page_path),
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn handle_dashboard_request(stream: &mut TcpStream, root: &Path) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    if method != "GET" && method != "HEAD" {
        write_response(
            stream,
            405,
            "text/plain; charset=utf-8",
            b"method not allowed",
        )?;
        return Ok(());
    }

    let relative = target.split('?').next().unwrap_or("/");
    let relative = percent_decode(relative.trim_start_matches('/'));
    let requested = if relative.as_os_str().is_empty() {
        root.join("dashboard.html")
    } else {
        root.join(relative)
    };
    let path = requested
        .canonicalize()
        .unwrap_or_else(|_| root.join("dashboard.html"));
    let root_canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !path.starts_with(&root_canonical) {
        write_response(stream, 403, "text/plain; charset=utf-8", b"forbidden")?;
        return Ok(());
    }
    let path = if path.is_dir() {
        path.join("dashboard.html")
    } else {
        path
    };

    match fs::read(&path) {
        Ok(bytes) => {
            let content_type = content_type_for_path(&path);
            if method == "HEAD" {
                write_head(stream, 200, content_type, bytes.len())?;
            } else {
                write_response(stream, 200, content_type, &bytes)?;
            }
        }
        Err(_) => {
            write_response(stream, 404, "text/plain; charset=utf-8", b"not found")?;
        }
    }

    Ok(())
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reason_phrase(status),
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

fn write_head(stream: &mut TcpStream, status: u16, content_type: &str, len: usize) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
        reason_phrase(status),
    )?;
    stream.flush()?;
    Ok(())
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    }
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
    {
        "html" => "text/html; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn percent_decode(value: &str) -> PathBuf {
    let mut bytes = Vec::with_capacity(value.len());
    let mut chars = value.as_bytes().iter().copied().peekable();
    while let Some(ch) = chars.next() {
        if ch == b'%' {
            let hi = chars.next();
            let lo = chars.next();
            if let (Some(hi), Some(lo)) = (hi, lo) {
                let hex = [hi, lo];
                if let Ok(text) = std::str::from_utf8(&hex) {
                    if let Ok(value) = u8::from_str_radix(text, 16) {
                        bytes.push(value);
                        continue;
                    }
                }
                bytes.extend_from_slice(&[b'%', hi, lo]);
                continue;
            }
        }
        bytes.push(ch);
    }
    PathBuf::from(String::from_utf8_lossy(&bytes).into_owned())
}

fn dashboard_state_path(paths: &AppPaths, source_name: &str, snapshot_label: &str) -> PathBuf {
    paths.home.join(format!(
        "dashboard-server-{}-{}.json",
        source_name, snapshot_label
    ))
}

fn write_dashboard_state(path: &Path, state: &DashboardServerState) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(state)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn read_dashboard_state(path: &Path) -> Result<DashboardServerState> {
    let body =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&body).with_context(|| format!("failed to parse {}", path.display()))
}

fn is_dashboard_running(state: &DashboardServerState) -> bool {
    TcpStream::connect_timeout(
        &format!("{}:{}", state.host, state.port)
            .parse()
            .unwrap_or_else(|_| ([127, 0, 0, 1], state.port).into()),
        Duration::from_millis(250),
    )
    .is_ok()
}

fn stop_process(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .arg(pid.to_string())
            .status()
            .context("failed to execute kill")?;
        if !status.success() {
            bail!("failed to stop dashboard process `{pid}`");
        }
        return Ok(());
    }

    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .context("failed to execute taskkill")?;
        if !status.success() {
            bail!("failed to stop dashboard process `{pid}`");
        }
        return Ok(());
    }

    #[cfg(not(any(unix, windows)))]
    {
        bail!("dashboard stop is not supported on this platform");
    }

    #[allow(unreachable_code)]
    Ok(())
}

fn dashboard_url(host: &str, port: u16) -> String {
    let display_host = if host == "0.0.0.0" { "127.0.0.1" } else { host };
    format!("http://{display_host}:{port}/")
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
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Result;

    use super::build_dashboard;
    use crate::config::AppPaths;
    use crate::models::{
        DiscoverySummary, FetchSummary, PageChangeStatus, PageManifestEntry, SnapshotManifest,
        SourceKind, VersionStrategy,
    };
    use crate::probe::{DetectedInputKind, SuggestedMode};
    use crate::quality::{PageQualityRating, PageQualitySummary, QualitySummary};

    #[test]
    fn writes_dashboard_html_for_snapshot() -> Result<()> {
        let root = make_temp_dir("dashboard");
        let paths = AppPaths {
            home: root.clone(),
            config_file: root.join("config.json"),
            sources_dir: root.join("sources"),
            snapshots_dir: root.join("snapshots"),
        };
        let snapshot_dir = paths.snapshots_dir.join("demo").join("snap-1");
        fs::create_dir_all(snapshot_dir.join("pages"))?;
        fs::write(snapshot_dir.join("pages/intro.md"), "# Intro\n")?;
        let manifest = SnapshotManifest {
            schema_version: 7,
            created_at: "2026-03-09T00:00:00Z".to_string(),
            source_name: "demo".to_string(),
            entry_url: "https://example.com".to_string(),
            source_kind: SourceKind::Website,
            version_strategy: VersionStrategy::DateSnapshot,
            source_ref: "snap-1".to_string(),
            snapshot_label: "snap-1".to_string(),
            snapshot_dir: snapshot_dir.clone(),
            status: "fetched".to_string(),
            previous_snapshot_label: None,
            detected_input_kind: DetectedInputKind::ContentPage,
            suggested_mode: SuggestedMode::HybridSeed,
            discovery: DiscoverySummary {
                manifest_path: snapshot_dir.join("discovery.json"),
                adapters: vec!["seed_page".to_string()],
                frontier_count: 1,
                llms_index_url: None,
                llms_full_index_url: None,
                sitemap_count: 0,
            },
            git: None,
            fetch: Some(FetchSummary {
                attempted: 1,
                stored_pages: 1,
                skipped_pages: 0,
                reused_pages: 0,
                normalized_pages: 1,
                normalization_changed_pages: 1,
                quality: Some(QualitySummary {
                    pages_scored: 1,
                    high_quality_pages: 1,
                    medium_quality_pages: 0,
                    low_quality_pages: 0,
                    missing_title_pages: 0,
                    residual_markup_pages: 0,
                }),
                method_counts: std::collections::BTreeMap::new(),
            }),
            diff: None,
            pages: vec![PageManifestEntry {
                page_key: "https://example.com/intro".to_string(),
                url: "https://example.com/intro".to_string(),
                final_url: "https://example.com/intro".to_string(),
                fetch_method: "markdown_negotiation".to_string(),
                status: "stored".to_string(),
                change_status: PageChangeStatus::New,
                reused_from_snapshot: None,
                page_path: Some(snapshot_dir.join("pages/intro.md")),
                metadata_path: None,
                raw_path: None,
                rendered_raw_path: None,
                content_type: Some("text/markdown".to_string()),
                sha256: Some("abc".to_string()),
                raw_sha256: None,
                etag: None,
                last_modified: None,
                byte_size: 10,
                normalization: None,
                quality: Some(PageQualitySummary {
                    score: 90,
                    rating: PageQualityRating::High,
                    word_count: 100,
                    non_empty_lines: 10,
                    text_lines: 8,
                    heading_count: 2,
                    code_block_count: 0,
                    link_count: 0,
                    residual_html_tags: 0,
                    residual_mdx_components: 0,
                    title_present: true,
                    text_density: 0.8,
                    low_signal_reasons: Vec::new(),
                }),
            }],
            notes: Vec::new(),
        };
        fs::write(
            snapshot_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;

        let result = build_dashboard(&paths, "demo", Some("snap-1".to_string()), None)?;
        let html = fs::read_to_string(&result.output_path)?;
        assert!(html.contains("docsync dashboard"));
        assert!(html.contains("https://example.com/intro"));
        assert!(html.contains("score 90"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    fn make_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("current time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("docsync-dashboard-{label}-{nanos}"));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }
}
