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
# Point textorium at your site (first time only)
textorium use ~/Projects/my-blog

# Launch the TUI
textorium
```

Textorium auto-detects your SSG type (Hugo, Jekyll, Eleventy) and scans the appropriate content directories.

## TUI

Three-pane layout: posts table (left), metadata editor (top-right), content preview (bottom-right).

- Sortable columns (title, date, type, status)
- Real-time search across title, content, categories, and tags
- Inline metadata editing — add, edit, and delete frontmatter fields
- Unsaved changes indicator and quit confirmation
- External editor integration (`$EDITOR`)
- Browser preview (auto-detects dev server URL)
- Draft filter toggle
- Save changes directly to markdown files (`Ctrl+S`)

### Keyboard shortcuts

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
| `q` / `Ctrl+C` | Quit (confirms if unsaved changes) |

## CLI commands

```bash
# Create a new post
textorium new "My post title" --category blog --tags "rust,tui"

# List posts (table or JSON)
textorium list
textorium list --drafts --json

# Publish a draft
textorium publish my-post-slug

# Start dev server (SSG-aware, includes drafts by default)
textorium serve
textorium serve --port 3000 --no-drafts

# Build for production
textorium build
textorium build --minify  # Hugo only
```

## Supported SSGs

| SSG | Detection | Content directory | Default dev port |
|-----|-----------|-------------------|-----------------|
| Hugo | `content/` directory | `content/` | 1313 |
| Jekyll | `_posts/` directory | `_posts/` + `_drafts/` | 4000 |
| Eleventy | `.eleventy.js` config | Full directory scan | 8080 |

## Performance

On a 621-post Hugo site:

| Metric | Result |
|--------|-------:|
| Startup | ~15ms |
| Initial scan | ~120ms |
| Navigation | <1ms |
| Search | ~5ms |
| Binary size | ~1.5MB |

## GUI companion

Textorium also has a native Mac app with a table-based content browser, WYSIWYG editor, and visual metadata management. Free on the [App Store](https://apps.apple.com/app/textorium/id6740587828).

## Author

Built by [Paul Welty](https://paulwelty.com).

## License

MIT
