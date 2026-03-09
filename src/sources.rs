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
        browser_cmd: new_source.browser_cmd,
        auto_import: new_source.auto_import,
        omnimem_cmd: new_source.omnimem_cmd,
        omnimem_direct: new_source.omnimem_direct,
        omnimem_include_low_signal: new_source.omnimem_include_low_signal,
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

pub fn configure_source_auto_import(
    config: &mut AppConfig,
    paths: &AppPaths,
    name: &str,
    enabled: bool,
    omnimem_cmd: Option<String>,
    omnimem_direct: bool,
    omnimem_include_low_signal: bool,
) -> Result<SourceDefinition> {
    let updated = {
        let source = config
            .sources
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("source `{name}` not found"))?;
        let now = now_utc_rfc3339();

        source.auto_import = enabled;
        source.omnimem_cmd = omnimem_cmd;
        source.omnimem_direct = omnimem_direct;
        source.omnimem_include_low_signal = omnimem_include_low_signal;
        source.updated_at = now.clone();
        config.updated_at = now;
        source.clone()
    };
    config.save(paths)?;

    Ok(updated)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Result;

    use super::{add_source, configure_source_auto_import};
    use crate::config::AppPaths;
    use crate::models::{AppConfig, NewSource, SourceKind, VersionStrategy};

    #[test]
    fn adds_source_with_auto_import_policy() -> Result<()> {
        let root = temp_dir("source-auto-import-add");
        let paths = make_paths(&root);
        fs::create_dir_all(&paths.home)?;
        let mut config = AppConfig::default();

        let source = add_source(
            &mut config,
            &paths,
            NewSource {
                name: "demo".to_string(),
                entry_url: "https://docs.example.com".to_string(),
                proxy_url: None,
                browser_cmd: None,
                auto_import: true,
                omnimem_cmd: Some("/root/omnimem/omnimem".to_string()),
                omnimem_direct: true,
                omnimem_include_low_signal: true,
                source_kind: SourceKind::Website,
                repo_url: None,
                docs_path: None,
                default_ref: None,
                version_strategy: VersionStrategy::DateSnapshot,
                tags: Vec::new(),
            },
        )?;

        assert!(source.auto_import);
        assert_eq!(source.omnimem_cmd.as_deref(), Some("/root/omnimem/omnimem"));
        assert!(source.omnimem_direct);
        assert!(source.omnimem_include_low_signal);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn toggles_auto_import_for_existing_source() -> Result<()> {
        let root = temp_dir("source-auto-import-toggle");
        let paths = make_paths(&root);
        fs::create_dir_all(&paths.home)?;
        let mut config = AppConfig::default();

        add_source(
            &mut config,
            &paths,
            NewSource {
                name: "demo".to_string(),
                entry_url: "https://docs.example.com".to_string(),
                proxy_url: None,
                browser_cmd: None,
                auto_import: false,
                omnimem_cmd: None,
                omnimem_direct: false,
                omnimem_include_low_signal: false,
                source_kind: SourceKind::Website,
                repo_url: None,
                docs_path: None,
                default_ref: None,
                version_strategy: VersionStrategy::DateSnapshot,
                tags: Vec::new(),
            },
        )?;

        let source = configure_source_auto_import(
            &mut config,
            &paths,
            "demo",
            true,
            Some("/tmp/fake-omnimem".to_string()),
            true,
            true,
        )?;

        assert!(source.auto_import);
        assert_eq!(source.omnimem_cmd.as_deref(), Some("/tmp/fake-omnimem"));
        assert!(source.omnimem_direct);
        assert!(source.omnimem_include_low_signal);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("docsync-{prefix}-{stamp}"))
    }

    fn make_paths(root: &PathBuf) -> AppPaths {
        AppPaths {
            home: root.clone(),
            config_file: root.join("config.json"),
            sources_dir: root.join("sources"),
            snapshots_dir: root.join("snapshots"),
        }
    }
}
