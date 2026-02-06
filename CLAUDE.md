# Textorium TUI - Claude Development Guide

## Project Overview

Textorium TUI is a fast terminal interface for static site generators (Hugo, Jekyll, Eleventy). Built in Rust with ratatui for instant startup (~15ms) and zero-lag navigation on sites with 600+ posts.

**This is the public, standalone repo.** The macOS GUI app lives in a separate private monorepo (`pwelty/Textorium`). They share no code — the TUI is pure Rust, the GUI is pure SwiftUI.

**Distribution:** `brew install pwelty/tap/textorium` (Homebrew tap at `pwelty/homebrew-tap`)

## Architecture

```
src/
├── main.rs              # Entry point (13 lines)
├── cli.rs               # CLI argument parsing via clap (134 lines)
├── core/
│   ├── config.rs        # Site config management, SSG detection (196 lines)
│   └── posts.rs         # Markdown parsing, frontmatter extraction, file scanning (224 lines)
├── tui/
│   └── app.rs           # Main TUI application, all UI logic (745 lines)
└── widgets/
    └── mod.rs           # Custom widget stubs (1 line)
```

**Total:** ~1,165 lines of Rust

### Key components

**`core/config.rs`** — Manages site configuration stored in `~/.config/textorium/config.toml`. Auto-detects SSG type from directory structure and config files. Stores site path, detected type, and dev server URL.

**`core/posts.rs`** — Scans content directories for `.md` files. Parses YAML frontmatter via serde_yaml. Extracts title, date, draft status, tags, categories. SSG-aware: Hugo scans `content/`, Jekyll scans `_posts/` + `_drafts/`, Eleventy scans everything.

**`tui/app.rs`** — The main TUI. Three panes: posts table (left), metadata editor (top-right), content preview (bottom-right). Handles all keyboard input, pane focus, sorting, filtering, search, inline editing, and file saving. This is the largest file and where most feature work happens.

**`cli.rs`** — Clap-based CLI with subcommands: `use`, `new`, `list`, `publish`, `idea`, `serve`, `build`. Currently only `use` is implemented; others are stubbed.

### Data flow

1. `main.rs` → parses CLI args via `cli.rs`
2. If no subcommand → launches TUI
3. TUI loads config from `~/.config/textorium/config.toml`
4. Scans site directory via `posts.rs` (SSG-aware scanning)
5. Renders three-pane UI via ratatui
6. User edits → modifies in-memory post data
7. Ctrl+S → writes changes back to markdown files on disk

## Conventions

### Rust patterns

- **Error handling:** `anyhow::Result` for application errors, `thiserror` for typed errors
- **Async:** tokio runtime exists but not heavily used yet (mainly for future file watching)
- **Serialization:** serde + serde_yaml for frontmatter, toml for config
- **TUI:** ratatui 0.29 + crossterm 0.28 for terminal rendering and input

### File operations

Posts are the source of truth. The TUI reads from disk on startup and writes back on Ctrl+S. No database, no cache, no intermediate storage.

### SSG detection priority

Same as the GUI app: Hugo → Jekyll → Eleventy → full directory scan.

Dev server URLs:
- Hugo: `http://localhost:1313`
- Jekyll: `http://localhost:4000`
- Eleventy: `http://localhost:8080`

## Building

```bash
# Dev build
cargo build

# Release build (optimized, stripped, LTO)
cargo build --release

# Install locally
cargo install --path .
```

Release profile uses `opt-level = 3`, LTO, single codegen unit, and symbol stripping for minimal binary size (~1.5MB).

## Release process

1. Bump version in `Cargo.toml`
2. Commit and push to main
3. Tag: `git tag vX.Y.Z && git push origin vX.Y.Z`
4. GitHub Actions builds arm64 + x86_64 macOS binaries
5. Creates GitHub Release with tarballs
6. Update SHA256 hashes in `pwelty/homebrew-tap` formula
7. Users get update via `brew upgrade textorium`

## Commit message format

Use conventional commits with category prefixes:

```
Feature: Brief description in sentence case
Fix: Brief description in sentence case
Docs: Brief description in sentence case
Refactor: Brief description in sentence case
```

## What's implemented vs stubbed

**Working:**
- TUI with three panes, navigation, sorting, filtering
- Real-time search (title, content, categories)
- Inline metadata editing (add, edit, delete fields)
- External editor integration ($EDITOR)
- Save to disk (Ctrl+S)
- Browser preview (press `o`)
- SSG detection and config management
- `textorium use <path>` command

**Stubbed (CLI commands in cli.rs):**
- `textorium new "Title"` — create new post
- `textorium list` — list posts
- `textorium publish` — publish draft
- `textorium idea` — capture to Notion
- `textorium serve` — start dev server
- `textorium build` — build site

These have clap definitions but the match arms just print "not yet implemented."

## Related projects

- **Textorium macOS app:** Native SwiftUI GUI, App Store distribution. Private repo (`pwelty/Textorium`).
- **Homebrew tap:** `pwelty/homebrew-tap` — formula for `brew install pwelty/tap/textorium`
- **Website:** textorium.app — landing page hosted on Cloudflare Pages

## Agent usage

This project has access to custom agents via symlinked `.claude/agents/` directory. See the Textorium monorepo CLAUDE.md for full agent documentation.

## Using PaulOS CLI

```bash
# Distribute work log
paulos work-log distribute \
  --title "Work log: Textorium TUI - Month Day, Year" \
  --date "YYYY-MM-DDTHH:MM:SS-05:00" \
  --categories "work-log,textorium-tui" \
  --tags "development,rust,tui" \
  --content-file "work-log/YYYY-MM-DD.md"
```
