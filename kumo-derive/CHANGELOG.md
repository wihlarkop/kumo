# Changelog

All notable changes to `kumo-derive` will be documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-04-25

### Added

- `#[derive(Extract)]` proc-macro generating async `Extract` trait implementations
- `#[extract(css = "selector")]` — required CSS selector per field
- `#[extract(attr = "name")]` — read an HTML attribute instead of text content
- `#[extract(re = r"pattern")]` — apply a regex and return the first match / capture group 1
- `#[extract(text)]` — explicit text extraction (default; can be omitted)
- `#[extract(llm_fallback = "hint")]` — fall back to an LLM client when the selector returns empty
- `#[extract(llm_fallback)]` — bare form, uses the field name as the extraction hint
- `String` fields use `unwrap_or_default()` on missing matches
- `Option<String>` fields stay as `None` when not found
- Batch LLM call: all `llm_fallback` fields for one struct use a single `extract_json` call
- Clear compile errors for unknown attributes and missing required `css` key
- trybuild test suite: 3 pass cases + 4 fail cases with `.stderr` snapshots
