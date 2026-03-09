# docsync

[Tiếng Việt](README_vi.md) | [Русский](README_ru.md) | [English](README.md)

`docsync` la CLI viet bang Rust de tao cac snapshot tai lieu ky thuat co version tren may local, sau do co the import vao OmniMem hoac bat ky he thong retrieval nao khac.

Du an duoc thiet ke theo huong **universal intake**, khong chi gom mot vai docs homepage co dinh. Entry URL cua mot source co the la:

- docs homepage
- mot trang docs don
- `llms.txt`
- `llms-full.txt`
- `sitemap.xml`
- `robots.txt`
- mot file markdown hoac MDX raw
- tai lieu OpenAPI hoac JSON schema
- URL PDF hoac office document
- mot docs site lay noi dung tu repo Git rieng

Du an co dinh huong **binary-first**:

- nguoi dung co the tai mot binary release va chay ngay
- trang thai local nam trong cau truc thu muc on dinh, de doan
- luong sync co the script hoa trong CI, cron, hoac terminal
- docs chi co website van phai ingest duoc ma khong can viet scraper rieng cho tung du an

Ho tro proxy cho ca HTTP sync va `git-docs` sync:

- global CLI override: `--proxy http://127.0.0.1:7890`
- per-source setting: `docsync source add ... --proxy ...`
- runtime config: `default_proxy_url` trong `config.json`
- environment fallback: `DOCSYNC_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`

Dang proxy URL duoc ho tro:

- `http://user:pass@host:port`
- `socks5://host:port`
- `socks5h://user:pass@host:port`

Headless browser fallback co the cau hinh qua:

- global CLI override: `--browser-cmd /usr/bin/chromium`
- per-source setting: `docsync source add ... --browser-cmd ...`
- runtime config: `default_browser_cmd` trong `config.json`
- tu dong detect cac Chromium-based browser pho bien tren `PATH`

## Trang thai

Version hien tai: `1.1.0`

Ban phat hanh nay tap trung vao:

- config local va source registry
- probe cac link co the import de xac dinh capability phu hop cho tac vu AI
- tao snapshot dua tren discovery tu `llms.txt`, `llms-full.txt`, sitemap index, va seed page
- fetch trang theo uu tien markdown, luu raw response va sidecar metadata tung page
- git-native docs sync tu ref cua repo, co docs root detection va nav manifest discovery
- HTML fallback conversion cho cac website page khong expose markdown truc tiep
- incremental sync voi snapshot lineage, diff summary, reused pages, va removed page tracking
- headless browser fallback cho dynamic docs page khi static HTML extraction qua mong
- markdown/MDX normalization de loai bo boilerplate pho bien cua docs va flatten cac UI component truoc khi hash hoac import
- lenh import va verify voi OmniMem, kem changed-only incremental import mac dinh va bo qua duplicate content trong cung snapshot
- migrate command cho runtime schema va compatibility guarantee cho manifest/config
- shell completions, GitHub Actions CI/release automation, va install scripts

## Tai sao dung Rust

Rust phu hop voi `docsync` vi du an can:

- mot binary don le, it friction khi chay
- typing ro rang cho manifest, config, va release compatibility
- kha nang concurrency tot hon o cac giai doan crawl/fetch sau nay
- de phan phoi tren Linux, macOS, va Windows

## Workflow du kien

```bash
docsync init
docsync source add postiz --url https://docs.postiz.com --kind website --tag docs --tag mintlify
docsync probe https://docs.postiz.com/introduction
docsync sync postiz --ref snapshot-20260307
docsync import postiz --ref snapshot-20260307
```

## Cac lenh hien co

### `docsync init`

Tao runtime layout local:

- config file
- sources directory
- snapshots directory

Home mac dinh:

```text
~/.docsync
```

Co the override bang:

```bash
docsync --home /path/to/runtime init
DOCSYNC_HOME=/path/to/runtime docsync init
```

### `docsync paths`

In ra cac runtime path da duoc resolve.

### `docsync source add`

Dang ky source definition tu mot entry URL:

```bash
docsync source add shadcn-ui \
  --url https://ui.shadcn.com \
  --proxy http://127.0.0.1:7890 \
  --browser-cmd /usr/bin/chromium \
  --kind git-docs \
  --repo https://github.com/shadcn-ui/ui \
  --docs-path apps/v4/content/docs \
  --default-ref main \
  --version-strategy git-ref \
  --tag components \
  --tag docs
```

### `docsync source list`

Liet ke cac source da cau hinh.

### `docsync source show <name>`

Hien thi chi tiet mot source.

### `docsync probe <url>`

Probe mot docs URL de kiem tra cac capability quan trong cho sync huong AI:

- ho tro `Accept: text/markdown`
- `x-markdown-tokens`
- `x-original-tokens`
- `content-signal`
- `llms.txt`
- `robots.txt`
- sitemap

Lenh nay hien la lenh huu ich nhat vi no cho biet site can adapter strategy nao truoc khi viet crawler.
Lenh nay cung ton trong `--proxy` hoac configured default proxy khi site chan datacenter IP.

No dong thoi classify URL thanh cac intake mode tong quat nhu:

- discovery root
- page seed
- single file asset
- markdown endpoint
- `llms` index
- sitemap hoac robots discovery seed

### `docsync import <source>`

Import mot snapshot da luu vao OmniMem va ghi `omnimem-import.json` trong snapshot do.

Voi incremental snapshot, mac dinh lenh nay chi import cac page `new` va `changed`.
Dung `--all-pages` neu ban muon full re-import.
Neu nhieu page normalize thanh cung mot content hash, `docsync import` mac dinh chi import mot ban.

### `docsync verify <source> <query>`

Chay OmniMem search helper tren mot snapshot da luu va ghi `omnimem-verify.json`.

### `docsync completions <shell>`

Sinh shell completion cho `bash`, `zsh`, `fish`, `elvish`, hoac `powershell`.

### `docsync migrate`

Rewrite config runtime va snapshot manifests cu sang stable schema hien tai.

### `docsync sync <source>`

`v1.1.0` thuc hien discovery va sau do chon incremental markdown-first HTTP fetch, HTML fallback, headless fallback, hoac git-native repo sync:

- snapshot directory
- `discovery.json`
- URL frontier da discover tu `llms.txt`, `llms-full.txt`, sitemap index, direct seed page, hoac repo-native docs tree
- normalized markdown pages trong `pages/`
- per-page metadata sidecars trong `pages/`
- raw response bodies trong `raw/`
- `manifest.json`
- tuy chon `omnimem-import.json`
- tuy chon `omnimem-verify.json`

Voi `git-docs`, `docsync` clone repo, resolve ref duoc yeu cau, detect hoac dung `docs_path`, snapshot truc tiep cac file markdown, va record nav manifests nhu `meta.json`, `docs.json`, hoac `mint.json`.
Voi page seed chi tra HTML, `docsync` convert HTML do sang Markdown va luu raw HTML body cung noi dung da normalize.
Voi nguon Markdown va MDX, `docsync` normalize cac component pho bien nhu callout, tab, step, card, duplicate heading va boilerplate o dau trang truoc khi hash, diff, hoac import.
Neu da co snapshot truoc do, `docsync` se ghi them `new`, `changed`, `unchanged`, `removed` vao `manifest.json`, reuse page co validator, va report diff counts tren CLI output.

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

## Phat trien

Build:

```bash
cargo build
```

Chay test:

```bash
cargo test
```

Format:

```bash
cargo fmt
```

## Tai lieu

- [ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [INTAKE_MODEL.md](docs/INTAKE_MODEL.md)
- [LANGUAGE_POLICY.md](docs/LANGUAGE_POLICY.md)
- [MIGRATION.md](docs/MIGRATION.md)
- [RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md)
- [USAGE.md](docs/USAGE.md)
- [RELEASE_POLICY.md](docs/RELEASE_POLICY.md)
- [ROADMAP.md](ROADMAP.md)
