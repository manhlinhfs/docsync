use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::models::{SourceKind, VersionStrategy};

#[derive(Debug, Parser)]
#[command(
    name = "docsync",
    version,
    about = "Binary-first docs snapshot CLI for AI retrieval pipelines"
)]
pub struct Cli {
    #[arg(long, global = true)]
    pub home: Option<PathBuf>,

    #[arg(long, global = true)]
    pub proxy: Option<String>,

    #[arg(long, global = true)]
    pub browser_cmd: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init(InitArgs),
    Paths(PathsArgs),
    Completions(CompletionsArgs),
    Migrate(MigrateArgs),
    Dashboard(DashboardArgs),
    #[command(hide = true)]
    DashboardHost(DashboardHostArgs),
    Notify {
        #[command(subcommand)]
        command: NotifyCommands,
    },
    Probe(ProbeArgs),
    Import(ImportArgs),
    Source {
        #[command(subcommand)]
        command: SourceCommands,
    },
    Sync(SyncArgs),
    Verify(VerifyArgs),
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct PathsArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ProbeArgs {
    pub url: String,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DashboardArgs {
    pub name: Option<String>,

    #[arg(long)]
    pub all: bool,

    #[arg(long = "ref")]
    pub reference: Option<String>,

    #[arg(long)]
    pub output: Option<PathBuf>,

    #[arg(long)]
    pub serve: bool,

    #[arg(long)]
    pub stop: bool,

    #[arg(long)]
    pub status: bool,

    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    #[arg(long, default_value_t = 4317)]
    pub port: u16,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DashboardHostArgs {
    #[arg(long)]
    pub root: PathBuf,

    #[arg(long)]
    pub host: String,

    #[arg(long)]
    pub port: u16,
}

#[derive(Debug, Args)]
pub struct ImportArgs {
    pub name: String,

    #[arg(long = "ref")]
    pub reference: Option<String>,

    #[arg(long)]
    pub omnimem_cmd: Option<String>,

    #[arg(long)]
    pub direct: bool,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub all_pages: bool,

    #[arg(long)]
    pub include_low_signal: bool,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    #[value(name = "powershell")]
    Powershell,
    Zsh,
}

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    #[arg(value_enum)]
    pub shell: CompletionShell,
}

#[derive(Debug, Args)]
pub struct MigrateArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Subcommand)]
pub enum SourceCommands {
    Add(SourceAddArgs),
    AutoImport(SourceAutoImportArgs),
    List(SourceListArgs),
    Show(SourceShowArgs),
}

#[derive(Debug, Subcommand)]
pub enum NotifyCommands {
    Telegram(TelegramNotifyArgs),
}

#[derive(Debug, Args)]
pub struct TelegramNotifyArgs {
    pub name: String,

    #[arg(long = "ref")]
    pub reference: Option<String>,

    #[arg(long)]
    pub bot_token: Option<String>,

    #[arg(long)]
    pub chat_id: Option<String>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SourceAddArgs {
    pub name: String,

    #[arg(long)]
    pub url: String,

    #[arg(long)]
    pub proxy: Option<String>,

    #[arg(long)]
    pub browser_cmd: Option<String>,

    #[arg(long)]
    pub auto_import: bool,

    #[arg(long)]
    pub omnimem_cmd: Option<String>,

    #[arg(long)]
    pub omnimem_direct: bool,

    #[arg(long)]
    pub omnimem_include_low_signal: bool,

    #[arg(long, value_enum, default_value_t = SourceKind::Auto)]
    pub kind: SourceKind,

    #[arg(long)]
    pub repo: Option<String>,

    #[arg(long)]
    pub docs_path: Option<String>,

    #[arg(long)]
    pub default_ref: Option<String>,

    #[arg(long, value_enum, default_value_t = VersionStrategy::Auto)]
    pub version_strategy: VersionStrategy,

    #[arg(long = "tag")]
    pub tag: Vec<String>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SourceAutoImportArgs {
    pub name: String,

    #[arg(long, conflicts_with = "disable")]
    pub enable: bool,

    #[arg(long, conflicts_with = "enable")]
    pub disable: bool,

    #[arg(long)]
    pub omnimem_cmd: Option<String>,

    #[arg(long)]
    pub omnimem_direct: bool,

    #[arg(long)]
    pub omnimem_include_low_signal: bool,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SourceListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SourceShowArgs {
    pub name: String,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    pub name: String,

    #[arg(long = "ref")]
    pub reference: Option<String>,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub import: bool,

    #[arg(long)]
    pub omnimem_cmd: Option<String>,

    #[arg(long)]
    pub omnimem_direct: bool,

    #[arg(long)]
    pub omnimem_include_low_signal: bool,

    #[arg(long)]
    pub chunk_target_words: Option<usize>,

    #[arg(long)]
    pub chunk_overlap_words: Option<usize>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct VerifyArgs {
    pub name: String,

    pub query: String,

    #[arg(long = "ref")]
    pub reference: Option<String>,

    #[arg(long)]
    pub omnimem_cmd: Option<String>,

    #[arg(long)]
    pub direct: bool,

    #[arg(long)]
    pub json: bool,
}
