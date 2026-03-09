use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

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

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init(InitArgs),
    Paths(PathsArgs),
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
    pub json: bool,
}

#[derive(Debug, Subcommand)]
pub enum SourceCommands {
    Add(SourceAddArgs),
    List(SourceListArgs),
    Show(SourceShowArgs),
}

#[derive(Debug, Args)]
pub struct SourceAddArgs {
    pub name: String,

    #[arg(long)]
    pub url: String,

    #[arg(long)]
    pub proxy: Option<String>,

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
