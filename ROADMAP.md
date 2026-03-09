# Roadmap

This roadmap is version-first, not date-first. The sequence matters more than exact release dates.

## Release themes

### `v0.1.0` Foundation

Status: implemented in this scaffold

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

Status: implemented in the current release

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

Status: implemented in the current release

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

Status: implemented in the current release

Scope:

- source type for repo-native docs
- clone/fetch git repositories
- docs root detection
- ref-aware snapshoting by branch/tag/SHA
- nav manifest detection (`meta.json`, `docs.json`, `mint.json`, similar)

Exit criteria:

- shadcn-ui, Supabase, and OpenClaw-style repos can be snapshotted from Git directly

### `v0.5.0` HTML Normalization

Status: implemented in the current release

Scope:

- HTML fallback adapter
- readable markdown conversion
- cleanup of nav/footer/chrome noise
- normalization of code blocks, tables, callouts, tabs, and breadcrumbs
- direct single-page HTML intake

Exit criteria:

- website-only docs can be converted into importable markdown with acceptable quality

### `v0.6.0` OmniMem Integration

Status: implemented in the current release

Scope:

- batch import command
- optional direct call into local OmniMem CLI
- import logs
- per-snapshot import summary
- verification search helper

Exit criteria:

- one command can import a snapshot into OmniMem and record what happened

### `v0.7.0` Incremental Sync

Scope:

- content hashing
- page diffing
- changed/new/removed page tracking
- re-sync without re-fetching everything

Exit criteria:

- repeat syncs are incremental and manifest-aware

### `v0.8.0` Headless and Dynamic Docs

Scope:

- browser-rendered adapter
- JS-heavy docs support
- fallback for client-side nav or generated content
- capture of final DOM before normalization

Exit criteria:

- difficult docs sites can still be snapshotted with bounded complexity

### `v0.9.0` Release Engineering

Scope:

- GitHub Actions release pipeline
- multi-platform binaries
- checksums
- install scripts
- shell completions

Exit criteria:

- users can download release binaries for Linux/macOS/Windows

### `v1.0.0` Stable CLI

Scope:

- schema stability
- migration path for runtime data
- production-grade docs
- compatibility guarantees for config and manifest formats

Exit criteria:

- major commands are stable
- manifests are backward-compatible or migratable
- release notes communicate upgrade guarantees

## Backlog after `v1.0.0`

- distributed worker mode
- pluggable extraction providers
- OpenAPI/schema-aware enrichment
- asset downloads and attachment rewriting
- remote registry of community source adapters
