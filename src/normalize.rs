use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NormalizationSummary {
    pub applied: bool,
    pub normalizer: String,
    #[serde(default)]
    pub profiles_applied: Vec<String>,
    pub changed: bool,
    pub boilerplate_blocks_removed: usize,
    pub component_wrappers_removed: usize,
    pub duplicate_headings_removed: usize,
    pub asset_lines_removed: usize,
    pub blank_lines_collapsed: usize,
}

#[derive(Debug, Clone)]
pub struct NormalizedDocument {
    pub markdown: String,
    pub summary: NormalizationSummary,
}

pub fn normalize_markdown(input: &str) -> NormalizedDocument {
    let original = normalize_line_endings(input);
    let mut summary = NormalizationSummary {
        applied: true,
        normalizer: "markdown_cleanup_v2".to_string(),
        ..NormalizationSummary::default()
    };

    let code_fence_re = Regex::new(r"^```([A-Za-z0-9_+-]+).*$").expect("code fence regex");
    let tooltip_re =
        Regex::new(r#"<Tooltip[^>]*>(?P<inner>.*?)</Tooltip>"#).expect("tooltip regex");
    let summary_tag_re =
        Regex::new(r#"^<summary>(?P<inner>.+)</summary>$"#).expect("summary tag regex");
    let component_re = Regex::new(r"</?[A-Z][A-Za-z0-9]*(?:\s[^>]*)?>").expect("component regex");
    let title_tag_re = Regex::new(
        r#"^<(?P<tag>Step|Accordion|Tab|TabItem|Card|Details|Callout|VPCard)\b(?P<attrs>[^>]*)/?>\s*$"#,
    )
    .expect("title tag regex");
    let mdx_import_export_re =
        Regex::new(r"^(?:import\s.+;|export\s+(?:const|default|function|class)\b.*)$")
            .expect("mdx import/export regex");
    let jsx_comment_re = Regex::new(r"^\s*\{/\*.*\*/\}\s*$").expect("jsx comment regex");
    let html_comment_re = Regex::new(r"^\s*<!--.*-->\s*$").expect("html comment regex");
    let gitbook_tag_re = Regex::new(
        r#"^\{%\s*(?P<tag>hint|endhint|tabs|endtabs|tab|endtab)\b(?P<attrs>.*?)%\}\s*$"#,
    )
    .expect("gitbook tag regex");
    let mkdocs_tab_re =
        Regex::new(r#"^===\s+["'](?P<title>.+?)["']\s*$"#).expect("mkdocs tab regex");
    let mkdocs_admonition_re =
        Regex::new(r#"^(?:!!!|\?\?\?)\s+(?P<kind>[A-Za-z_-]+)(?:\s+["'](?P<title>.+?)["'])?\s*$"#)
            .expect("mkdocs admonition regex");
    let title_attr_re = Regex::new(r#"title="([^"]+)""#).expect("title attr regex");
    let label_attr_re = Regex::new(r#"label="([^"]+)""#).expect("label attr regex");
    let value_attr_re = Regex::new(r#"value="([^"]+)""#).expect("value attr regex");
    let summary_attr_re = Regex::new(r#"summary="([^"]+)""#).expect("summary attr regex");
    let type_attr_re = Regex::new(r#"type="([^"]+)""#).expect("type attr regex");
    let style_attr_re = Regex::new(r#"style="([^"]+)""#).expect("style attr regex");
    let href_attr_re = Regex::new(r#"href="([^"]+)""#).expect("href attr regex");
    let text_attr_re = Regex::new(r#"text="([^"]+)""#).expect("text attr regex");
    let desc_attr_re = Regex::new(r#"desc="([^"]+)""#).expect("desc attr regex");

    let lines = strip_docs_index_block(&original, &mut summary);
    let mut normalized_lines = Vec::new();
    let mut active_dedent = false;

    for raw_line in lines {
        let mut line = raw_line.replace('\t', "    ");
        if active_dedent {
            if line.trim().is_empty() {
                normalized_lines.push(String::new());
                continue;
            }
            let leading = leading_spaces(&line);
            if leading >= 4 {
                line = line[4..].to_string();
            } else if !is_explicit_close_marker(line.trim()) {
                active_dedent = false;
            }
        }
        let tooltip_replaced = tooltip_re.replace_all(&line, "$inner").into_owned();
        if tooltip_replaced != line {
            summary.component_wrappers_removed += 1;
            record_profile(&mut summary, "mintlify");
            line = tooltip_replaced;
        }
        let trimmed = line.trim();

        if trimmed.is_empty() {
            normalized_lines.push(String::new());
            continue;
        }

        if mdx_import_export_re.is_match(trimmed)
            || jsx_comment_re.is_match(trimmed)
            || html_comment_re.is_match(trimmed)
        {
            summary.component_wrappers_removed += 1;
            record_profile(&mut summary, "mdx");
            continue;
        }

        if trimmed.contains("<img ") {
            summary.asset_lines_removed += 1;
            continue;
        }

        if let Some(captures) = gitbook_tag_re.captures(trimmed) {
            let tag = captures
                .name("tag")
                .map(|value| value.as_str())
                .unwrap_or("");
            let attrs = captures
                .name("attrs")
                .map(|value| value.as_str())
                .unwrap_or("");
            summary.component_wrappers_removed += 1;
            record_profile(&mut summary, "gitbook");

            match tag {
                "hint" => {
                    normalized_lines.push(callout_prefix(
                        read_attr(attrs, &type_attr_re)
                            .or_else(|| read_attr(attrs, &style_attr_re)),
                        read_attr(attrs, &title_attr_re),
                    ));
                    active_dedent = true;
                }
                "tab" => {
                    let title = read_attr(attrs, &title_attr_re)
                        .or_else(|| read_attr(attrs, &label_attr_re))
                        .unwrap_or_else(|| "Tab".to_string());
                    normalized_lines.push(format!("### {title}"));
                    active_dedent = true;
                }
                "tabs" => active_dedent = true,
                "endhint" | "endtabs" | "endtab" => active_dedent = false,
                _ => {}
            }
            continue;
        }

        if let Some(captures) = mkdocs_tab_re.captures(trimmed) {
            let title = captures
                .name("title")
                .map(|value| value.as_str().trim())
                .unwrap_or("Tab");
            summary.component_wrappers_removed += 1;
            record_profile(&mut summary, "mkdocs");
            normalized_lines.push(format!("### {title}"));
            active_dedent = true;
            continue;
        }

        if let Some(captures) = mkdocs_admonition_re.captures(trimmed) {
            let kind = captures
                .name("kind")
                .map(|value| value.as_str())
                .unwrap_or("note");
            let title = captures
                .name("title")
                .map(|value| value.as_str().trim().to_string());
            summary.component_wrappers_removed += 1;
            record_profile(&mut summary, "mkdocs");
            normalized_lines.push(callout_prefix(Some(kind.to_string()), title));
            active_dedent = true;
            continue;
        }

        if let Some(captures) = title_tag_re.captures(trimmed) {
            let tag = captures
                .name("tag")
                .map(|value| value.as_str())
                .unwrap_or("");
            let attrs = captures
                .name("attrs")
                .map(|value| value.as_str())
                .unwrap_or("");
            summary.component_wrappers_removed += 1;
            record_profile(&mut summary, profile_for_component_tag(tag));

            let title = read_attr(attrs, &title_attr_re)
                .or_else(|| read_attr(attrs, &label_attr_re))
                .or_else(|| read_attr(attrs, &value_attr_re))
                .or_else(|| read_attr(attrs, &summary_attr_re))
                .or_else(|| read_attr(attrs, &text_attr_re));
            let href = read_attr(attrs, &href_attr_re);
            let desc = read_attr(attrs, &desc_attr_re);

            match tag {
                "Callout" => {
                    normalized_lines.push(callout_prefix(read_attr(attrs, &type_attr_re), title))
                }
                "Tab" | "TabItem" => {
                    if let Some(title) = title {
                        normalized_lines.push(format!("### {title}"));
                    }
                }
                "Card" | "VPCard" => {
                    if let Some(title) = title {
                        normalized_lines.push(format!("## {title}"));
                    }
                    if let Some(href) = href {
                        normalized_lines.push(format!("Source: {href}"));
                    }
                    if let Some(desc) = desc {
                        normalized_lines.push(desc);
                    }
                }
                _ => {
                    if let Some(title) = title {
                        normalized_lines.push(format!("## {title}"));
                    }
                }
            }
            active_dedent = should_dedent_after_component(tag);
            continue;
        }

        if let Some(captures) = summary_tag_re.captures(trimmed) {
            let inner = captures
                .name("inner")
                .map(|value| value.as_str().trim())
                .unwrap_or("Details");
            summary.component_wrappers_removed += 1;
            record_profile(&mut summary, "docusaurus");
            normalized_lines.push(format!("## {inner}"));
            active_dedent = true;
            continue;
        }

        match trimmed {
            "<Info>" | "<Tip>" | "<Warning>" | "<Note>" | "<Check>" => {
                summary.component_wrappers_removed += 1;
                record_profile(&mut summary, "mintlify");
                let prefix = match trimmed {
                    "<Tip>" => "Tip:",
                    "<Warning>" => "Warning:",
                    "<Check>" => "Check:",
                    _ => "Note:",
                };
                normalized_lines.push(prefix.to_string());
                active_dedent = true;
                continue;
            }
            "<Tabs>" | "</Tabs>" | "<Steps>" | "</Steps>" | "<Columns>" | "</Columns>"
            | "<CardGroup>" | "</CardGroup>" | "<AccordionGroup>" | "</AccordionGroup>"
            | "<details>" | "</details>" | "<Badge>" | "</Badge>" | "</Info>" | "</Tip>"
            | "</Warning>" | "</Note>" | "</Check>" | "</Step>" | "</Accordion>" | "</Tab>"
            | "</TabItem>" | "</Card>" | "</Details>" | "</Callout>" | "</VPCard>" => {
                summary.component_wrappers_removed += 1;
                if trimmed.contains("details") {
                    record_profile(&mut summary, "docusaurus");
                } else {
                    record_profile(&mut summary, "mintlify");
                }
                active_dedent = false;
                continue;
            }
            _ => {}
        }

        if trimmed.starts_with("<Badge ") || trimmed.starts_with("<VPBadge ") {
            summary.component_wrappers_removed += 1;
            record_profile(&mut summary, "vitepress");
            continue;
        }

        if trimmed.starts_with('<')
            && trimmed.ends_with('>')
            && trimmed
                .trim_start_matches("</")
                .trim_start_matches('<')
                .split_whitespace()
                .next()
                .is_some_and(is_component_name)
        {
            summary.component_wrappers_removed += 1;
            record_profile(&mut summary, "mdx");
            continue;
        }

        if let Some(captures) = code_fence_re.captures(trimmed) {
            let language = captures.get(1).map(|value| value.as_str()).unwrap_or("");
            if trimmed != format!("```{language}") {
                summary.component_wrappers_removed += 1;
            }
            normalized_lines.push(format!("```{language}"));
            continue;
        }

        let stripped = component_re.replace_all(&line, "").into_owned();
        if stripped != line {
            summary.component_wrappers_removed += 1;
        }
        let final_line = stripped.trim_end().to_string();
        if final_line.is_empty() {
            summary.component_wrappers_removed += 1;
            continue;
        }
        normalized_lines.push(final_line);
    }

    let deduped_lines = dedupe_duplicate_headings(normalized_lines, &mut summary);
    let collapsed_lines = collapse_blank_lines(deduped_lines, &mut summary);
    let markdown = if collapsed_lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", collapsed_lines.join("\n").trim())
    };
    summary.changed = markdown != original;

    NormalizedDocument { markdown, summary }
}

fn strip_docs_index_block(input: &str, summary: &mut NormalizationSummary) -> Vec<String> {
    let lines = input
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<String>>();

    if lines.len() < 3 {
        return lines;
    }

    let first_non_empty = lines.iter().position(|line| !line.trim().is_empty());
    let Some(start) = first_non_empty else {
        return lines;
    };
    if !lines[start].trim().starts_with("> ## Documentation Index") {
        return lines;
    }
    if !lines
        .iter()
        .skip(start)
        .take(5)
        .any(|line| line.contains("llms.txt"))
    {
        return lines;
    }

    let mut end = start;
    while end < lines.len()
        && (lines[end].trim_start().starts_with('>') || lines[end].trim().is_empty())
    {
        end += 1;
    }
    summary.boilerplate_blocks_removed += 1;
    summary.component_wrappers_removed += end.saturating_sub(start);

    lines[..start]
        .iter()
        .chain(lines[end..].iter())
        .cloned()
        .collect()
}

fn dedupe_duplicate_headings(
    lines: Vec<String>,
    summary: &mut NormalizationSummary,
) -> Vec<String> {
    let mut deduped = Vec::with_capacity(lines.len());
    let mut previous_heading: Option<String> = None;

    for line in lines {
        let trimmed = line.trim();
        let is_heading = trimmed.starts_with('#');
        if is_heading
            && previous_heading
                .as_deref()
                .is_some_and(|previous| previous == trimmed)
        {
            summary.duplicate_headings_removed += 1;
            continue;
        }
        if is_heading {
            previous_heading = Some(trimmed.to_string());
        } else if !trimmed.is_empty() {
            previous_heading = None;
        }
        deduped.push(line);
    }

    deduped
}

fn collapse_blank_lines(lines: Vec<String>, summary: &mut NormalizationSummary) -> Vec<String> {
    let mut collapsed = Vec::with_capacity(lines.len());
    let mut last_blank = false;

    for line in lines {
        let blank = line.trim().is_empty();
        if blank && last_blank {
            summary.blank_lines_collapsed += 1;
            continue;
        }
        last_blank = blank;
        collapsed.push(line);
    }

    collapsed
}

fn normalize_line_endings(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n");
    format!("{}\n", normalized.trim())
}

fn record_profile(summary: &mut NormalizationSummary, profile: &str) {
    if !summary
        .profiles_applied
        .iter()
        .any(|value| value == profile)
    {
        summary.profiles_applied.push(profile.to_string());
    }
}

fn leading_spaces(value: &str) -> usize {
    value.chars().take_while(|ch| *ch == ' ').count()
}

fn read_attr(attrs: &str, re: &Regex) -> Option<String> {
    re.captures(attrs)
        .and_then(|value| value.get(1))
        .map(|value| value.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn profile_for_component_tag(tag: &str) -> &'static str {
    match tag {
        "TabItem" => "docusaurus",
        "Callout" | "Details" => "nextra",
        "VPCard" => "vitepress",
        _ => "mintlify",
    }
}

fn should_dedent_after_component(tag: &str) -> bool {
    matches!(
        tag,
        "Step" | "Accordion" | "Tab" | "TabItem" | "Card" | "Details" | "Callout" | "VPCard"
    )
}

fn callout_prefix(kind: Option<String>, title: Option<String>) -> String {
    let prefix = match kind
        .as_deref()
        .unwrap_or("note")
        .trim_matches('"')
        .to_ascii_lowercase()
        .as_str()
    {
        "tip" | "success" => "Tip",
        "warning" | "danger" | "caution" => "Warning",
        "check" => "Check",
        "info" | "note" => "Note",
        "important" => "Important",
        "example" => "Example",
        other => return format!("{}:", title.unwrap_or_else(|| title_case(other))),
    };
    match title {
        Some(title) if !title.is_empty() => format!("{prefix}: {title}"),
        _ => format!("{prefix}:"),
    }
}

fn title_case(value: &str) -> String {
    let mut output = String::new();
    for (index, part) in value
        .split(['-', '_', ' '])
        .filter(|part| !part.is_empty())
        .enumerate()
    {
        if index > 0 {
            output.push(' ');
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            output.push(first.to_ascii_uppercase());
            output.push_str(chars.as_str());
        }
    }
    output
}

fn is_explicit_close_marker(trimmed: &str) -> bool {
    matches!(
        trimmed,
        "</Info>"
            | "</Tip>"
            | "</Warning>"
            | "</Note>"
            | "</Check>"
            | "</Tabs>"
            | "</Steps>"
            | "</Columns>"
            | "</CardGroup>"
            | "</AccordionGroup>"
            | "</Step>"
            | "</Accordion>"
            | "</Tab>"
            | "</TabItem>"
            | "</Card>"
            | "</Details>"
            | "</Callout>"
            | "</VPCard>"
            | "</details>"
    ) || trimmed.starts_with("{% end")
}

fn is_component_name(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::normalize_markdown;

    #[test]
    fn strips_docs_index_and_duplicate_heading() {
        let input = "\
> ## Documentation Index\n\
> Fetch the complete documentation index at: https://docs.example.com/llms.txt\n\
> Use this file to discover all available pages before exploring further.\n\
\n\
# Intro\n\
# Intro\n\
\n\
Hello\n";
        let normalized = normalize_markdown(input);
        assert!(!normalized.markdown.contains("Documentation Index"));
        assert_eq!(normalized.summary.boilerplate_blocks_removed, 1);
        assert_eq!(normalized.summary.duplicate_headings_removed, 1);
        assert!(normalized.markdown.contains("# Intro\n\nHello"));
    }

    #[test]
    fn flattens_mdx_components_and_code_fence_metadata() {
        let input = "\
<Info>\n\
  Fast path\n\
</Info>\n\
<Tabs>\n\
  <Tab title=\"macOS\">\n\
    ```bash theme={\"theme\":{\"light\":\"min-light\"}}\n\
    brew install openclaw\n\
    ```\n\
  </Tab>\n\
</Tabs>\n\
        <Tooltip headline=\"Gateway host\" tip=\"Machine\">gateway host</Tooltip>\n\
<img src=\"https://cdn.example.com/demo.png\" alt=\"Demo\" />\n";
        let normalized = normalize_markdown(input);
        assert!(normalized.markdown.contains("Note:"));
        assert!(normalized.markdown.contains("Fast path"));
        assert!(normalized.markdown.contains("### macOS"));
        assert!(normalized.markdown.contains("```bash"));
        assert!(normalized.markdown.contains("gateway host"));
        assert!(!normalized.markdown.contains("<Tooltip"));
        assert!(!normalized.markdown.contains("<img"));
        assert!(
            normalized
                .summary
                .profiles_applied
                .iter()
                .any(|value| value == "mintlify")
        );
    }

    #[test]
    fn turns_cards_into_headings_with_source_links() {
        let input = "\
<Card title=\"Dashboard\" href=\"/web/dashboard\">\n\
  Open the dashboard.\n\
</Card>\n";
        let normalized = normalize_markdown(input);
        assert!(normalized.markdown.contains("## Dashboard"));
        assert!(normalized.markdown.contains("Source: /web/dashboard"));
        assert!(normalized.markdown.contains("Open the dashboard."));
    }

    #[test]
    fn normalizes_docusaurus_tabs_and_mdx_imports() {
        let input = "\
import Tabs from '@theme/Tabs';\n\
import TabItem from '@theme/TabItem';\n\
<Tabs>\n\
  <TabItem value=\"npm\" label=\"npm\">\n\
    npm install docsync\n\
  </TabItem>\n\
</Tabs>\n\
<details>\n\
<summary>Advanced</summary>\n\
  Extra flags\n\
</details>\n";
        let normalized = normalize_markdown(input);
        assert!(!normalized.markdown.contains("import Tabs"));
        assert!(normalized.markdown.contains("### npm"));
        assert!(normalized.markdown.contains("npm install docsync"));
        assert!(normalized.markdown.contains("## Advanced"));
        assert!(normalized.markdown.contains("Extra flags"));
        assert!(
            normalized
                .summary
                .profiles_applied
                .iter()
                .any(|value| value == "docusaurus")
        );
    }

    #[test]
    fn normalizes_gitbook_tabs_and_hints() {
        let input = "\
{% hint style=\"warning\" %}\n\
Be careful.\n\
{% endhint %}\n\
\n\
{% tabs %}\n\
{% tab title=\"Node.js\" %}\n\
    npm install docsync\n\
{% endtab %}\n\
{% endtabs %}\n";
        let normalized = normalize_markdown(input);
        assert!(normalized.markdown.contains("Warning:"));
        assert!(normalized.markdown.contains("Be careful."));
        assert!(normalized.markdown.contains("### Node.js"));
        assert!(normalized.markdown.contains("npm install docsync"));
        assert!(!normalized.markdown.contains("{%"));
        assert!(
            normalized
                .summary
                .profiles_applied
                .iter()
                .any(|value| value == "gitbook")
        );
    }

    #[test]
    fn normalizes_mkdocs_admonitions_and_tabs() {
        let input = "\
!!! note \"Heads up\"\n\
    Keep this safe.\n\
\n\
=== \"Python\"\n\
    pip install docsync\n";
        let normalized = normalize_markdown(input);
        assert!(normalized.markdown.contains("Note: Heads up"));
        assert!(normalized.markdown.contains("Keep this safe."));
        assert!(normalized.markdown.contains("### Python"));
        assert!(normalized.markdown.contains("pip install docsync"));
        assert!(!normalized.markdown.contains("    pip install docsync"));
        assert!(
            normalized
                .summary
                .profiles_applied
                .iter()
                .any(|value| value == "mkdocs")
        );
    }

    #[test]
    fn normalizes_nextra_and_vitepress_components() {
        let input = "\
<Callout type=\"warning\" title=\"Beta\">\n\
  Experimental API\n\
</Callout>\n\
<Details summary=\"Advanced\">\n\
  Hidden flags\n\
</Details>\n\
<VPCard title=\"CLI\" href=\"/cli\" desc=\"Command reference\" />\n\
<Badge text=\"Beta\" />\n";
        let normalized = normalize_markdown(input);
        assert!(normalized.markdown.contains("Warning: Beta"));
        assert!(normalized.markdown.contains("Experimental API"));
        assert!(normalized.markdown.contains("## Advanced"));
        assert!(normalized.markdown.contains("Hidden flags"));
        assert!(normalized.markdown.contains("## CLI"));
        assert!(normalized.markdown.contains("Source: /cli"));
        assert!(normalized.markdown.contains("Command reference"));
        assert!(!normalized.markdown.contains("<Badge"));
        assert!(
            normalized
                .summary
                .profiles_applied
                .iter()
                .any(|value| value == "nextra")
        );
        assert!(
            normalized
                .summary
                .profiles_applied
                .iter()
                .any(|value| value == "vitepress")
        );
    }
}
