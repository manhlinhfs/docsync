use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::config::AppPaths;
use crate::models::{PageManifestEntry, SnapshotManifest};
use crate::quality::PageQualityRating;

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
