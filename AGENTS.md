# Textorium TUI

## Operational Memory

Read this repo's `MEMORY.md` at the start of project work. Also read `/Users/paul/Projects/MEMORY.md` when the task touches fleet-level conventions, dispatch, MCP setup, automation, or cross-project behavior.

Private context is not operational memory. Chat history is not operational memory. Tool-specific auto-memory is not shared operational memory. If future agents need a durable fact to work in this repo, write it to `MEMORY.md`.

Before final response, issue close, or handoff, ask: "Did I learn anything durable that future agents need?" If yes, append or update `MEMORY.md` first.

Do not store secrets, API keys, raw `.env` values, transient progress, issue status dumps, test output dumps, speculation, or facts easily derivable from current code.

Fast terminal interface for static site generators (Hugo, Jekyll, Eleventy). Built in Rust with ratatui for instant startup (~15ms) and zero-lag navigation on sites with 600+ posts.

**This is the public, standalone repo.** The macOS GUI app lives in a separate private repo (`pwelty/Textorium`). They share no code — the TUI is pure Rust, the GUI is pure SwiftUI.

**Distribution**: `brew install pwelty/tap/textorium` (Homebrew tap at `pwelty/homebrew-tap`)

## Architecture

```
src/
├── main.rs              # Entry point (11 lines)
├── cli.rs               # CLI argument parsing + subcommand execution (~340 lines)
├── core/
│   ├── mod.rs
│   ├── config.rs        # Site config, SSG detection (~340 lines)
│   └── posts.rs         # Markdown parsing, frontmatter, file scanning (~1340 lines)
└── tui/
    ├── mod.rs
    └── app.rs           # Main TUI application, all UI logic (~1250 lines)
```

~3,300 lines of Rust including ~1,400 lines of tests.

### Key components

- **`core/config.rs`** — site config in `~/.config/textorium/config.json`. Auto-detects SSG type. Stores site path, type, dev server URL.
- **`core/posts.rs`** — scans content dirs, parses YAML frontmatter via serde_yaml, extracts title/date/draft/tags/categories. SSG-aware: Hugo scans `content/`, Jekyll scans `_posts/` + `_drafts/`, Eleventy scans everything.
- **`tui/app.rs`** — three panes (posts table left, metadata top-right, content preview bottom-right). Handles all keyboard input, pane focus, sorting, filtering, search, inline editing, file saving. Largest file, where most feature work happens.
- **`cli.rs`** — Clap-based CLI: `use`, `new`, `list`, `publish`, `idea`, `serve`, `build`. All implemented.

### Data flow

1. `main.rs` → parses CLI args via `cli.rs`
2. No subcommand → launches TUI
3. TUI loads `~/.config/textorium/config.json`
4. Scans site directory via `posts.rs` (SSG-aware)
5. Renders three-pane UI via ratatui
6. User edits → modifies in-memory post data
7. Ctrl+S → writes back to markdown files

## Conventions

- **Error handling**: `anyhow::Result` for app errors, `thiserror` for typed errors
- **Async**: none — fully synchronous
- **Serialization**: serde + serde_yaml (frontmatter), toml (config)
- **TUI**: ratatui 0.29 + crossterm 0.28
- **Source of truth**: posts on disk. Read on startup, write on Ctrl+S. No database, no cache.

### SSG detection priority

Same as the GUI app: Hugo → Jekyll → Eleventy → full directory scan.

Dev server URLs:
- Hugo: `http://localhost:1313`
- Jekyll: `http://localhost:4000`
- Eleventy: `http://localhost:8080`

## Building

```bash
cargo build              # Dev
cargo build --release    # Release (opt-level=3, LTO, single codegen unit, stripped — ~1.5MB)
cargo install --path .   # Install locally
```

## Release process

1. Bump version in `Cargo.toml`
2. Commit and push to main
3. Tag: `git tag vX.Y.Z && git push origin vX.Y.Z`
4. GitHub Actions builds arm64 + x86_64 macOS binaries
5. Creates GitHub Release with tarballs
6. Update SHA256 hashes in `pwelty/homebrew-tap` formula
7. Users get update via `brew upgrade textorium`

## Commit messages

Conventional commits:

```
Feature: Brief description in sentence case
Fix: Brief description
Docs: Brief description
Refactor: Brief description
```

## Features

**TUI (default, no subcommand):**
- Three panes: posts table (left), metadata editor (top-right), content preview (bottom-right)
- Real-time search (title, content, categories)
- Inline metadata editing (add, edit, delete fields)
- Smart quotes conversion (`Q` — curly quotes, em dashes, ellipses)
- External editor integration (`$EDITOR`)
- Per-post revert (`u`)
- Save all unsaved (Ctrl+S)
- Browser preview (`o`)
- TOML and YAML frontmatter

**CLI subcommands:**
- `textorium use <path>` — set active site
- `textorium new "Title"` — create new post
- `textorium list` — list posts
- `textorium publish` — publish draft
- `textorium idea` — capture to Notion
- `textorium serve` — start dev server
- `textorium build` — build site

## Related

- **Textorium macOS app**: native SwiftUI GUI, App Store. Private repo `pwelty/Textorium`.
- **Homebrew tap**: `pwelty/homebrew-tap`
- **Website**: textorium.app — Cloudflare Pages
