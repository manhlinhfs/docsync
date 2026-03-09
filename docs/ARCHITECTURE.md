# Architecture

## Objective

`docsync` turns docs sources into local, versioned snapshots that are suitable for AI retrieval systems such as OmniMem.

The project should not behave like a generic web crawler. It should behave like a docs-aware synchronizer.

## Design priorities

1. Binary-first distribution
2. Snapshot reproducibility
3. Source provenance
4. Version-aware storage
5. Adapter-based fetch strategy

## Core flow

### 1. Source registry

User registers a source with:

- entry URL
- source kind
- optional repo URL
- optional docs path
- version strategy
- tags

This data lives in `config.json`.

The entry URL is intentionally generic. It may be a docs homepage, a single page, `llms.txt`, `sitemap.xml`, a markdown file, or another directly importable asset.
Browser fallback can be configured globally or per source for dynamic sites.

### 2. Capability probe

Before syncing content, `docsync` probes the site to decide which adapter should run:

- `llms.txt`
- `llms-full.txt`
- `Accept: text/markdown`
- `robots.txt`
- sitemap
- file-type hints such as markdown, PDF, office docs, or OpenAPI
- raw HTML fallback

This is intentionally early because it prevents expensive and brittle fetch decisions later.

### 3. Discovery

Discovery builds a page frontier from:

- repo docs trees
- `llms.txt`
- `llms-full.txt`
- sitemap indexes
- nav manifests
- internal links

If the entry URL is a single-file asset, discovery may be skipped entirely.

### 4. Fetch and normalize

Fetch strategy order:

1. repo-native markdown
2. negotiated markdown
3. `llms` indexes
4. raw HTML normalization
5. headless browser fallback

Current implementation reaches all five steps:

- for `git-docs` sources, clone the repo, resolve the requested ref, detect the docs root, and copy markdown files directly
- fetch pages with `Accept: text/markdown`
- fall back to HTML-to-Markdown conversion when direct markdown is unavailable
- invoke a headless browser fallback when static HTML extraction is too thin for JS-heavy pages
- store markdown bodies under `pages/`
- archive raw response bodies under `raw/`
- write per-page metadata JSON sidecars with response headers and hashes
- reuse validator-backed pages from previous snapshots when possible

### 5. Snapshot write

Each sync writes a versioned snapshot:

```text
snapshots/{source}/{ref}/
  discovery.json
  manifest.json
  pages/
  raw/
```

Current discovery snapshots record:

- detected input kind and suggested mode
- adapters used during discovery
- deduped URL frontier
- llms and sitemap provenance
- optional git summary with resolved ref, docs path, and nav manifests
- fetch summary with stored/skipped/reused counts
- diff summary with `new`, `changed`, `unchanged`, and `removed`
- per-page fetch method, content hashes, validator headers, and optional reused-from metadata
- optional OmniMem import and verify logs per snapshot

## Runtime files

### `config.json`

Stores source registry, proxy/browser defaults, and runtime schema version.

### `manifest.json`

Stores snapshot metadata and sync state for one source/ref.

### `discovery.json`

Stores the discovered page frontier and how it was built for one source/ref.

### `omnimem-import.json`

Stores page-by-page OmniMem import results for one snapshot.

### `omnimem-verify.json`

Stores OmniMem verification-search output for one snapshot.

## Why adapter-based

The same docs pipeline should handle:

- repo-native docs like shadcn-ui or Supabase
- Mintlify docs with `llms.txt`
- Cloudflare-enabled markdown negotiation
- classic docs sites that only expose HTML
- one-off asset URLs like `openapi.json` or `guide.pdf`

Trying to force one crawler path across all of them would make the project both fragile and expensive.
