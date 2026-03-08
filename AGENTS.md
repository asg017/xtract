# AGENTS.md

## Project Overview

CLI that converts Zod schemas to JSON Schema and extracts structured data from images/PDFs via vision models (OpenRouter, LlamaBarn, custom URLs). Supports markdown recipe files that bundle prompt + schema. Uses `rquickjs` to run JS/Zod in an embedded QuickJS runtime.

## Architecture

```
src/
  main.rs          — CLI entry, dispatches to commands
  cli.rs           — Cli struct and Command enum (clap derive)
  js_runner.rs     — QuickJS runtime for Zod→JSON Schema conversion
  markdown.rs      — Parses .md recipe files into sections (prompt + ```schema block)
  pages.rs         — Page range parsing
  progress.rs      — Progress tracking
  sqlite.rs        — SQLite result insertion
  commands/
    schema.rs      — `schema` subcommand: Zod→JSON Schema
    check.rs       — `check` subcommand: validate recipe files
    extract.rs     — `extract` subcommand: image/PDF/markdown → structured JSON via LLMs
js/                — JS bundle for Zod runtime (built from js/entry.js via build.rs)
viewer/            — Svelte/SvelteKit web frontend for viewing results
```

Extract supports three input modes (schema always comes first):
1. **Image**: `extract schema.js photo.jpg --prompt "..."`
2. **PDF**: `extract schema.js doc.pdf --prompt "..." [--page N] [--screenshot]`
3. **Markdown**: `extract recipe.md doc.pdf [--page N] [--name section]` — .md file contains prompt text and ` ```schema ` blocks (Zod or raw JSON Schema), no separate `--prompt`/schema args needed

Multiple inputs supported: `extract schema.js *.pdf -o results.db`
Clipboard input: `extract schema.js clipboard --prompt "..."`

## Building & Testing

```sh
cargo build
cargo test          # markdown parser tests
```

The `build.rs` runs `npx esbuild` to bundle `js/entry.js` → `js/bundle.js`. Requires Node.js and esbuild.

## External Dependencies

- `pdftoppm` (from poppler) — required at runtime for `extract --screenshot`
- OpenRouter/LlamaBarn API key — required for `extract` subcommand
- Node.js + esbuild — required at build time for JS bundling
- `pdf-lib-rs` — from crates.io

## Conventions

- Commit messages: imperative mood, concise summary line
- No `--release` for dev iteration
- Dual-licensed MIT OR Apache-2.0
