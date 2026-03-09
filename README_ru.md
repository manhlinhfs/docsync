# docsync

[Tiếng Việt](README_vi.md) | [Русский](README_ru.md) | [English](README.md)

> **docsync** — это *binary-first CLI*, который превращает сайты с документацией и docs-репозитории в чистые локальные Markdown snapshot'ы, готовые для **OmniMem** или любого другого RAG-контура.

## **Что Делает `docsync`**

С помощью одного инструмента можно:

- **Проверить** docs URL перед синком
- **Скачать** сайт документации или docs-репозиторий в локальный snapshot
- **Нормализовать** Markdown/MDX перед hash и import
- **Отслеживать изменения** между snapshot'ами
- **Импортировать** в OmniMem только новые или изменённые страницы
- **Просматривать** собранные данные через локальный dashboard
- **Отправлять** summary в Telegram bot

Лучше всего подходит для:

- **docs-сайтов** с `llms.txt`, `llms-full.txt`, `sitemap.xml`
- **Markdown-first документации**
- **HTML-документации** с fallback в Markdown
- **JS-heavy docs** с headless fallback
- **Git-native docs-репозиториев**

Текущая stable-версия: **`1.3.1`**

Следующие этапы после `v1.3.1`: см. [ROADMAP.md](ROADMAP.md).

## **Быстрый Старт**

### **1. Инициализация**

```bash
docsync init
```

Каталог по умолчанию:

```text
~/.docsync
```

### **2. Probe URL**

```bash
docsync probe https://docs.openclaw.ai/ --json
```

Команда показывает:

- поддерживает ли сайт `text/markdown`
- есть ли `llms.txt`
- есть ли sitemap
- является ли URL **корнем сайта**, **одной страницей** или **отдельным asset**

### **3. Добавить source**

```bash
docsync source add openclaw \
  --url https://docs.openclaw.ai/ \
  --kind website \
  --version-strategy date-snapshot
```

### **4. Выполнить sync**

```bash
docsync sync openclaw --json
```

### **5. Импортировать в OmniMem**

```bash
docsync import openclaw
```

## **Практические Примеры**

### **OpenClaw: полный sync docs-сайта**

```bash
docsync source add openclaw \
  --url https://docs.openclaw.ai/ \
  --kind website \
  --version-strategy date-snapshot

docsync sync openclaw
docsync import openclaw --dry-run --json
```

Это хороший кейс, когда сайт имеет:

- `llms.txt`
- sitemap
- выдачу Markdown по HTTP

### **shadcn/ui: sync из Git-репозитория**

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

Используйте `git-docs`, когда source of truth находится в репозитории.

### **Supabase: sync docs-сайта**

```bash
docsync source add supabase \
  --url https://supabase.com/docs \
  --kind website \
  --version-strategy date-snapshot

docsync sync supabase
```

### **Только одна страница**

```bash
docsync source add openclaw-getting-started \
  --url https://docs.openclaw.ai/start/getting-started \
  --kind website \
  --version-strategy date-snapshot

docsync sync openclaw-getting-started
```

Полезно, когда нужно импортировать только один page seed без полного обхода сайта.

## **Простая Модель Работы**

`docsync` работает так:

1. **Probe** URL
2. **Discover** frontier из `llms.txt`, `llms-full.txt`, sitemap или page seed
3. **Fetch** лучший доступный формат
4. **Normalize** контент в более чистый Markdown
5. **Write** локальный snapshot
6. **Import** только нужные страницы

## **Нормализация Контента**

`docsync` не импортирует сырой MDX как есть.

Перед hash или import он очищает типичный docs-шум:

- дублирующиеся H1
- boilerplate в начале страницы
- UI-компоненты вроде callout, tabs, steps, cards, tooltip
- лишние Markdown/MDX wrapper'ы
- duplicate-content страницы при import в OmniMem

Встроенные profile cleanup уже покрывают типовые patterns из **Mintlify**, **Docusaurus**, **GitBook**, **MkDocs**, **Nextra** и **VitePress**.

Итог:

- **`pages/`** содержит **нормализованный Markdown**
- **`raw/`** содержит **исходный body ответа**

## **Quality Score И Review**

Теперь каждая сохранённая page получает quality score на основе:

- наличия корректного title
- плотности текстового содержимого
- остаточного HTML или MDX noise
- длины и структуры контента

Собрать локальный HTML dashboard:

```bash
docsync dashboard openclaw
docsync dashboard openclaw --serve --host 127.0.0.1 --port 4317
docsync dashboard openclaw --status
docsync dashboard openclaw --stop
```

## **Incremental Sync**

Если у source уже есть прошлый snapshot, `docsync` классифицирует страницы как:

- **new**
- **changed**
- **unchanged**
- **removed**

По умолчанию `docsync import` импортирует только:

- **новые страницы**
- **изменённые страницы**

Если несколько страниц после normalization дают одинаковый content hash, импортируется только одна копия.

Для полного re-import:

```bash
docsync import openclaw --all-pages
```

Если нужно всё равно импортировать low-signal pages:

```bash
docsync import openclaw --include-low-signal
```

## **Telegram Notifications**

Отправить summary snapshot'а в Telegram bot:

```bash
export DOCSYNC_TELEGRAM_BOT_TOKEN=123456:ABCDEF
export DOCSYNC_TELEGRAM_CHAT_ID=-1001234567890

docsync notify telegram openclaw
```

## **Proxy И Headless**

### **Proxy**

Поддерживаются:

- **HTTP**
- **HTTPS**
- **SOCKS5**
- **SOCKS5h**

Пример:

```bash
docsync --proxy socks5h://user:pass@127.0.0.1:1080 probe https://docs.example.com
```

Или сохранить proxy на уровне source:

```bash
docsync source add blocked-site \
  --url https://docs.example.com \
  --kind website \
  --version-strategy date-snapshot \
  --proxy http://user:pass@host:port
```

### **Headless browser fallback**

```bash
docsync --browser-cmd /usr/bin/chromium sync some-dynamic-site
```

Это полезно для docs-сайтов, где основной контент появляется только после JS-render.

## **Структура Runtime**

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

Ключевые файлы:

- **`discovery.json`**: как был собран frontier
- **`manifest.json`**: summary snapshot'а, fetch, diff и page metadata
- **`pages/`**: нормализованный Markdown
- **`raw/`**: исходный response body

## **Самые Полезные Команды**

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

## **Разработка**

```bash
cargo fmt --check
cargo test
cargo build --release
```

## **Дополнительная Документация**

- [Usage](docs/USAGE.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Migration](docs/MIGRATION.md)
- [Roadmap](ROADMAP.md)
- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)
