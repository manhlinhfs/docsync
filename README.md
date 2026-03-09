# docsync

[Tiếng Việt](README_vi.md) | [Русский](README_ru.md) | [English](README.md)

`docsync` is a Rust CLI for building local, versioned snapshots of technical documentation that can later be imported into OmniMem or any other retrieval system.

It is designed around **universal intake**, not just a few handpicked docs homepages. The entry URL for a source may be:

- a docs homepage
- a single docs page
- `llms.txt`
- `llms-full.txt`
- `sitemap.xml`
- `robots.txt`
- a raw markdown or MDX file
- an OpenAPI or JSON schema document
- a PDF or office document URL
- a repo-backed docs site with a separate Git source of truth

The project is intentionally **binary-first**:

- users should be able to download one release artifact and run it
- local state should live in a predictable directory
- sync workflows should be scriptable in CI, cron, or terminal sessions
- website-only docs should still be ingestible without maintaining ad hoc scrape scripts per project

Proxy support is available for both HTTP sync and `git-docs` sync through:

- global CLI override: `--proxy http://127.0.0.1:7890`
- per-source setting: `docsync source add ... --proxy ...`
- runtime config: `default_proxy_url` in `config.json`
- environment fallback: `DOCSYNC_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`

Supported proxy URL forms include:

- `http://user:pass@host:port`
- `socks5://host:port`
- `socks5h://user:pass@host:port`

## Status

Current version: `0.6.0`

This release focuses on:

- local config and source registry
- probing arbitrary importable links for agent-friendly capabilities
- discovery-backed snapshot generation from `llms.txt`, `llms-full.txt`, sitemap indexes, and seed pages
- markdown-first page fetch with raw response archival and per-page metadata sidecars
- git-native docs sync from repository refs with docs root detection and nav manifest discovery
- HTML fallback conversion for website pages that do not expose markdown directly
- OmniMem import and verification commands with per-snapshot logs
- project docs, release planning, and architecture baseline

Incremental sync, headless docs support, and release engineering are staged across later roadmap versions.

## Why Rust

Rust is a good fit for `docsync` because the project needs:

- a single static-ish binary with low runtime friction
- strong typing for manifests, config, and release compatibility
- good concurrency later for crawling and fetch pipelines
- easy distribution on Linux, macOS, and Windows

## Intended workflow

```bash
docsync init
docsync source add postiz --url https://docs.postiz.com --kind website --tag docs --tag mintlify
docsync probe https://docs.postiz.com/introduction
docsync sync postiz --ref snapshot-20260307
```

## Current commands

### `docsync init`

Create the local runtime layout:

- config file
- sources directory
- snapshots directory

Default home:

```text
~/.docsync
```

Override it with:

```bash
docsync --home /path/to/runtime init
DOCSYNC_HOME=/path/to/runtime docsync init
```

### `docsync paths`

Print resolved runtime paths.

### `docsync source add`

Register a source definition from an entry URL:

```bash
docsync source add shadcn-ui \
  --url https://ui.shadcn.com \
  --proxy http://127.0.0.1:7890 \
  --kind git-docs \
  --repo https://github.com/shadcn-ui/ui \
  --docs-path apps/v4/content/docs \
  --default-ref main \
  --version-strategy git-ref \
  --tag components \
  --tag docs
```

### `docsync source list`

List configured sources.

### `docsync source show <name>`

Display one configured source.

### `docsync probe <url>`

Probe a docs URL for capabilities that matter to AI-oriented sync:

- `Accept: text/markdown` support
- `x-markdown-tokens`
- `x-original-tokens`
- `content-signal`
- `llms.txt`
- `robots.txt`
- sitemap presence

This is the current most useful command because it tells you which adapter strategy a site needs before implementing a crawler.
It also respects `--proxy` or the configured default proxy when a site blocks direct datacenter IPs.

It also classifies the URL into a generic intake mode such as:

- discovery root
- page seed
- single file asset
- markdown endpoint
- `llms` index
- sitemap or robots discovery seed

### `docsync import <source>`

Import a stored snapshot into OmniMem and write `omnimem-import.json` under that snapshot.

### `docsync verify <source> <query>`

Run an OmniMem search helper against a stored snapshot and write `omnimem-verify.json`.

### `docsync sync <source>`

`v0.6.0` now performs discovery plus either markdown-first HTTP fetch, HTML fallback, or git-native repo sync:

- snapshot directory
- `discovery.json`
- discovered URL frontier from `llms.txt`, `llms-full.txt`, sitemap indexes, direct seed pages, or repo-native docs trees
- markdown pages under `pages/`
- per-page metadata sidecars under `pages/`
- raw response bodies under `raw/`
- `pages/`
- `raw/`
- `manifest.json`
- optional `omnimem-import.json`
- optional `omnimem-verify.json`

For `git-docs` sources, `docsync` clones the repo, resolves the requested ref, detects or uses `docs_path`, snapshots markdown files directly, and records nav manifests such as `meta.json`, `docs.json`, or `mint.json`.
For HTML-only page seeds, `docsync` converts the fetched HTML into Markdown and stores the raw HTML body alongside the normalized page.

## Runtime layout

```text
~/.docsync/
  config.json
  sources/
  snapshots/
    postiz/
      snapshot-20260307/
        discovery.json
        manifest.json
        pages/
          docs.postiz.com/introduction.md
          docs.postiz.com/introduction.md.json
        raw/
          docs.postiz.com/introduction.body
```

## Development

Build:

```bash
cargo build
```

Run tests:

```bash
cargo test
```

Format:

```bash
cargo fmt
```

## Documentation

- [ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [INTAKE_MODEL.md](docs/INTAKE_MODEL.md)
- [LANGUAGE_POLICY.md](docs/LANGUAGE_POLICY.md)
- [USAGE.md](docs/USAGE.md)
- [RELEASE_POLICY.md](docs/RELEASE_POLICY.md)
- [ROADMAP.md](ROADMAP.md)
- [CHANGELOG.md](CHANGELOG.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)

## Project principles

1. Prefer source-of-truth docs artifacts over brittle scraping.
2. Probe for agent-friendly endpoints before converting HTML yourself.
3. Keep every snapshot versioned, attributable, and reproducible.
4. Design around stable local files that can be imported into OmniMem page-by-page.
