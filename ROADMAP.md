# Roadmap

This roadmap is version-first, not date-first. `docsync` is currently stable at `v1.1.0`.

## Release themes

### `v0.1.0` Foundation

Status: implemented

Scope:

- Rust binary project
- local runtime layout
- source registry
- probe command for markdown negotiation, `llms.txt`, `robots.txt`, sitemap hints
- snapshot scaffold manifest
- core documentation and release planning

Exit criteria:

- project builds cleanly
- probe works on real docs URLs
- sync creates snapshot directories and manifests

### `v0.2.0` Discovery

Status: implemented

Scope:

- detect intake kind from arbitrary entry URLs
- fetch and parse `llms.txt`
- fetch and parse `llms-full.txt`
- fetch sitemap indexes and nested sitemaps
- dedupe canonical page URLs
- persist discovery manifest per source/ref
- support non-homepage discovery seeds such as `sitemap.xml` and `llms.txt`

Exit criteria:

- one command can build a complete URL frontier from any valid discovery entrypoint when `llms.txt` or sitemap exists
- discovery output is stored in snapshot metadata

### `v0.3.0` Markdown-First Fetch

Status: implemented

Scope:

- markdown negotiation adapter via `Accept: text/markdown`
- metadata capture from headers
- markdown page storage under `pages/`
- raw response archival under `raw/`
- per-page metadata JSON sidecars
- importable markdown endpoints should work even when the user provides a single page URL instead of a docs root

Exit criteria:

- supported sites can be synced without HTML conversion
- manifest records page hashes and fetch method

### `v0.4.0` Git-Native Adapter

Status: implemented

Scope:

- source type for repo-native docs
- clone/fetch git repositories
- docs root detection
- ref-aware snapshoting by branch/tag/SHA
- nav manifest detection (`meta.json`, `docs.json`, `mint.json`, similar)

Exit criteria:

- shadcn-ui, Supabase, and OpenClaw-style repos can be snapshotted from Git directly

### `v0.5.0` HTML Normalization

Status: implemented

Scope:

- HTML fallback adapter
- readable markdown conversion
- cleanup of nav/footer/chrome noise
- normalization of code blocks, tables, callouts, tabs, and breadcrumbs
- direct single-page HTML intake

Exit criteria:

- website-only docs can be converted into importable markdown with acceptable quality

### `v0.6.0` OmniMem Integration

Status: implemented

Scope:

- batch import command
- optional direct call into local OmniMem CLI
- import logs
- per-snapshot import summary
- verification search helper

Exit criteria:

- one command can import a snapshot into OmniMem and record what happened

### `v0.7.0` Incremental Sync

Status: implemented

Scope:

- content hashing
- page diffing
- changed/new/removed page tracking
- re-sync without re-fetching everything
- changed-only OmniMem import by default on incremental snapshots

Exit criteria:

- repeat syncs are incremental and manifest-aware

### `v0.8.0` Headless and Dynamic Docs

Status: implemented

Scope:

- browser-rendered adapter
- JS-heavy docs support
- fallback for client-side nav or generated content
- capture of final DOM before normalization
- source/global browser command configuration and proxy-aware browser launch

Exit criteria:

- difficult docs sites can still be snapshotted with bounded complexity

### `v0.9.0` Release Engineering

Status: implemented

Scope:

- GitHub Actions release pipeline
- multi-platform binaries
- checksums
- install scripts
- shell completions

Exit criteria:

- users can download release binaries for Linux/macOS/Windows

### `v1.0.0` Stable CLI

Status: implemented

Scope:

- schema stability
- migration path for runtime data
- production-grade docs
- compatibility guarantees for config and manifest formats

Exit criteria:

- major commands are stable
- manifests are backward-compatible or migratable
- release notes communicate upgrade guarantees

## Milestones after `v1.1.0`

Development after `v1.1.0` should move in milestone order unless a regression, release blocker, or security issue needs to interrupt the sequence.

### `v1.2.0` Multi-Engine Normalization

Status: planned

Scope:

- add targeted cleanup profiles for Mintlify, Docusaurus, Nextra, MkDocs, VitePress, and GitBook
- improve boilerplate stripping for repeated nav chrome, page index banners, and duplicated headings
- normalize common callout, tab, step, card, and accordion patterns without dropping important content
- improve canonical page selection when multiple URLs normalize to the same page body
- add real-site smoke coverage for at least one site per supported docs engine

Exit criteria:

- common docs engines produce cleaner Markdown with materially fewer UI artifacts
- canonical page selection is stable across repeated syncs
- the smoke matrix passes on supported real-world docs sites

### `v1.3.0` Quality Scoring and Import Policy

Status: planned

Scope:

- score each page for residual HTML/MDX noise, text density, title quality, and boilerplate ratio
- record page quality signals in snapshot metadata and sidecars
- add import policy rules for low-signal pages, empty pages, and duplicate groups
- keep the best canonical page when duplicate content appears at multiple URLs
- expose quality summary counters in `manifest.json`

Exit criteria:

- each snapshot reports page-quality metrics in a way that is easy to audit
- low-signal pages can be filtered before OmniMem import
- duplicate groups keep the best canonical page consistently

### `v1.4.0` Section-Aware Chunking

Status: planned

Scope:

- split normalized pages by heading structure instead of page-only import
- keep code blocks close to the nearest explanatory section
- preserve section path metadata for each chunk
- expose configurable chunk size and overlap limits without breaking stable defaults
- support changed-only chunk import for incremental snapshots

Exit criteria:

- imported chunks map cleanly to page sections
- retrieval quality improves over page-level import on representative test queries
- chunk metadata remains stable enough for incremental sync and re-import

### `v1.5.0` Structured Asset Adapters

Status: planned

Scope:

- add adapters for `openapi.json`, JSON schema, and similar structured docs assets
- add a practical PDF ingestion path for text-heavy reference PDFs
- attach asset-derived metadata to generated Markdown output
- support source-level rules for including or excluding structured assets
- keep unsupported binary assets out of the main import path by default

Exit criteria:

- structured docs assets can be converted into import-ready Markdown with traceable provenance
- unsupported asset types fail clearly without breaking the rest of the sync
- asset handling is configurable per source

### `v1.6.0` Continuous Validation

Status: planned

Scope:

- build a maintained smoke suite against real docs sites such as OpenClaw, shadcn/ui, Supabase, and representative engine-specific sites
- add snapshot quality regression checks to CI
- add a `doctor`-style runtime check for Git, browser, OmniMem, proxy, and headless prerequisites
- document the support matrix by docs engine and ingestion path
- tighten release gates so a stable release requires clean CI plus passing smoke coverage

Exit criteria:

- stable releases are gated by CI and real-site smoke checks
- users can diagnose local runtime problems quickly
- the supported and partially supported docs-engine matrix is explicit

## Longer-Term Backlog

- distributed worker mode
- pluggable extraction providers
- asset downloads and attachment rewriting
- remote registry of community source adapters
