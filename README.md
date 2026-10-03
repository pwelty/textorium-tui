# Textorium

Fast terminal interface for static site generators. Browse posts as a database, edit metadata inline, search content — all from your terminal.

Built with Rust for instant startup (~15ms) and zero-lag navigation, even with 600+ posts.

**Canonical source:** this repository is the sole maintained Rust TUI source used by Homebrew. Build and contribute here, not in the obsolete private `Textorium/tui-rust` copy. See the [coverage and migration record](docs/canonical-source-migration.md).

![Textorium demo](demo.gif)

## Install

```bash
brew install pwelty/tap/textorium
```

The latest published release/Homebrew formula is **v1.0.2 (2026-03-28)**. This README describes current source; the March release does not include the later Astro/templates/multi-site/property-filter/batch features. This consolidation does not publish a new release. Source builds still report the unchanged Cargo package version, 1.0.2.

Build newer source from this repository:

```bash
git clone https://github.com/pwelty/textorium-tui.git
cd textorium-tui
cargo install --path . --locked
```

For development checks, use `cargo fmt -- --check`, `cargo clippy --locked -- -D warnings`, `cargo test --locked`, and `cargo build --locked`.

## Quick start

```bash
# Point textorium at your site (first time only)
textorium use ~/Projects/my-blog

# Launch the TUI
textorium
```

Textorium auto-detects your SSG type (Hugo, Jekyll, Eleventy, Astro) and scans its configured content directory.

## TUI

Three-pane layout: posts table (left), metadata editor (top-right), content preview (bottom-right).

- Sortable columns (title, date, type, status)
- Real-time search across title, content, categories, and tags
- Inline metadata editing — add, edit, and delete frontmatter fields
- Smart quotes conversion — curly quotes, em dashes, ellipses
- Per-post revert for unsaved changes
- YAML and TOML frontmatter support
- Unsaved changes indicator and quit confirmation
- External editor integration (`$EDITOR`)
- Browser preview (auto-detects dev server URL)
- Draft filter toggle
- Save changes directly to markdown files (`Ctrl+S`)
- Site switching and site-local YAML post templates
- Property filters (AND logic) and batch frontmatter operations with confirmation/revert

Batch operations currently write immediately, unlike ordinary unsaved metadata edits. Refresh, arbitrary body-byte fidelity, external-edit conflict checks, and batch/undo safety remain [follow-up work](docs/canonical-source-migration.md#deferred-safety-and-parity-work), not repairs delivered by consolidation. Use disposable copies or ordinary Git backups when exploring these paths.

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
| `u` | Revert last batch, or current post when no batch revert is pending |
| `s` | Cycle sort mode |
| `f` | Toggle drafts filter |
| `/` | Search |
| `o` | Open in browser |
| `Q` | Smart quotes (curly quotes, em dashes, ellipses) |
| `r` | Refresh posts |
| `?` | Help overlay |
| `q` / `Ctrl+C` | Quit (confirms if unsaved changes) |
| `S` | Switch registered site |
| `n` | New post / template picker |
| `F` / `x` | Build property filter / clear property filters |
| `Space` / `Ctrl+A` / `b` | Toggle marked post (posts pane) / toggle all filtered marks / batch operations |

## CLI commands

```bash
# Create a new post
textorium new "My post title" --category blog --tags "rust,tui"

# List posts (table or JSON)
textorium list
textorium list --drafts --json
textorium list --filter "draft:is_true" --filter "tags:contains:rust"

# Publish a draft
textorium publish my-post-slug

# Start dev server (SSG-aware, includes drafts by default)
textorium serve
textorium serve --port 3000 --no-drafts

# Build for production
textorium build
textorium build --minify

# Site-local YAML templates (<site>/.textorium/templates/)
textorium templates create article
textorium templates list
textorium new "Templated post" --template article --no-edit

# Register and switch sites (source features after v1.0.2)
textorium sites add ~/Projects/my-blog --name blog
textorium sites list
textorium sites use blog
textorium sites remove old-site  # Cannot remove the active site
```

## Supported SSGs

| SSG | Detection | Content directory | Frontmatter | Default dev port |
|-----|-----------|-------------------|-------------|-----------------|
| Hugo | `hugo.toml`, `hugo.yaml`, `config.toml` | `content/` | YAML, TOML | 1313 |
| Jekyll | `_config.yml` | `_posts/` | YAML | 4000 |
| Eleventy | `.eleventy.js`, `eleventy.config.js` | `posts/`, otherwise `src/` if present, otherwise `posts/` | YAML | 8080 |
| Astro | `astro.config.mjs`, `astro.config.ts` | `src/content/` | YAML | 4321 |

Detection priority is Hugo → Jekyll → Eleventy → Astro, defaulting to Hugo when no marker matches. The scanner recursively reads Markdown in the configured content root; Hugo currently skips both `_index.md` and `index.md`. It does not automatically include Jekyll `_drafts/`. See the migration follow-ups for discovery and preview limitations.

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

Textorium also has an independent native Mac app with a table-based content browser, WYSIWYG editor, and visual metadata management. See the [App Store](https://apps.apple.com/us/app/textorium/id6756587260) or [textorium.app](https://textorium.app). The SwiftUI app and Rust TUI share no implementation code.

## Author

Built by [Paul Welty](https://paulwelty.com).

## License

MIT
