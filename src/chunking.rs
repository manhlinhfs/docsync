use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::incremental::sha256_hex;
use crate::models::{ChunkManifestEntry, ChunkingSummary};
use crate::util::ensure_directory;

#[derive(Debug, Clone, Copy)]
pub struct ChunkingConfig {
    pub target_words: usize,
    pub overlap_words: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            target_words: 220,
            overlap_words: 40,
        }
    }
}

#[derive(Debug, Clone)]
struct ChunkDraft {
    heading: Option<String>,
    section_path: Vec<String>,
    markdown: String,
    word_count: usize,
}

#[derive(Debug, Clone)]
struct SectionDraft {
    heading: Option<String>,
    section_path: Vec<String>,
    lines: Vec<String>,
}

#[derive(Debug, Clone)]
struct Block {
    markdown: String,
    word_count: usize,
}

pub fn write_markdown_chunks(
    chunks_root: &Path,
    stem: &str,
    page_key: &str,
    markdown: &str,
    config: ChunkingConfig,
) -> Result<Vec<ChunkManifestEntry>> {
    ensure_directory(chunks_root)?;
    let drafts = split_markdown_into_chunks(markdown, config);
    let mut entries = Vec::new();

    for (index, draft) in drafts.into_iter().enumerate() {
        let ordinal = index + 1;
        let chunk_path = chunk_path(chunks_root, stem, ordinal);
        if let Some(parent) = chunk_path.parent() {
            ensure_directory(parent)?;
        }
        fs::write(&chunk_path, draft.markdown.as_bytes())
            .with_context(|| format!("failed to write {}", chunk_path.display()))?;
        let sha256 = sha256_hex(draft.markdown.as_bytes());
        let byte_size = chunk_path
            .metadata()
            .with_context(|| format!("failed to stat {}", chunk_path.display()))?
            .len();
        let chunk_id = sha256_hex(
            format!("{page_key}::{ordinal}::{}", draft.section_path.join(" > ")).as_bytes(),
        );

        entries.push(ChunkManifestEntry {
            chunk_id,
            page_key: page_key.to_string(),
            ordinal,
            heading: draft.heading,
            section_path: draft.section_path,
            chunk_path,
            sha256,
            byte_size,
            word_count: draft.word_count,
        });
    }

    Ok(entries)
}

pub fn summarize_chunks(entries: &[ChunkManifestEntry], config: ChunkingConfig) -> ChunkingSummary {
    let chunk_count = entries.len();
    let mut unique_pages = std::collections::BTreeSet::new();
    for entry in entries {
        unique_pages.insert(entry.page_key.clone());
    }
    let chunked_pages = unique_pages.len();
    let average_chunks_per_page = if chunked_pages == 0 {
        0.0
    } else {
        chunk_count as f32 / chunked_pages as f32
    };

    ChunkingSummary {
        chunk_count,
        chunked_pages,
        average_chunks_per_page,
        target_words: config.target_words,
        overlap_words: config.overlap_words,
    }
}

fn split_markdown_into_chunks(markdown: &str, config: ChunkingConfig) -> Vec<ChunkDraft> {
    let sections = split_into_sections(markdown);
    let mut chunks = Vec::new();

    for section in sections {
        chunks.extend(split_section(section, config));
    }

    if chunks.is_empty() {
        let trimmed = markdown.trim();
        if !trimmed.is_empty() {
            chunks.push(ChunkDraft {
                heading: None,
                section_path: Vec::new(),
                markdown: trimmed.to_string(),
                word_count: count_words(trimmed),
            });
        }
    }

    chunks
}

fn split_into_sections(markdown: &str) -> Vec<SectionDraft> {
    let mut sections = Vec::new();
    let mut current = SectionDraft {
        heading: None,
        section_path: Vec::new(),
        lines: Vec::new(),
    };
    let mut heading_stack: Vec<String> = Vec::new();

    for raw_line in markdown.lines() {
        let line = raw_line.to_string();
        if let Some((level, heading)) = parse_heading(raw_line) {
            if !current.lines.is_empty() {
                sections.push(current);
            }

            heading_stack.truncate(level.saturating_sub(1));
            heading_stack.push(heading.clone());
            current = SectionDraft {
                heading: Some(heading.clone()),
                section_path: heading_stack.clone(),
                lines: vec![line],
            };
        } else {
            current.lines.push(line);
        }
    }

    if !current.lines.is_empty() {
        sections.push(current);
    }

    sections
}

fn split_section(section: SectionDraft, config: ChunkingConfig) -> Vec<ChunkDraft> {
    let blocks = split_into_blocks(&section.lines);
    if blocks.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut start = 0usize;

    while start < blocks.len() {
        let mut end = start;
        let mut words = 0usize;

        while end < blocks.len() && (words < config.target_words || end == start) {
            words += blocks[end].word_count.max(1);
            end += 1;
        }

        let markdown = blocks[start..end]
            .iter()
            .map(|block| block.markdown.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
            .trim()
            .to_string();

        if !markdown.is_empty() {
            chunks.push(ChunkDraft {
                heading: section.heading.clone(),
                section_path: section.section_path.clone(),
                word_count: count_words(&markdown),
                markdown,
            });
        }

        if end >= blocks.len() {
            break;
        }

        let mut next_start = end;
        let mut overlap = 0usize;
        while next_start > start {
            let candidate = next_start - 1;
            let block_words = blocks[candidate].word_count.max(1);
            if overlap >= config.overlap_words && candidate < end - 1 {
                break;
            }
            overlap += block_words;
            next_start = candidate;
            if overlap >= config.overlap_words {
                break;
            }
        }

        if next_start == start {
            start = end;
        } else {
            start = next_start;
        }
    }

    chunks
}

fn split_into_blocks(lines: &[String]) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut current = Vec::new();
    let mut in_code_fence = false;

    for line in lines {
        let trimmed = line.trim();
        let is_code_fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");

        if is_code_fence {
            current.push(line.clone());
            if in_code_fence {
                push_block(&mut blocks, &mut current);
            }
            in_code_fence = !in_code_fence;
            continue;
        }

        if in_code_fence {
            current.push(line.clone());
            continue;
        }

        if trimmed.is_empty() {
            push_block(&mut blocks, &mut current);
            continue;
        }

        current.push(line.clone());
    }

    push_block(&mut blocks, &mut current);
    blocks
}

fn push_block(blocks: &mut Vec<Block>, current: &mut Vec<String>) {
    if current.is_empty() {
        return;
    }
    let markdown = current.join("\n").trim().to_string();
    if !markdown.is_empty() {
        blocks.push(Block {
            word_count: count_words(&markdown),
            markdown,
        });
    }
    current.clear();
}

fn chunk_path(root: &Path, stem: &str, ordinal: usize) -> PathBuf {
    let stem = stem.strip_suffix(".md").unwrap_or(stem);
    root.join(format!("{stem}__chunk-{ordinal:03}.md"))
}

fn parse_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let title = trimmed[level..].trim();
    if title.is_empty() {
        return None;
    }
    Some((level, title.trim_matches('#').trim().to_string()))
}

fn count_words(markdown: &str) -> usize {
    markdown.split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::{ChunkingConfig, split_markdown_into_chunks};

    #[test]
    fn splits_markdown_by_heading_structure() {
        let markdown = "\
# Intro\n\
Overview text for intro.\n\
\n\
## Install\n\
Install steps go here with enough words to keep the section separate.\n\
\n\
## Query\n\
Query examples live here.\n";

        let chunks = split_markdown_into_chunks(
            markdown,
            ChunkingConfig {
                target_words: 8,
                overlap_words: 2,
            },
        );

        assert!(chunks.len() >= 3);
        assert_eq!(chunks[0].section_path, vec!["Intro".to_string()]);
        assert_eq!(
            chunks[1].section_path,
            vec!["Intro".to_string(), "Install".to_string()]
        );
    }

    #[test]
    fn keeps_code_fences_in_one_chunk() {
        let markdown = "\
## Example\n\
Use this snippet.\n\
\n\
```ts\n\
const x = 1;\n\
const y = 2;\n\
```\n\
\n\
More explanation after the code block.\n";

        let chunks = split_markdown_into_chunks(
            markdown,
            ChunkingConfig {
                target_words: 6,
                overlap_words: 2,
            },
        );

        assert!(chunks.iter().any(|chunk| chunk.markdown.contains("```ts")));
        assert!(
            chunks
                .iter()
                .all(|chunk| !chunk.markdown.contains("const x = 1;\n\nconst y = 2;"))
        );
    }
}
