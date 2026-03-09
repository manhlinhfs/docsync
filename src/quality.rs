use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PageQualityRating {
    High,
    Medium,
    #[default]
    Low,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageQualitySummary {
    pub score: u8,
    pub rating: PageQualityRating,
    pub word_count: usize,
    pub non_empty_lines: usize,
    pub text_lines: usize,
    pub heading_count: usize,
    pub code_block_count: usize,
    pub link_count: usize,
    pub residual_html_tags: usize,
    pub residual_mdx_components: usize,
    pub title_present: bool,
    pub text_density: f32,
    #[serde(default)]
    pub low_signal_reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualitySummary {
    pub pages_scored: usize,
    pub high_quality_pages: usize,
    pub medium_quality_pages: usize,
    pub low_quality_pages: usize,
    pub missing_title_pages: usize,
    pub residual_markup_pages: usize,
}

pub fn score_markdown_quality(markdown: &str) -> PageQualitySummary {
    let word_re = Regex::new(r"[A-Za-z0-9][A-Za-z0-9'_-]*").expect("word regex");
    let link_re = Regex::new(r"\[[^\]]+\]\([^)]+\)").expect("link regex");
    let html_tag_re = Regex::new(r"<[a-z][^>]*>").expect("html tag regex");
    let mdx_component_re =
        Regex::new(r"</?[A-Z][A-Za-z0-9]*(?:\s[^>]*)?>").expect("mdx component regex");

    let lines = markdown.lines().collect::<Vec<_>>();
    let non_empty_lines = lines.iter().filter(|line| !line.trim().is_empty()).count();
    let text_lines = lines
        .iter()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with("```")
                && trimmed.chars().any(|ch| ch.is_ascii_alphanumeric())
        })
        .count();
    let heading_count = lines
        .iter()
        .filter(|line| line.trim_start().starts_with('#'))
        .count();
    let code_block_count = lines
        .iter()
        .filter(|line| line.trim_start().starts_with("```"))
        .count()
        / 2;
    let word_count = word_re.find_iter(markdown).count();
    let link_count = link_re.find_iter(markdown).count();
    let residual_html_tags = html_tag_re.find_iter(markdown).count();
    let residual_mdx_components = mdx_component_re.find_iter(markdown).count();
    let title_present = lines
        .iter()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line.trim_start().starts_with("# "));
    let text_density = if non_empty_lines == 0 {
        0.0
    } else {
        text_lines as f32 / non_empty_lines as f32
    };

    let mut score = 100i32;
    let mut low_signal_reasons = Vec::new();

    if !title_present {
        score -= 12;
        low_signal_reasons.push("missing_title".to_string());
    }

    if word_count == 0 {
        score = 0;
        low_signal_reasons.push("empty_content".to_string());
    } else if word_count < 15 {
        score -= 45;
        low_signal_reasons.push("very_short".to_string());
    } else if word_count < 40 {
        score -= 25;
        low_signal_reasons.push("short_content".to_string());
    } else if word_count < 80 {
        score -= 10;
    }

    if non_empty_lines <= 3 {
        score -= 20;
        low_signal_reasons.push("too_few_lines".to_string());
    }

    if text_density < 0.45 {
        score -= 20;
        low_signal_reasons.push("low_text_density".to_string());
    } else if text_density < 0.60 {
        score -= 10;
    }

    if residual_html_tags > 0 {
        score -= (residual_html_tags as i32 * 3).min(24);
        low_signal_reasons.push("residual_html".to_string());
    }
    if residual_mdx_components > 0 {
        score -= (residual_mdx_components as i32 * 4).min(20);
        low_signal_reasons.push("residual_mdx".to_string());
    }
    if heading_count == 0 && word_count > 60 {
        score -= 8;
        low_signal_reasons.push("weak_structure".to_string());
    }

    let score = score.clamp(0, 100) as u8;
    let rating = if score >= 75 {
        PageQualityRating::High
    } else if score >= 45 {
        PageQualityRating::Medium
    } else {
        PageQualityRating::Low
    };

    PageQualitySummary {
        score,
        rating,
        word_count,
        non_empty_lines,
        text_lines,
        heading_count,
        code_block_count,
        link_count,
        residual_html_tags,
        residual_mdx_components,
        title_present,
        text_density,
        low_signal_reasons,
    }
}

pub fn summarize_quality<'a, I>(pages: I) -> QualitySummary
where
    I: IntoIterator<Item = &'a PageQualitySummary>,
{
    let mut summary = QualitySummary::default();

    for page in pages {
        summary.pages_scored += 1;
        match page.rating {
            PageQualityRating::High => summary.high_quality_pages += 1,
            PageQualityRating::Medium => summary.medium_quality_pages += 1,
            PageQualityRating::Low => summary.low_quality_pages += 1,
        }
        if !page.title_present {
            summary.missing_title_pages += 1;
        }
        if page.residual_html_tags > 0 || page.residual_mdx_components > 0 {
            summary.residual_markup_pages += 1;
        }
    }

    summary
}

pub fn is_low_signal(page: &PageQualitySummary) -> bool {
    page.rating == PageQualityRating::Low && (page.word_count < 40 || page.score < 30)
}

#[cfg(test)]
mod tests {
    use super::{PageQualityRating, is_low_signal, score_markdown_quality};

    #[test]
    fn scores_rich_markdown_as_high_quality() {
        let markdown = "# Intro\n\nThis page explains the runtime layout for docsync and shows how incremental sync keeps imports efficient.\n\n## Install\n\nRun `docsync init` first.\n";
        let quality = score_markdown_quality(markdown);
        assert_eq!(quality.rating, PageQualityRating::High);
        assert!(quality.score >= 75);
        assert!(quality.title_present);
    }

    #[test]
    fn marks_thin_markup_heavy_content_as_low_signal() {
        let markdown = "<div>noise</div>\n<Component foo=\"bar\" />\nshort\n";
        let quality = score_markdown_quality(markdown);
        assert_eq!(quality.rating, PageQualityRating::Low);
        assert!(is_low_signal(&quality));
        assert!(
            quality
                .low_signal_reasons
                .iter()
                .any(|value| value == "residual_html")
        );
    }
}
