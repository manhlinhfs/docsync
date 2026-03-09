mod cli;
mod config;
mod dashboard;
mod discovery;
mod fetch;
mod git_sync;
mod headless;
mod incremental;
mod migrate;
mod models;
mod network;
mod normalize;
mod omnimem;
mod probe;
mod quality;
mod sources;
mod sync;
mod telegram;
mod util;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use clap_complete::{Shell, generate};
use serde::Serialize;

use crate::cli::{Cli, Commands, CompletionShell, NotifyCommands, SourceCommands};
use crate::config::{ensure_layout, load_config, resolve_paths};
use crate::dashboard::{
    build_dashboard, build_global_dashboard, dashboard_status, global_dashboard_status,
    run_dashboard_host, serve_dashboard, serve_global_dashboard, stop_dashboard,
    stop_global_dashboard,
};
use crate::migrate::migrate_runtime;
use crate::models::NewSource;
use crate::omnimem::{import_snapshot, verify_snapshot};
use crate::sources::{add_source, configure_source_auto_import, get_source, list_sources};
use crate::sync::sync_source;
use crate::telegram::send_telegram_snapshot_summary;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let home_override = cli.home.as_ref().map(PathBuf::as_path);
    let paths = resolve_paths(home_override)?;

    match cli.command {
        Commands::Init(args) => {
            ensure_layout(&paths)?;
            let mut config = load_config(&paths)?;

            if config.created_at.is_empty() {
                config.created_at = util::now_utc_rfc3339();
                config.updated_at = config.created_at.clone();
                config.save(&paths)?;
            }

            if args.json {
                print_json(&paths)?;
            } else {
                println!("Initialized docsync home at {}", paths.home.display());
                println!("Config: {}", paths.config_file.display());
                println!("Sources: {}", paths.sources_dir.display());
                println!("Snapshots: {}", paths.snapshots_dir.display());
            }
        }
        Commands::Paths(args) => {
            ensure_layout(&paths)?;
            if args.json {
                print_json(&paths)?;
            } else {
                println!("docsync home: {}", paths.home.display());
                println!("config file: {}", paths.config_file.display());
                println!("sources dir: {}", paths.sources_dir.display());
                println!("snapshots dir: {}", paths.snapshots_dir.display());
            }
        }
        Commands::Completions(args) => {
            let shell = match args.shell {
                CompletionShell::Bash => Shell::Bash,
                CompletionShell::Elvish => Shell::Elvish,
                CompletionShell::Fish => Shell::Fish,
                CompletionShell::Powershell => Shell::PowerShell,
                CompletionShell::Zsh => Shell::Zsh,
            };
            print_completions(shell);
        }
        Commands::Migrate(args) => {
            ensure_layout(&paths)?;
            let result = migrate_runtime(&paths)?;
            if args.json {
                print_json(&result)?;
            } else {
                println!("Config updated: {}", result.config_updated);
                println!("Manifests scanned: {}", result.manifests_scanned);
                println!("Manifests rewritten: {}", result.manifests_rewritten);
            }
        }
        Commands::Dashboard(args) => {
            ensure_layout(&paths)?;
            let mode_count =
                usize::from(args.serve) + usize::from(args.stop) + usize::from(args.status);
            if mode_count > 1 {
                anyhow::bail!("choose at most one of --serve, --stop, or --status");
            }
            if args.all && args.reference.is_some() {
                anyhow::bail!("--ref is only valid for source-specific dashboards");
            }
            if !args.all && args.name.is_none() {
                anyhow::bail!("provide a source name or use --all");
            }
            if args.stop {
                let result = if args.all {
                    stop_global_dashboard(&paths)?
                } else {
                    stop_dashboard(
                        &paths,
                        args.name.as_deref().expect("source name"),
                        args.reference,
                    )?
                };
                if args.json {
                    print_json(&result)?;
                } else {
                    println!("Source: {}", result.source_name);
                    println!("Snapshot: {}", result.snapshot_label);
                    println!("Running: {}", result.running);
                    println!("State file: {}", result.state_path.display());
                }
            } else if args.status {
                let result = if args.all {
                    global_dashboard_status(&paths)?
                } else {
                    dashboard_status(
                        &paths,
                        args.name.as_deref().expect("source name"),
                        args.reference,
                    )?
                };
                if args.json {
                    print_json(&result)?;
                } else {
                    println!("Source: {}", result.source_name);
                    println!("Snapshot: {}", result.snapshot_label);
                    println!("Running: {}", result.running);
                    println!("URL: {}", result.url);
                    println!("Host: {}", result.host);
                    println!("Port: {}", result.port);
                    if let Some(pid) = result.pid {
                        println!("PID: {pid}");
                    }
                    println!("State file: {}", result.state_path.display());
                }
            } else if args.serve {
                let result = if args.all {
                    serve_global_dashboard(&paths, args.output, &args.host, args.port)?
                } else {
                    serve_dashboard(
                        &paths,
                        args.name.as_deref().expect("source name"),
                        args.reference,
                        args.output,
                        &args.host,
                        args.port,
                    )?
                };
                if args.json {
                    print_json(&result)?;
                } else {
                    println!("Source: {}", result.source_name);
                    println!("Snapshot: {}", result.snapshot_label);
                    println!("URL: {}", result.url);
                    println!("Host: {}", result.host);
                    println!("Port: {}", result.port);
                    println!("PID: {}", result.pid);
                    println!("Already running: {}", result.already_running);
                    println!("Output: {}", result.output_path.display());
                    println!("State file: {}", result.state_path.display());
                }
            } else {
                let result = if args.all {
                    build_global_dashboard(&paths, args.output)?
                } else {
                    build_dashboard(
                        &paths,
                        args.name.as_deref().expect("source name"),
                        args.reference,
                        args.output,
                    )?
                };
                if args.json {
                    print_json(&result)?;
                } else {
                    println!("Source: {}", result.source_name);
                    println!("Snapshot: {}", result.snapshot_label);
                    println!("Pages shown: {}", result.pages_shown);
                    println!("High quality pages: {}", result.high_quality_pages);
                    println!("Medium quality pages: {}", result.medium_quality_pages);
                    println!("Low quality pages: {}", result.low_quality_pages);
                    println!("Output: {}", result.output_path.display());
                }
            }
        }
        Commands::DashboardHost(args) => {
            run_dashboard_host(&args.root, &args.host, args.port)?;
        }
        Commands::Notify { command } => {
            ensure_layout(&paths)?;
            match command {
                NotifyCommands::Telegram(args) => {
                    let result = send_telegram_snapshot_summary(
                        &paths,
                        &args.name,
                        args.reference,
                        args.bot_token,
                        args.chat_id,
                        cli.proxy.as_deref(),
                    )?;
                    if args.json {
                        print_json(&result)?;
                    } else {
                        println!("Source: {}", result.source_name);
                        println!("Snapshot: {}", result.snapshot_label);
                        println!("Chat ID: {}", result.chat_id);
                        println!("Message length: {}", result.message_length);
                        println!("API endpoint: {}", result.api_endpoint);
                    }
                }
            }
        }
        Commands::Probe(args) => {
            ensure_layout(&paths)?;
            let config = load_config(&paths)?;
            let proxy_url = network::resolve_proxy_url(cli.proxy.as_deref(), Some(&config), None);
            let report = probe::probe_url_with_proxy(&args.url, proxy_url.as_deref())?;
            if args.json {
                print_json(&report)?;
            } else {
                println!("Requested URL: {}", report.requested_url);
                println!("Final URL: {}", report.final_url);
                println!("Detected input kind: {:?}", report.detected_input_kind);
                println!("Suggested mode: {:?}", report.suggested_mode);
                println!("Markdown supported: {}", report.markdown_supported);
                println!(
                    "Content-Type: {}",
                    report.markdown.content_type.as_deref().unwrap_or("unknown")
                );
                if let Some(value) = report.markdown.x_markdown_tokens {
                    println!("x-markdown-tokens: {value}");
                }
                if let Some(value) = report.markdown.x_original_tokens {
                    println!("x-original-tokens: {value}");
                }
                if let Some(value) = report.markdown.content_signal.as_deref() {
                    println!("content-signal: {value}");
                }
                if let Some(url) = report.llms.index_url.as_deref() {
                    println!("llms.txt: {url}");
                }
                if let Some(url) = report.llms.full_index_url.as_deref() {
                    println!("llms-full.txt: {url}");
                }
                if !report.robots.sitemaps.is_empty() {
                    println!("Sitemaps:");
                    for sitemap in &report.robots.sitemaps {
                        println!("  - {sitemap}");
                    }
                }
                if let Some(count) = report.robots.first_sitemap_url_count {
                    println!("First sitemap URL count: {count}");
                }
                if !report.recommendations.is_empty() {
                    println!("Recommendations:");
                    for recommendation in &report.recommendations {
                        println!("  - {recommendation}");
                    }
                }
            }
        }
        Commands::Import(args) => {
            ensure_layout(&paths)?;
            let result = import_snapshot(
                &paths,
                &args.name,
                args.reference,
                args.omnimem_cmd,
                args.direct,
                args.dry_run,
                args.all_pages,
                args.include_low_signal,
            )?;
            if args.json {
                print_json(&result)?;
            } else {
                println!("Source: {}", result.source_name);
                println!("Snapshot: {}", result.snapshot_label);
                println!("Dry run: {}", result.dry_run);
                println!("Selected pages: {}", result.selected_pages);
                println!("Imported pages: {}", result.imported_pages);
                println!("Failed pages: {}", result.failed_pages);
                println!(
                    "Skipped unchanged pages: {}",
                    result.skipped_unchanged_pages
                );
                println!(
                    "Skipped low-signal pages: {}",
                    result.skipped_low_signal_pages
                );
                println!("OmniMem command: {}", result.omnimem_cmd);
                println!("Summary: {}", result.summary_path.display());
            }
        }
        Commands::Source { command } => {
            ensure_layout(&paths)?;
            match command {
                SourceCommands::Add(args) => {
                    let mut config = load_config(&paths)?;
                    let new_source = NewSource {
                        name: args.name,
                        entry_url: args.url,
                        proxy_url: args.proxy,
                        browser_cmd: args.browser_cmd,
                        auto_import: args.auto_import,
                        omnimem_cmd: args.omnimem_cmd,
                        omnimem_direct: args.omnimem_direct,
                        omnimem_include_low_signal: args.omnimem_include_low_signal,
                        source_kind: args.kind,
                        repo_url: args.repo,
                        docs_path: args.docs_path,
                        default_ref: args.default_ref,
                        version_strategy: args.version_strategy,
                        tags: args.tag,
                    };
                    let source = add_source(&mut config, &paths, new_source)?;
                    if args.json {
                        print_json(&source)?;
                    } else {
                        println!("Saved source `{}`", source.name);
                        println!("Entry URL: {}", source.entry_url);
                        println!("Kind: {}", source.source_kind);
                        println!("Version strategy: {}", source.version_strategy);
                        if let Some(proxy_url) = source.proxy_url.as_deref() {
                            println!("Proxy: {proxy_url}");
                        }
                        if let Some(browser_cmd) = source.browser_cmd.as_deref() {
                            println!("Browser command: {browser_cmd}");
                        }
                        println!("Auto import: {}", source.auto_import);
                        if let Some(omnimem_cmd) = source.omnimem_cmd.as_deref() {
                            println!("OmniMem command: {omnimem_cmd}");
                        }
                        if source.omnimem_direct {
                            println!("OmniMem direct mode: true");
                        }
                        if source.omnimem_include_low_signal {
                            println!("OmniMem include low signal: true");
                        }
                        if let Some(repo_url) = source.repo_url.as_deref() {
                            println!("Repo: {repo_url}");
                        }
                        if let Some(docs_path) = source.docs_path.as_deref() {
                            println!("Docs path: {docs_path}");
                        }
                    }
                }
                SourceCommands::AutoImport(args) => {
                    let mut config = load_config(&paths)?;
                    let enabled = if args.enable {
                        true
                    } else if args.disable {
                        false
                    } else {
                        anyhow::bail!("choose one of --enable or --disable");
                    };
                    let source = configure_source_auto_import(
                        &mut config,
                        &paths,
                        &args.name,
                        enabled,
                        args.omnimem_cmd,
                        args.omnimem_direct,
                        args.omnimem_include_low_signal,
                    )?;
                    if args.json {
                        print_json(&source)?;
                    } else {
                        println!("Name: {}", source.name);
                        println!("Auto import: {}", source.auto_import);
                        if let Some(omnimem_cmd) = source.omnimem_cmd.as_deref() {
                            println!("OmniMem command: {omnimem_cmd}");
                        }
                        println!("OmniMem direct mode: {}", source.omnimem_direct);
                        println!(
                            "OmniMem include low signal: {}",
                            source.omnimem_include_low_signal
                        );
                    }
                }
                SourceCommands::List(args) => {
                    let config = load_config(&paths)?;
                    let sources = list_sources(&config);
                    if args.json {
                        print_json(&sources)?;
                    } else if sources.is_empty() {
                        println!("No sources configured.");
                    } else {
                        for source in sources {
                            println!(
                                "- {} [{}] {}",
                                source.name, source.source_kind, source.entry_url
                            );
                        }
                    }
                }
                SourceCommands::Show(args) => {
                    let config = load_config(&paths)?;
                    let source = get_source(&config, &args.name)
                        .with_context(|| format!("source `{}` not found", args.name))?;
                    if args.json {
                        print_json(source)?;
                    } else {
                        println!("Name: {}", source.name);
                        println!("Entry URL: {}", source.entry_url);
                        println!("Kind: {}", source.source_kind);
                        println!("Version strategy: {}", source.version_strategy);
                        if let Some(proxy_url) = source.proxy_url.as_deref() {
                            println!("Proxy: {proxy_url}");
                        }
                        if let Some(browser_cmd) = source.browser_cmd.as_deref() {
                            println!("Browser command: {browser_cmd}");
                        }
                        println!("Auto import: {}", source.auto_import);
                        if let Some(omnimem_cmd) = source.omnimem_cmd.as_deref() {
                            println!("OmniMem command: {omnimem_cmd}");
                        }
                        println!("OmniMem direct mode: {}", source.omnimem_direct);
                        println!(
                            "OmniMem include low signal: {}",
                            source.omnimem_include_low_signal
                        );
                        if let Some(repo) = source.repo_url.as_deref() {
                            println!("Repo URL: {repo}");
                        }
                        if let Some(path) = source.docs_path.as_deref() {
                            println!("Docs path: {path}");
                        }
                        if let Some(reference) = source.default_ref.as_deref() {
                            println!("Default ref: {reference}");
                        }
                        if !source.tags.is_empty() {
                            println!("Tags: {}", source.tags.join(", "));
                        }
                        println!("Created: {}", source.created_at);
                    }
                }
            }
        }
        Commands::Sync(args) => {
            ensure_layout(&paths)?;
            if args.dry_run && args.import {
                anyhow::bail!("--import cannot be used with --dry-run");
            }
            let config = load_config(&paths)?;
            let result = sync_source(
                &config,
                &paths,
                &args.name,
                args.reference,
                args.dry_run,
                cli.proxy.as_deref(),
                cli.browser_cmd.as_deref(),
                args.import,
                args.omnimem_cmd,
                args.omnimem_direct,
                args.omnimem_include_low_signal,
            )?;
            if args.json {
                print_json(&result)?;
            } else {
                println!("Source: {}", result.source_name);
                println!("Entry URL: {}", result.entry_url);
                println!("Ref label: {}", result.snapshot_label);
                println!("Dry run: {}", result.dry_run);
                println!("Strategy: {}", result.strategy_summary);
                println!("Discovered pages: {}", result.discovered_pages);
                println!("Fetched pages: {}", result.fetched_pages);
                println!("Skipped pages: {}", result.skipped_pages);
                println!("Reused pages: {}", result.reused_pages);
                println!("Changed/new pages: {}", result.changed_pages);
                println!("Unchanged pages: {}", result.unchanged_pages);
                println!("Removed pages: {}", result.removed_pages);
                if let Some(previous_snapshot_label) = result.previous_snapshot_label.as_deref() {
                    println!("Previous snapshot: {previous_snapshot_label}");
                }
                println!("Snapshot dir: {}", result.snapshot_dir.display());
                println!(
                    "Discovery manifest: {}",
                    result.discovery_manifest_path.display()
                );
                println!("Manifest: {}", result.manifest_path.display());
                if !args.dry_run {
                    println!("Snapshot discovery metadata written.");
                }
                if let Some(import_result) = result.import.as_ref() {
                    println!("Auto import: true");
                    println!("Imported pages: {}", import_result.imported_pages);
                    println!("Failed imports: {}", import_result.failed_pages);
                    println!("OmniMem summary: {}", import_result.summary_path.display());
                } else {
                    println!("Auto import: false");
                }
            }
        }
        Commands::Verify(args) => {
            ensure_layout(&paths)?;
            let result = verify_snapshot(
                &paths,
                &args.name,
                args.reference,
                &args.query,
                args.omnimem_cmd,
                args.direct,
            )?;
            if args.json {
                print_json(&result)?;
            } else {
                println!("Source: {}", result.source_name);
                println!("Snapshot: {}", result.snapshot_label);
                println!("Query: {}", result.query);
                println!("Success: {}", result.success);
                println!("OmniMem command: {}", result.omnimem_cmd);
                println!("Summary: {}", result.summary_path.display());
            }
        }
    }

    Ok(())
}

fn print_json<T>(value: &T) -> Result<()>
where
    T: Serialize,
{
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_completions(shell: Shell) {
    let mut command = Cli::command();
    generate(shell, &mut command, "docsync", &mut std::io::stdout());
}
