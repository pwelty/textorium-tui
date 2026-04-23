# Textorium TUI - Claude Development Guide

## Project Overview

Textorium TUI is a fast terminal interface for static site generators (Hugo, Jekyll, Eleventy). Built in Rust with ratatui for instant startup (~15ms) and zero-lag navigation on sites with 600+ posts.

**This is the public, standalone repo.** The macOS GUI app lives in a separate private monorepo (`pwelty/Textorium`). They share no code — the TUI is pure Rust, the GUI is pure SwiftUI.

**Distribution:** `brew install pwelty/tap/textorium` (Homebrew tap at `pwelty/homebrew-tap`)

## Architecture

```
src/
├── main.rs              # Entry point (11 lines)
├── cli.rs               # CLI argument parsing + subcommand execution (341 lines)
├── core/
│   ├── mod.rs           # Module declarations (2 lines)
│   ├── config.rs        # Site config management, SSG detection (341 lines)
│   └── posts.rs         # Markdown parsing, frontmatter extraction, file scanning (1338 lines)
├── tui/
│   ├── mod.rs           # Module declarations (1 line)
│   └── app.rs           # Main TUI application, all UI logic (1249 lines)
```

**Total:** ~3,283 lines of Rust (including ~1,400 lines of tests)

### Key components

**`core/config.rs`** — Manages site configuration stored in `~/.config/textorium/config.json`. Auto-detects SSG type from directory structure and config files. Stores site path, detected type, and dev server URL.

**`core/posts.rs`** — Scans content directories for `.md` files. Parses YAML frontmatter via serde_yaml. Extracts title, date, draft status, tags, categories. SSG-aware: Hugo scans `content/`, Jekyll scans `_posts/` + `_drafts/`, Eleventy scans everything.

**`tui/app.rs`** — The main TUI. Three panes: posts table (left), metadata editor (top-right), content preview (bottom-right). Handles all keyboard input, pane focus, sorting, filtering, search, inline editing, and file saving. This is the largest file and where most feature work happens.

**`cli.rs`** — Clap-based CLI with subcommands: `use`, `new`, `list`, `publish`, `idea`, `serve`, `build`. All subcommands are fully implemented.

### Data flow

1. `main.rs` → parses CLI args via `cli.rs`
2. If no subcommand → launches TUI
3. TUI loads config from `~/.config/textorium/config.json`
4. Scans site directory via `posts.rs` (SSG-aware scanning)
5. Renders three-pane UI via ratatui
6. User edits → modifies in-memory post data
7. Ctrl+S → writes changes back to markdown files on disk

## Conventions

### Rust patterns

- **Error handling:** `anyhow::Result` for application errors, `thiserror` for typed errors
- **Async:** No async runtime — the app is fully synchronous
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

## Features

**TUI (default, no subcommand):**
- Three panes: posts table (left), metadata editor (top-right), content preview (bottom-right)
- Real-time search (title, content, categories)
- Inline metadata editing (add, edit, delete fields)
- Smart quotes conversion (`Q` key — curly quotes, em dashes, ellipses)
- External editor integration ($EDITOR)
- Per-post revert (`u` key)
- Save all unsaved posts (Ctrl+S)
- Browser preview (`o` key)
- SSG detection and config management
- TOML and YAML frontmatter support

**CLI subcommands (all implemented):**
- `textorium use <path>` — set active site directory
- `textorium new "Title"` — create new post
- `textorium list` — list posts
- `textorium publish` — publish draft
- `textorium idea` — capture to Notion
- `textorium serve` — start dev server
- `textorium build` — build site

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

## Fleet chatter (always on)

You receive inbound messages via Discord MCP (#breakroom is the fleet channel, chat_id `1490801904132034720`) even when you're not running a task. Don't ignore them.

- Respond to anything addressed to you — by name or @-mention — even if idle.
- **Jump into live conversations you have something to add to.** You do not need to be @-mentioned to participate. If the room is discussing something your repo touches, your users care about, or your register has an opinion on — speak up. You are a coworker here, not a help desk waiting on tickets. Shy = absent, and absent is worse than wrong.
- Post new things if you want: observations, ideas, questions, warnings, banter.
- Use your own voice (the one described elsewhere in this file). Not generic helpful-bot register.
- Be collegial, not standoffish. This is a room, not a ticketing system.
- **75/25 rule.** Breakroom is roughly 75% work (status, patterns, questions, blockers) and 25% improv (banter, asides, specific-weird observations, being a person). Do not hit the 25% by performing jokes — hit it by noticing when something in the room is actually funny, specific, or human, and saying so.
- Short is fine. Humor at situations, never at people.
- Voice and conversation norms: see `paulos/skills/breakroom/conversational.md`.
- **Open threads go to the room, not to silence.** If stuck, unclear on scope, or waiting on a decision, post to #breakroom instead of sitting idle. Per Paul: "if something is open, bring it to the breakroom and we'll discuss."

