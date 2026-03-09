use anyhow::{Result, bail};

use crate::config::AppPaths;
use crate::models::{AppConfig, NewSource, SourceDefinition};
use crate::util::{dedupe_tags, normalize_url, now_utc_rfc3339, validate_source_name};

pub fn add_source(
    config: &mut AppConfig,
    paths: &AppPaths,
    new_source: NewSource,
) -> Result<SourceDefinition> {
    validate_source_name(&new_source.name)?;
    let entry_url = normalize_url(&new_source.entry_url)?;
    let now = now_utc_rfc3339();

    if config.sources.contains_key(&new_source.name) {
        bail!("source `{}` already exists", new_source.name);
    }

    let source = SourceDefinition {
        name: new_source.name.clone(),
        entry_url: entry_url.to_string(),
        proxy_url: new_source.proxy_url,
        source_kind: new_source.source_kind,
        repo_url: new_source.repo_url,
        docs_path: new_source.docs_path,
        default_ref: new_source.default_ref,
        version_strategy: new_source.version_strategy,
        tags: dedupe_tags(new_source.tags),
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    config.updated_at = now;
    config
        .sources
        .insert(new_source.name.clone(), source.clone());
    config.save(paths)?;

    Ok(source)
}

pub fn list_sources(config: &AppConfig) -> Vec<&SourceDefinition> {
    config.sources.values().collect()
}

pub fn get_source<'a>(config: &'a AppConfig, name: &str) -> Option<&'a SourceDefinition> {
    config.sources.get(name)
}
