# docsync

[Tiếng Việt](README_vi.md) | [Русский](README_ru.md) | [English](README.md)

`docsync` - eto CLI na Rust dlya sborki lokalnykh versioned-snapshotov tekhnicheskoi dokumentatsii, kotorye potom mozhno importirovat v OmniMem ili lyubuyu druguyu retrieval-sistemu.

Proekt stroitsya vokrug **universal intake**, a ne vokrug neskolkikh zhestko zadannykh docs homepage. Entry URL istochnika mozhet byt:

- docs homepage
- odna stranitsa dokumentatsii
- `llms.txt`
- `llms-full.txt`
- `sitemap.xml`
- `robots.txt`
- syroi markdown ili MDX fail
- dokument OpenAPI ili JSON schema
- URL PDF ili office dokumenta
- docs site, gde istochnik pravdy zhivet v Git repo

Proekt namerenno sdelan v modele **binary-first**:

- polzovatel dolzhen mozhno skachat odin release artifact i srazu zapustit ego
- lokalnoe sostoyanie dolzhno zhit v predskazuemom kataloge
- sync workflow dolzhen skriptovatsya v CI, cron ili terminal sessions
- docs, dostupnye tolko cherez website, vse ravno dolzhny byt ingestible bez ruchek per-project scraper script

Proxy podderzhivaetsya i dlya HTTP sync, i dlya `git-docs` sync:

- global CLI override: `--proxy http://127.0.0.1:7890`
- per-source setting: `docsync source add ... --proxy ...`
- runtime config: `default_proxy_url` v `config.json`
- fallback iz environment: `DOCSYNC_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`

Podderzhivaemye formy proxy URL:

- `http://user:pass@host:port`
- `socks5://host:port`
- `socks5h://user:pass@host:port`

## Status

Tekushchaya versiya: `0.6.0`

Etot release fokusiruetsya na:

- lokalnom config i source registry
- probe proizvolnykh importable links dlya agent-friendly capability detection
- snapshot generation na osnove discovery iz `llms.txt`, `llms-full.txt`, sitemap indexes i seed pages
- markdown-first fetch stranits s arkhirovaniem raw response i sidecar metadata dlya kazhdoi stranitsy
- git-native docs sync iz repository refs s docs root detection i nav manifest discovery
- HTML fallback conversion dlya saitov, kotorye ne otdayut markdown napryamuyu
- komandakh OmniMem import i verify s per-snapshot logs
- bazovoi documentation, roadmap i architecture

Incremental sync, podderzhka headless docs i release engineering ostayutsya v sleduyushchikh milestone.

## Pochemu Rust

Rust podkhodit dlya `docsync`, potomu chto proektu nuzhny:

- odin otdelnyi binary s nizkim runtime friction
- zhestkaya tipizatsiya dlya manifests, config i release compatibility
- khoroshaya osnova dlya budushchikh concurrent crawling i fetch pipelines
- prostoe rasprostranenie na Linux, macOS i Windows

## Predpolagaemyi workflow

```bash
docsync init
docsync source add postiz --url https://docs.postiz.com --kind website --tag docs --tag mintlify
docsync probe https://docs.postiz.com/introduction
docsync sync postiz --ref snapshot-20260307
```

## Tekushchie komandy

### `docsync init`

Sozdaet lokalnyi runtime layout:

- config file
- sources directory
- snapshots directory

Dom po umolchaniyu:

```text
~/.docsync
```

Mozhno pereopredelit tak:

```bash
docsync --home /path/to/runtime init
DOCSYNC_HOME=/path/to/runtime docsync init
```

### `docsync paths`

Pechataet resolve-runtime paths.

### `docsync source add`

Registriruet source definition po entry URL:

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

Pokazyvaet vse nastroennye sources.

### `docsync source show <name>`

Pokazyvaet odin source.

### `docsync probe <url>`

Probe dokumentatsionnogo URL dlya capability, vazhnykh dlya AI-oriented sync:

- podderzhka `Accept: text/markdown`
- `x-markdown-tokens`
- `x-original-tokens`
- `content-signal`
- `llms.txt`
- `robots.txt`
- nalichie sitemap

Seichas eto samaya poleznaya komanda, potomu chto ona pokazyvaet, kakaya adapter strategy nuzhna saitu do napisaniya crawler.
Komanda takzhe uvazhaet `--proxy` ili configured default proxy, kogda sait blokiruet datacenter IP.

Takzhe ona classify URL v obshchie intake modes:

- discovery root
- page seed
- single file asset
- markdown endpoint
- `llms` index
- sitemap ili robots discovery seed

### `docsync import <source>`

Importiruet sokhranennyi snapshot v OmniMem i zapisivaet `omnimem-import.json` v etot snapshot.

### `docsync verify <source> <query>`

Zapuskayet OmniMem search helper protiv sokhranennogo snapshot i zapisivaet `omnimem-verify.json`.

### `docsync sync <source>`

V `v0.6.0` komanda vypolnyaet discovery, a zatem markdown-first HTTP fetch, HTML fallback ili git-native repo sync:

- snapshot directory
- `discovery.json`
- discovered URL frontier iz `llms.txt`, `llms-full.txt`, sitemap indexes, direct seed pages ili repo-native docs trees
- markdown pages v `pages/`
- sidecar metadata dlya kazhdoi stranitsy v `pages/`
- raw response bodies v `raw/`
- `manifest.json`
- optional `omnimem-import.json`
- optional `omnimem-verify.json`

Dlya `git-docs` `docsync` kloniruet repo, resolveit zaproshennyi ref, detectit ili ispolzuet `docs_path`, snapshotit markdown files napryamuyu i zapisivaet nav manifests, takie kak `meta.json`, `docs.json` ili `mint.json`.
Dlya HTML-only page seed `docsync` konvertiruet poluchennyi HTML v Markdown i sokhranyaet raw HTML body vmeste s normalized page.

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

## Razrabotka

Build:

```bash
cargo build
```

Testy:

```bash
cargo test
```

Format:

```bash
cargo fmt
```

## Dokumentatsiya

- [ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [INTAKE_MODEL.md](docs/INTAKE_MODEL.md)
- [LANGUAGE_POLICY.md](docs/LANGUAGE_POLICY.md)
- [USAGE.md](docs/USAGE.md)
- [RELEASE_POLICY.md](docs/RELEASE_POLICY.md)
- [ROADMAP.md](ROADMAP.md)
