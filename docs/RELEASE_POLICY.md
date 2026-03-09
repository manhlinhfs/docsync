# Release Policy

## Versioning

`docsync` uses semantic versioning.

### Stable releases after `1.0.0`

After `1.0.0`, user-visible command behavior and schema files should remain backward-compatible unless a migration command and release note explicitly say otherwise.

### Patch releases

Patch releases should not change config or manifest formats unless they fix a correctness bug and include a migration note.

## Release quality gates

Every release should pass:

1. `cargo fmt --check`
2. `cargo test`
3. `cargo build --release`
4. documentation review for new or changed commands
5. root README language parity review for English, Vietnamese, and Russian

## Binary distribution

Target release artifacts:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Each release should eventually include:

- binary archives
- checksums
- changelog notes
- example quickstart commands
- synchronized `README.md`, `README_vi.md`, and `README_ru.md`

## Schema files that need explicit compatibility notes

- `config.json`
- `manifest.json`
- `discovery.json`
- per-page metadata sidecars
