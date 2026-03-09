# Contributing

## Goal

Keep `docsync` practical, inspectable, and easy to distribute as a single CLI binary.

## Engineering rules

1. Prefer explicit manifest/schema changes over hidden magic.
2. New adapters must record provenance and fetch method.
3. Avoid adding heavyweight runtime dependencies unless they unlock a clear roadmap milestone.
4. Every new feature should fit the binary-first distribution model.
5. Every user-visible behavior change should update docs and roadmap notes.

## Local workflow

```bash
cargo fmt
cargo test
cargo build
```

## Pull request checklist

- code compiles
- tests pass
- docs updated
- roadmap milestone updated if scope changed
- manifest/config schema changes are documented

## Release mindset

This project is expected to ship prebuilt binaries. That means:

- startup UX matters
- error messages matter
- config path behavior must stay predictable
- breaking config/schema changes require an explicit migration plan
