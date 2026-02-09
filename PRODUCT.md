# Textorium TUI product vision

## Problem

Static site generators store content as markdown files in nested directories. At scale (200+ posts), managing this content — finding posts, editing frontmatter, checking draft status, searching across content — means juggling `grep`, `find`, Finder, and text editors. There's no single tool that gives you a database-like view of your content while keeping files as the source of truth.

## Users

**Primary: prolific static site authors** — bloggers, documentation writers, and content-heavy site operators running Hugo, Jekyll, or Eleventy with 100-1000+ posts. They live in the terminal, value speed over GUI polish, and want keyboard-driven workflows that don't break their flow.

**Secondary: developers managing content sites** — engineers who maintain static sites as part of their work and want a fast way to audit, search, and batch-inspect content without building custom scripts.

## Vision

- Fastest way to browse and manage static site content — sub-second startup, zero-lag navigation at any scale
- Terminal-native: works over SSH, in tmux, alongside your editor — no browser, no Electron, no GUI required
- Files are the source of truth — no database, no sync, no import. Point at a directory and go
- Multi-SSG: Hugo, Jekyll, Eleventy out of the box, extensible to any markdown-based generator
- Complement to the Textorium macOS app — same philosophy, different interface for different workflows
- Eventually: the command-line content management layer that SSGs are missing

## Principles

1. **Speed is a feature.** Startup in milliseconds, not seconds. If the user notices a delay, it's a bug.
2. **Files are truth.** Never copy, cache, or abstract away from the actual markdown files. Read from disk, write to disk, nothing in between.
3. **Keyboard-first, mouse-never.** Every action reachable by keyboard. Navigation should feel like vim, not a web app.
4. **Safe by default.** Never destroy data. Block risky edits on complex fields. Require explicit save (Ctrl+S), not auto-save.
5. **SSG-aware, not SSG-specific.** Auto-detect the generator and adapt, but don't hard-code assumptions that break for other setups.
6. **Small and sharp.** A 1.5MB binary that does one thing well. No plugins, no config files beyond site path, no runtime dependencies.

## Success metrics

- Startup to interactive in <50ms on a 600+ post site
- Binary size under 2MB
- Zero data loss incidents from metadata editing
- At least one external user actively using it on their own site
- Author uses it as primary content management tool for daily blogging workflow
