# docsync

[Tiếng Việt](README_vi.md) | [Русский](README_ru.md) | [English](README.md)

> **docsync** is a *binary-first CLI* that turns documentation sites and docs repos into clean, local Markdown snapshots ready for **OmniMem** or any other RAG pipeline.

## **What It Does**

With one tool, you can:

- **Probe** a docs URL to see what kind of source it is
- **Sync** a website or docs repo into a local snapshot
- **Normalize** noisy Markdown/MDX before hashing or import
- **Track changes** across snapshots with incremental sync
- **Import** only new or changed pages into OmniMem
- **Chunk** normalized docs into section-aware Markdown artifacts
- **Review** collected pages in a local dashboard
- **Notify** Telegram bots with snapshot summaries

It works well for:

- **Docs websites** with `llms.txt`, `llms-full.txt`, or `sitemap.xml`
- **Markdown-first docs**
- **HTML docs** with Markdown fallback
- **JS-heavy docs** with optional headless fallback
- **Git-native docs repos**

Current stable version: **`1.4.0`**

Planned milestones after `v1.4.0`: see [ROADMAP.md](ROADMAP.md).

## **Quick Start**

### **1. Initialize local state**

```bash
docsync init
```

Default home:

```text
~/.docsync
```

### **2. Probe a docs URL**

```bash
docsync probe https://docs.openclaw.ai/ --json
```

This tells you:

- whether the site supports `text/markdown`
- whether `llms.txt` exists
- whether sitemap discovery is available
- whether the URL looks like a **site root**, **single page**, or **asset**

### **3. Add a source**

```bash
docsync source add openclaw \
  --url https://docs.openclaw.ai/ \
  --kind website \
  --version-strategy date-snapshot
```

### **4. Sync**

```bash
docsync sync openclaw --json
docsync sync openclaw --chunk-target-words 180 --chunk-overlap-words 30
```

### **5. Import into OmniMem**

```bash
docsync import openclaw
```

Or do sync and import in one step:

```bash
docsync sync openclaw --import
```

## **Real Examples**

### **OpenClaw: sync a full docs website**

```bash
docsync source add openclaw \
  --url https://docs.openclaw.ai/ \
  --kind website \
  --version-strategy date-snapshot

docsync sync openclaw
docsync import openclaw --dry-run --json
```

If you want automatic OmniMem sync every time:

```bash
docsync source add openclaw \
  --url https://docs.openclaw.ai/ \
  --kind website \
  --version-strategy date-snapshot \
  --auto-import
```

Good fit when the docs site has:

- `llms.txt`
- sitemap discovery
- Markdown negotiation

### **shadcn/ui: sync from the Git repo**

```bash
docsync source add shadcn-ui \
  --url https://ui.shadcn.com \
  --kind git-docs \
  --repo https://github.com/shadcn-ui/ui \
  --docs-path apps/v4/content/docs \
  --default-ref main \
  --version-strategy git-ref

docsync sync shadcn-ui --ref main
```

Use `git-docs` when the docs source of truth is the repository itself.

### **Supabase: sync a docs website**

```bash
docsync source add supabase \
  --url https://supabase.com/docs \
  --kind website \
  --version-strategy date-snapshot

docsync sync supabase
```

### **Single page only**

```bash
docsync source add openclaw-getting-started \
  --url https://docs.openclaw.ai/start/getting-started \
  --kind website \
  --version-strategy date-snapshot

docsync sync openclaw-getting-started
```

This is useful when you want to import one page without expanding the whole site.

If the page URL clearly looks like docs, such as `/docs/...`, `/guides/...`, or `/start/...`, and the same host exposes `llms.txt` or sitemap indexes, `docsync` can automatically promote that page seed into full-site docs discovery.

## **Automatic OmniMem Sync**

Use one-shot automation:

```bash
docsync sync drizzle --import
```

Or enable it on the source itself:

```bash
docsync source auto-import drizzle --enable
docsync source auto-import drizzle --enable --omnimem-direct
```

After that, a normal `docsync sync drizzle` will fetch docs and immediately import the snapshot into OmniMem.

## **Section-Aware Chunking**

`docsync` now writes both:

- `pages/` for cleaned page-level Markdown
- `chunks/` for section-aware Markdown chunks

When chunk metadata exists, OmniMem import prefers `chunks/`.

Tune chunk sizing per sync:

```bash
docsync sync drizzle --chunk-target-words 180 --chunk-overlap-words 30
```

## **How `docsync` Thinks**

Keep the model simple:

1. **Probe** the URL
2. **Discover** pages from `llms.txt`, `llms-full.txt`, sitemap, or a direct seed
3. **Fetch** the best available representation
4. **Normalize** the content into cleaner Markdown
5. **Write** a local snapshot
6. **Import** only what changed

## **Normalization**

`docsync` does not blindly import raw MDX.

Before hashing or import, it cleans common docs noise such as:

- duplicated top headings
- top-of-page docs index boilerplate
- UI components like callouts, tabs, steps, cards, and tooltips
- noisy Markdown/MDX wrappers
- duplicate-content pages during OmniMem import

Built-in cleanup profiles now cover common patterns from **Mintlify**, **Docusaurus**, **GitBook**, **MkDocs**, **Nextra**, and **VitePress**.

That means the content under `pages/` is the **normalized** version.
The content under `raw/` is the **original fetched body**.

## **Quality And Review**

Each stored page now gets a quality score based on:

- title presence
- text density
- residual HTML or MDX noise
- content length and structure

Build a browser-friendly review page with:

```bash
docsync dashboard --all
docsync dashboard --all --serve --host 0.0.0.0 --port 4317
docsync dashboard --all --status
docsync dashboard --all --stop

docsync dashboard openclaw
docsync dashboard openclaw --serve --host 127.0.0.1 --port 4317
docsync dashboard openclaw --status
docsync dashboard openclaw --stop
```

## **Incremental Sync**

If a source already has an earlier snapshot, `docsync` will classify pages as:

- **new**
- **changed**
- **unchanged**
- **removed**

By default, `docsync import` only imports:

- **new pages**
- **changed pages**

If multiple pages normalize to the same content hash, only one copy is imported by default.

Force a full re-import with:

```bash
docsync import openclaw --all-pages
```

Include low-signal pages anyway:

```bash
docsync import openclaw --include-low-signal
```

## **Telegram Notifications**

Send a snapshot summary to a Telegram bot:

```bash
export DOCSYNC_TELEGRAM_BOT_TOKEN=123456:ABCDEF
export DOCSYNC_TELEGRAM_CHAT_ID=-1001234567890

docsync notify telegram openclaw
```

## **Proxy And Headless**

### **Use a proxy**

HTTP, HTTPS, SOCKS5, and SOCKS5h are supported:

```bash
docsync --proxy socks5h://user:pass@127.0.0.1:1080 probe https://docs.example.com
```

You can also store the proxy per source:

```bash
docsync source add blocked-site \
  --url https://docs.example.com \
  --kind website \
  --version-strategy date-snapshot \
  --proxy http://user:pass@host:port
```

### **Use a browser fallback**

```bash
docsync --browser-cmd /usr/bin/chromium sync some-dynamic-site
```

This is useful for docs sites that render most content in the browser.

## **Runtime Layout**

```text
~/.docsync/
  config.json
  sources/
  snapshots/
    openclaw/
      snapshot-20260309/
        discovery.json
        manifest.json
        pages/
        raw/
        omnimem-import.json
        omnimem-verify.json
```

Important files:

- **`discovery.json`**: how the page frontier was discovered
- **`manifest.json`**: snapshot summary, fetch summary, diff summary, page metadata
- **`pages/`**: normalized Markdown
- **`raw/`**: original fetched body

## **Most Useful Commands**

```bash
docsync init
docsync paths
docsync probe <url>
docsync source add <name> --url <url> ...
docsync source list
docsync source show <name>
docsync sync <name>
docsync import <name>
docsync verify <name> "<query>"
docsync migrate
docsync completions bash
```

## **Development**

```bash
cargo fmt --check
cargo test
cargo build --release
```

## **More Docs**

- [Usage](docs/USAGE.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Migration](docs/MIGRATION.md)
- [Roadmap](ROADMAP.md)
- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)
