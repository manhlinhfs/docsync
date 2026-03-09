# docsync

[Tiếng Việt](README_vi.md) | [Русский](README_ru.md) | [English](README.md)

> **docsync** là một *CLI binary-first* giúp bạn biến website tài liệu hoặc repo docs thành các snapshot Markdown sạch trên máy local, sẵn sàng để đưa vào **OmniMem** hoặc bất kỳ hệ thống RAG nào khác.

## **docsync Dùng Để Làm Gì?**

Với một công cụ duy nhất, bạn có thể:

- **Probe** một URL tài liệu để biết nên xử lý theo kiểu nào
- **Sync** website docs hoặc repo docs về máy
- **Làm sạch** Markdown/MDX trước khi hash hoặc import
- **Theo dõi thay đổi** giữa các snapshot
- **Import** chỉ những trang mới hoặc đã thay đổi vào OmniMem
- **Xem lại dữ liệu** bằng dashboard local trên trình duyệt
- **Gửi thông báo** tóm tắt sang Telegram bot

Phù hợp nhất với:

- **Website docs** có `llms.txt`, `llms-full.txt`, `sitemap.xml`
- **Docs dạng Markdown**
- **Docs HTML** có thể chuyển sang Markdown
- **Docs render bằng JS** với headless fallback
- **Docs lấy từ repo Git**

Phiên bản stable hiện tại: **`1.3.0`**

Các mốc phát triển tiếp theo sau `v1.3.0`: xem [ROADMAP.md](ROADMAP.md).

## **Bắt Đầu Nhanh**

### **1. Khởi tạo thư mục runtime**

```bash
docsync init
```

Thư mục mặc định:

```text
~/.docsync
```

### **2. Kiểm tra nhanh một URL docs**

```bash
docsync probe https://docs.openclaw.ai/ --json
```

Lệnh này cho bạn biết:

- site có hỗ trợ `text/markdown` hay không
- có `llms.txt` hay không
- có sitemap hay không
- URL đó là **trang gốc**, **một page đơn**, hay **một file docs**

### **3. Thêm source**

```bash
docsync source add openclaw \
  --url https://docs.openclaw.ai/ \
  --kind website \
  --version-strategy date-snapshot
```

### **4. Sync**

```bash
docsync sync openclaw --json
```

### **5. Import vào OmniMem**

```bash
docsync import openclaw
```

## **Ví Dụ Thực Tế**

### **OpenClaw: sync toàn bộ docs website**

```bash
docsync source add openclaw \
  --url https://docs.openclaw.ai/ \
  --kind website \
  --version-strategy date-snapshot

docsync sync openclaw
docsync import openclaw --dry-run --json
```

Rất phù hợp khi site có:

- `llms.txt`
- sitemap
- hỗ trợ trả Markdown trực tiếp

### **shadcn/ui: sync từ repo Git**

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

Hãy dùng `git-docs` khi nội dung docs chuẩn nằm trong repo.

### **Supabase: sync docs website**

```bash
docsync source add supabase \
  --url https://supabase.com/docs \
  --kind website \
  --version-strategy date-snapshot

docsync sync supabase
```

### **Chỉ sync một trang duy nhất**

```bash
docsync source add openclaw-getting-started \
  --url https://docs.openclaw.ai/start/getting-started \
  --kind website \
  --version-strategy date-snapshot

docsync sync openclaw-getting-started
```

Phù hợp khi bạn chỉ muốn nhập một page, không cần crawl cả site.

## **Cách Nghĩ Về `docsync`**

Mô hình đơn giản nhất là:

1. **Probe** URL
2. **Discover** danh sách trang từ `llms.txt`, `llms-full.txt`, sitemap, hoặc page seed
3. **Fetch** nội dung tốt nhất có thể lấy được
4. **Normalize** để làm sạch Markdown/MDX
5. **Ghi snapshot** vào local
6. **Import** phần thực sự cần import

## **Làm Sạch Dữ Liệu**

`docsync` không import thẳng MDX thô.

Trước khi hash hoặc import, công cụ sẽ làm sạch các phần nhiễu thường gặp như:

- tiêu đề H1 bị lặp
- khối boilerplate ở đầu trang
- component UI như callout, tab, step, card, tooltip
- wrapper Markdown/MDX không cần thiết
- các trang trùng nội dung khi import vào OmniMem

Hiện tại các profile làm sạch đã bao phủ những pattern phổ biến của **Mintlify**, **Docusaurus**, **GitBook**, **MkDocs**, **Nextra** và **VitePress**.

Điều đó có nghĩa:

- **`pages/`** chứa nội dung Markdown đã được làm sạch
- **`raw/`** chứa response gốc lấy từ nguồn

## **Chấm Điểm Chất Lượng Và Xem Lại Dữ Liệu**

Mỗi page đã lưu bây giờ có thêm điểm chất lượng dựa trên:

- có tiêu đề chuẩn hay không
- mật độ nội dung text
- còn sót HTML hoặc MDX bao nhiêu
- độ dài và cấu trúc nội dung

Tạo dashboard HTML local để mở bằng trình duyệt:

```bash
docsync dashboard openclaw
```

## **Incremental Sync**

Nếu source đã có snapshot cũ, `docsync` sẽ phân loại page thành:

- **new**
- **changed**
- **unchanged**
- **removed**

Mặc định, `docsync import` chỉ nhập:

- **trang mới**
- **trang đã thay đổi**

Nếu nhiều page sau khi normalize cho ra cùng một content hash, mặc định chỉ một bản được import.

Muốn import lại toàn bộ:

```bash
docsync import openclaw --all-pages
```

Muốn vẫn import cả các page low-signal:

```bash
docsync import openclaw --include-low-signal
```

## **Thông Báo Telegram**

Gửi tóm tắt snapshot sang Telegram bot:

```bash
export DOCSYNC_TELEGRAM_BOT_TOKEN=123456:ABCDEF
export DOCSYNC_TELEGRAM_CHAT_ID=-1001234567890

docsync notify telegram openclaw
```

## **Proxy Và Headless**

### **Dùng proxy**

Hỗ trợ:

- **HTTP**
- **HTTPS**
- **SOCKS5**
- **SOCKS5h**

Ví dụ:

```bash
docsync --proxy socks5h://user:pass@127.0.0.1:1080 probe https://docs.example.com
```

Hoặc lưu proxy theo từng source:

```bash
docsync source add blocked-site \
  --url https://docs.example.com \
  --kind website \
  --version-strategy date-snapshot \
  --proxy http://user:pass@host:port
```

### **Dùng browser fallback**

```bash
docsync --browser-cmd /usr/bin/chromium sync some-dynamic-site
```

Phù hợp với các site docs render nội dung chủ yếu bằng JavaScript.

## **Cấu Trúc Thư Mục Runtime**

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

Các file quan trọng:

- **`discovery.json`**: docsync tìm ra frontier bằng cách nào
- **`manifest.json`**: tóm tắt snapshot, fetch, diff, metadata từng page
- **`pages/`**: Markdown đã normalize
- **`raw/`**: nội dung gốc lấy từ nguồn

## **Những Lệnh Quan Trọng Nhất**

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

## **Phát Triển**

```bash
cargo fmt --check
cargo test
cargo build --release
```

## **Tài Liệu Khác**

- [Usage](docs/USAGE.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Migration](docs/MIGRATION.md)
- [Roadmap](ROADMAP.md)
- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)
