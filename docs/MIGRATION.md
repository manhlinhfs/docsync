# Migration

`docsync` `v1.0.0` keeps runtime schema changes backward-compatible and ships a built-in migration command:

```bash
docsync migrate
docsync migrate --json
```

## What it migrates

- `config.json` schema version updates
- snapshot `manifest.json` files that were written by older releases
- missing diff summaries for older snapshots

## Safety model

- migration rewrites only known `docsync` runtime files
- page bodies under `pages/` and `raw/` are not rewritten
- old manifests are upgraded into the current stable schema instead of being discarded

## When to run it

- after upgrading from a pre-`1.0.0` release
- before scripting around new diff fields in `manifest.json`
- when you want old snapshots to expose the same stable metadata surface as new ones
