# Textorium

Fast terminal interface for static site generators. Browse posts as a database, edit metadata inline, search content — all from your terminal.

Built with Rust for instant startup (~15ms) and zero-lag navigation, even with 600+ posts.

## Install

```bash
brew install pwelty/tap/textorium
```

Or build from source:

```bash
cargo install --path .
```

## Quick start

```bash
# Configure your site (first time only)
textorium use ~/Projects/my-blog

# Launch
textorium
```

Textorium auto-detects your SSG type (Hugo, Jekyll, Eleventy) and scans the appropriate content directories.

## Features

- Three-pane TUI: posts table, metadata editor, content preview
- Sortable columns (title, date, type, status)
- Real-time search across title, content, and categories
- Inline metadata editing with add/delete fields
- External editor integration (opens `$EDITOR`)
- Browser preview (auto-detects dev server URL)
- Save changes directly to markdown files
- Draft filter toggle

## Keyboard shortcuts

**Navigation:**

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate (context-aware per pane) |
| `Tab` / `l` | Next pane |
| `Shift+Tab` / `h` | Previous pane |

**Actions:**

| Key | Action |
|-----|--------|
| `Enter` | Edit field / open editor / add field |
| `d` | Delete metadata field |
| `Ctrl+S` | Save to disk |
| `s` | Cycle sort mode |
| `f` | Toggle drafts filter |
| `/` | Search |
| `o` | Open in browser |
| `r` | Refresh posts |
| `q` | Quit |

## Supported SSGs

| SSG | Detection | Dev server |
|-----|-----------|------------|
| Hugo | `content/` directory | localhost:1313 |
| Jekyll | `_posts/` directory | localhost:4000 |
| Eleventy | `.eleventy.js` config | localhost:8080 |

Falls back to full directory scan for other SSGs.

## Performance

On a 621-post Hugo site:

| Metric | Result |
|--------|--------|
| Startup | ~15ms |
| Initial scan | ~120ms |
| Navigation | <1ms |
| Search | ~5ms |
| Binary size | ~1.5MB |

## GUI companion

Textorium also has a native Mac app with a table-based content browser, WYSIWYG editor, and visual metadata management. Free on the [App Store](https://apps.apple.com/app/textorium/id6740587828).

## License

MIT
