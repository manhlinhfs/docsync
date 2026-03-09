# Usage

## Initialize local state

```bash
docsync init
```

## Inspect runtime paths

```bash
docsync paths
docsync paths --json
```

## Generate shell completions

```bash
docsync completions bash
docsync completions zsh
```

## Use a proxy for blocked sites

```bash
docsync --proxy http://127.0.0.1:7890 probe https://docs.example.com
docsync --proxy socks5h://user:pass@127.0.0.1:1080 probe https://docs.example.com
docsync source add blocked-site \
  --url https://docs.example.com \
  --proxy http://127.0.0.1:7890 \
  --kind website \
  --version-strategy date-snapshot
```

## Use a custom browser command for dynamic docs

```bash
docsync --browser-cmd /usr/bin/chromium sync blocked-site
docsync source add dynamic-site \
  --url https://docs.example.com \
  --browser-cmd /usr/bin/chromium \
  --kind website \
  --version-strategy date-snapshot
```

## Add a website docs source

```bash
docsync source add postiz \
  --url https://docs.postiz.com \
  --kind website \
  --version-strategy date-snapshot \
  --tag mintlify \
  --tag docs
```

## Add a source from a non-homepage discovery link

```bash
docsync source add postiz-llms \
  --url https://docs.postiz.com/llms.txt \
  --kind website \
  --version-strategy date-snapshot
```

## Probe a sitemap or llms index

```bash
docsync probe https://docs.postiz.com/llms.txt --json
docsync probe https://docs.postiz.com/sitemap.xml --json
```

## Add a git-native docs source

```bash
docsync source add supabase \
  --url https://supabase.com/docs \
  --kind git-docs \
  --repo https://github.com/supabase/supabase \
  --docs-path apps/docs/content \
  --default-ref master \
  --version-strategy git-ref
```

## Probe a docs page

```bash
docsync probe https://docs.postiz.com/introduction
docsync probe https://blog.cloudflare.com/markdown-for-agents/ --json
```

## Add a single page seed

```bash
docsync source add postiz-page \
  --url https://docs.postiz.com/introduction \
  --kind website \
  --version-strategy date-snapshot
```

## Probe a direct import asset

```bash
docsync probe https://example.com/openapi.json --json
docsync probe https://example.com/reference.pdf --json
```

## List sources

```bash
docsync source list
```

## Show one source

```bash
docsync source show postiz
```

## Build a fetched snapshot

```bash
docsync sync postiz --ref snapshot-20260307
docsync sync postiz-llms --dry-run --json
docsync sync postiz-page
docsync sync supabase --ref master
```

When a previous snapshot exists, `docsync sync` reports:

- reused pages
- changed/new pages
- unchanged pages
- removed pages

## Import a snapshot into OmniMem

```bash
docsync import postiz-page --ref snapshot-20260309
docsync import supabase --ref master --dry-run --json
docsync import supabase --ref master --all-pages
```

Incremental snapshots import only `new` and `changed` pages by default.
Duplicate normalized pages inside the same snapshot are skipped by default during import.

## Verify a snapshot with OmniMem search

```bash
docsync verify postiz-page "release notes" --ref snapshot-20260309
docsync verify supabase "auth" --ref master --json
```

## Migrate old runtime data

```bash
docsync migrate
docsync migrate --json
```

## Notes

`v1.2.0` adds multi-engine normalization profiles for Mintlify, Docusaurus, GitBook, MkDocs, Nextra, and VitePress-style docs content on top of the incremental diff-aware sync, normalized import, headless browser fallback, runtime migration, shell completions, and release automation already present in earlier versions.
