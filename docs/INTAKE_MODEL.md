# Intake Model

## Goal

`docsync` should accept any URL or entrypoint that can reasonably lead to importable knowledge, not just the docs homepages mentioned during planning.

That means the system design must start from a generic intake contract:

- one input URL
- classify what it is
- choose the right expansion or import strategy

## Supported entrypoint classes

### Discovery roots

These are links that should expand into more links:

- docs homepages
- `llms.txt`
- `llms-full.txt`
- `robots.txt`
- `sitemap.xml`
- sitemap indexes
- repo docs roots

### Page seeds

These are links that can be imported directly and may optionally expand into nearby navigation:

- a single docs page
- a markdown-negotiated endpoint
- a blog post that is part of a docs corpus

### Single-file assets

These are links that should usually be imported directly without crawl expansion:

- `.md`
- `.mdx`
- `.txt`
- `.pdf`
- `.docx`
- `.pptx`
- `.xlsx`
- `openapi.json`
- `swagger.json`
- `openapi.yaml`
- `schema.json`

## Core rule

Never assume an input URL is a crawl root.

`docsync` should decide whether a link is:

1. `discovery_root`
2. `hybrid_seed`
3. `single_file`
4. `single_document`

## Capability-first strategy

Given one arbitrary link:

1. Normalize URL
2. Probe headers and content type
3. Detect special paths like `llms.txt`, `robots.txt`, `sitemap.xml`, `openapi.json`
4. Attempt markdown negotiation with `Accept: text/markdown`
5. Select an adapter

## Adapter mapping

- `llms.txt` or `llms-full.txt` -> discovery adapter
- `sitemap.xml` -> sitemap adapter
- markdown response -> markdown fetch adapter
- repo-backed docs -> git adapter
- HTML page -> HTML normalization adapter
- PDF or office doc -> document extraction adapter
- OpenAPI/JSON schema -> schema adapter

## Why this matters

If `docsync` is designed only around docs homepages, users still need custom handling for:

- pasted single pages
- raw markdown files
- schema URLs
- PDF docs
- API specs
- discovery indexes

That defeats the point of shipping a general binary.

## Implication for storage

Because input kinds vary, the manifest model should store:

- original entry URL
- detected input kind
- suggested mode
- actual adapter used
- whether the result was expanded or imported directly

This lets later versions remain generic without losing provenance.
