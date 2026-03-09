# Changelog

## `0.6.0` - 2026-03-09

Added:

- HTML fallback conversion from fetched page responses into Markdown snapshots
- `docsync import` to batch-import stored pages into OmniMem with `omnimem-import.json` logs
- `docsync verify` to run OmniMem search helpers against snapshots with `omnimem-verify.json` logs
- proxy support for HTTP probe/sync and git-native sync via CLI override, source config, runtime config, or environment variables
- SOCKS5 and SOCKS5h proxy URLs for the HTTP transport layer
- fake-OmniMem tests for import and verify flows

Changed:

- website page seeds now store HTML-only pages through `html_fallback` instead of skipping them
- project docs and roadmap now describe `v0.5.0` and `v0.6.0` behavior

Known limitations:

- multi-page website discovery still depends on `llms.txt` and sitemap availability
- incremental sync and headless browser support are not implemented yet
- OmniMem import is page-by-page and does not yet deduplicate repeated imports

## `0.4.0` - 2026-03-09

Added:

- git-native `sync` path for `git-docs` sources
- repository clone and requested-ref checkout during sync
- docs root detection when `--docs-path` is omitted
- nav manifest detection for `meta.json`, `docs.json`, and `mint.json`
- manifest git summary with repo URL, requested/resolved ref, docs path, and detected nav manifests
- local git repository tests for docs root detection, nav manifests, specific-commit sync, and full git snapshotting

Changed:

- `sync` now branches between HTTP discovery/fetch and git-native repo sync based on source kind
- project docs and roadmap now describe `v0.4.0` git-native behavior

Known limitations:

- HTML-only website pages are still skipped until the HTML normalization adapter lands
- git-native sync copies markdown files but does not normalize MDX/includes yet
- no OmniMem import command yet

## `0.3.0` - 2026-03-09

Added:

- markdown-first fetch during `sync` using `Accept: text/markdown`
- markdown page storage under `pages/`
- raw response archival under `raw/`
- per-page JSON sidecars with response metadata and hashes
- manifest entries for fetch results, page hashes, fetch method, and stored/skipped counts
- unit tests for page path generation and markdown response classification

Changed:

- `sync` now fetches compatible discovered pages instead of stopping at discovery metadata
- snapshot status now distinguishes discovered frontiers from fetched markdown pages
- project docs and roadmap now describe `v0.3.0` fetch behavior

Known limitations:

- HTML-only pages are still skipped until the HTML normalization adapter lands
- git-native repository sync is still deferred to `v0.4.0`
- no OmniMem import command yet

## `0.2.0` - 2026-03-09

Added:

- discovery-backed `sync` that builds a deduped URL frontier from `llms.txt`, `llms-full.txt`, sitemap indexes, nested sitemaps, or a direct page seed fallback
- `discovery.json` snapshots with adapter provenance and discovered page URLs
- richer `manifest.json` metadata for detected input kind, suggested mode, discovery adapter summary, and frontier counts
- unit tests for `llms.txt` parsing, relative links, and sitemap parsing

Changed:

- `sync --dry-run` now performs live discovery planning and reports discovered page counts
- project docs and roadmap now describe `v0.2.0` discovery behavior

Known limitations:

- `sync` still does not fetch or normalize page bodies yet
- git-native repository discovery is still deferred to `v0.4.0`
- no OmniMem import command yet

## `0.1.0` - 2026-03-07

Initial scaffold release.

Added:

- Rust binary project for `docsync`
- CLI commands:
  - `init`
  - `paths`
  - `probe`
  - `source add`
  - `source list`
  - `source show`
  - `sync`
- local runtime layout and config persistence
- docs capability probe for markdown negotiation, `llms.txt`, `robots.txt`, sitemap hints, and generic input classification
- snapshot scaffold manifest generation
- project documentation, release policy, contributing guide, and roadmap
- universal intake design so source entrypoints are not limited to homepage URLs

Known limitations:

- `sync` does not fetch real content yet
- no git-native adapter yet
- no OmniMem import command yet
