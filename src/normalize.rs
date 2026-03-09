use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NormalizationSummary {
    pub applied: bool,
    pub normalizer: String,
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
        normalizer: "markdown_cleanup_v1".to_string(),
        ..NormalizationSummary::default()
    };

    let code_fence_re = Regex::new(r"^```([A-Za-z0-9_+-]+).*$").expect("code fence regex");
    let tooltip_re =
        Regex::new(r#"<Tooltip[^>]*>(?P<inner>.*?)</Tooltip>"#).expect("tooltip regex");
    let component_re = Regex::new(r"</?[A-Z][A-Za-z0-9]*(?:\s[^>]*)?>").expect("component regex");
    let title_tag_re = Regex::new(r#"^<(?P<tag>Step|Accordion|Tab|Card)\b(?P<attrs>[^>]*)>\s*$"#)
        .expect("title tag regex");
    let title_attr_re = Regex::new(r#"title="([^"]+)""#).expect("title attr regex");
    let href_attr_re = Regex::new(r#"href="([^"]+)""#).expect("href attr regex");

    let lines = strip_docs_index_block(&original, &mut summary);
    let mut normalized_lines = Vec::new();

    for raw_line in lines {
        let mut line = raw_line.replace('\t', "    ");
        let tooltip_replaced = tooltip_re.replace_all(&line, "$inner").into_owned();
        if tooltip_replaced != line {
            summary.component_wrappers_removed += 1;
            line = tooltip_replaced;
        }
        let trimmed = line.trim();

        if trimmed.is_empty() {
            normalized_lines.push(String::new());
            continue;
        }

        if trimmed.contains("<img ") {
            summary.asset_lines_removed += 1;
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
            let title = title_attr_re
                .captures(attrs)
                .and_then(|value| value.get(1))
                .map(|value| value.as_str().trim())
                .unwrap_or("");
            let href = href_attr_re
                .captures(attrs)
                .and_then(|value| value.get(1))
                .map(|value| value.as_str().trim());
            summary.component_wrappers_removed += 1;

            if !title.is_empty() {
                let heading = match tag {
                    "Tab" => format!("### {title}"),
                    _ => format!("## {title}"),
                };
                normalized_lines.push(heading);
                if tag == "Card" {
                    if let Some(href) = href {
                        normalized_lines.push(format!("Source: {href}"));
                    }
                }
            }
            continue;
        }

        match trimmed {
            "<Info>" | "<Tip>" | "<Warning>" | "<Note>" | "<Check>" => {
                summary.component_wrappers_removed += 1;
                let prefix = match trimmed {
                    "<Tip>" => "Tip:",
                    "<Warning>" => "Warning:",
                    "<Check>" => "Check:",
                    _ => "Note:",
                };
                normalized_lines.push(prefix.to_string());
                continue;
            }
            "<Tabs>" | "</Tabs>" | "<Steps>" | "</Steps>" | "<Columns>" | "</Columns>"
            | "<CardGroup>" | "</CardGroup>" | "<AccordionGroup>" | "</AccordionGroup>"
            | "</Info>" | "</Tip>" | "</Warning>" | "</Note>" | "</Check>" | "</Step>"
            | "</Accordion>" | "</Tab>" | "</Card>" => {
                summary.component_wrappers_removed += 1;
                continue;
            }
            _ => {}
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
}
