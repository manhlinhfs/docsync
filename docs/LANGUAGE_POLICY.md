# Language Policy

`docsync` keeps root-level user documentation aligned across the same language set used by OmniMem:

- English: `README.md`
- Vietnamese: `README_vi.md`
- Russian: `README_ru.md`

## Scope

The translation parity requirement currently applies to:

- release overview and feature status
- install and quickstart commands
- command summaries
- proxy support notes
- links to core project documentation

Detailed engineering docs under `docs/` remain English-first unless a release explicitly adds translated variants for them.

## Update rule

Any change that affects root-level user-facing behavior must update all three README variants in the same change set. That includes:

- new commands
- changed flags or config fields
- release status and version number
- workflow examples
- supported transport or proxy modes

## Release gate

A release is not considered documentation-complete until:

1. `README.md`
2. `README_vi.md`
3. `README_ru.md`

all describe the same product capabilities for that release.
