# Textorium

Fast terminal interface for static site generators. Browse posts as a database, edit metadata inline, search content — all from your terminal.

Built with Rust for instant startup (~15ms) and zero-lag navigation, even with 600+ posts.

**Canonical source:** this repository is the sole maintained Rust TUI source used by Homebrew. Build and contribute here, not in the obsolete private `Textorium/tui-rust` copy. See the [coverage and migration record](docs/canonical-source-migration.md).

![Textorium demo](demo.gif)

## Install

```bash
brew install pwelty/tap/textorium
```

**Version 1.1.0** includes Astro, templates, multi-site management, property filters, staged batch edits, and the safety/discovery fixes described below. See [GitHub Releases](https://github.com/pwelty/textorium-tui/releases/latest) for published binaries. Existing Homebrew users can run `brew update && brew upgrade textorium`.

**Batch behavior:** edits and undo are staged in memory; press **Ctrl+S** to write them. Refresh refuses unsaved edits, and saves refuse files changed externally instead of overwriting them.

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
- Preferred external editor integration (`config.editor` → `VISUAL` → `EDITOR` → `nano`)
- In-app configuration view (`,`) with safe edit/reload
- Browser preview (auto-detects dev server URL)
- Draft filter toggle
- Save changes directly to markdown files (`Ctrl+S`)
- Site switching and site-local YAML post templates
- Property filters (AND logic) and batch frontmatter operations with confirmation/revert

### File safety (current source)

- All metadata, smart-quote, and batch changes are staged in memory until **Ctrl+S**. `r` refuses refresh while any post has unsaved metadata or body edits; repeated `r` or Esc never discards them. Save, or explicitly revert with `u`, before refreshing. Site switching/new-post reloads are likewise refused while dirty; an external editor requires the selected post to be clean.
- Metadata-only saves retain the exact body bytes, including blank lines, CRLF, code, and missing final newline. The content pane remains a trimmed display projection for frontmatter posts; body transformations retain its original leading/trailing whitespace boundaries. A no-op save does not rewrite the file or add absent fields. Plain Markdown stays plain for body-only edits; adding metadata explicitly creates YAML frontmatter.
- Saves compare current bytes and (on Unix) device/inode identity with the loaded baseline. Changed, deleted, replaced, or symlinked targets are refused visibly, retaining in-memory edits. Writes use unique exclusive temporary files, fsynced content, atomic rename, and current file permissions. This is **optimistic protection**, not a cross-process filesystem transaction: another writer can still race the final check/rename. Ownership, ACLs, extended attributes, and directory-fsync durability are not promised. Backups/version control remain appropriate.
- `u` undoes the **last changing batch** in memory, before or after save. It restores only fields still equal to that batch's result; later edits to the same field, other fields, and body are retained. Undo after save stages an inverse edit requiring Ctrl+S. An unchanged batch preserves the previous undo. Save-all reports partial success/errors individually; failed posts stay dirty, and undo never writes external bytes. A clean refresh/site switch clears old batch undo. A newer changing batch replaces the single undo slot.
- Smart quotes protect backtick/tilde fenced code (including unclosed fences) and matched equal-length multi-backtick inline spans. Normal prose still gets curly quotes, em dashes, and ellipses.
- A frontmatter edit whose reconstructed metadata does not round-trip safely is refused rather than written. Complex formatting/permalink semantics are not a generic Markdown round-trip guarantee.

Synthetic verification: `cargo test --locked` and the stdlib-only real PTY/CLI smoke in [tests/safety_smoke.py](tests/safety_smoke.py). Run `python3 tests/safety_smoke.py /absolute/path/to/textorium /new/receipt-directory`; it isolates HOME/config, creates disposable sites, and retains raw ANSI, cell projections, file readbacks, child exits, and a JSON summary. It never uses real articles or launches a browser/editor.

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
| `u` | Undo last changing batch in memory, or revert current post when no batch undo is pending |
| `s` | Cycle sort mode |
| `f` | Toggle drafts filter |
| `/` | Search |
| `o` | Open in browser |
| `Q` | Smart quotes (curly quotes, em dashes, ellipses) |
| `r` | Refresh posts |
| `,` | Configuration: `e`/Enter edits, `r` reloads, `j`/`k` scroll, Esc closes |
| `?` | Help overlay |
| `q` / `Ctrl+C` | Quit (confirms if unsaved changes) |
| `S` | Switch registered site |
| `n` | New post / template picker |
| `F` / `x` | Build property filter / clear property filters |
| `Space` / `Ctrl+A` / `b` | Toggle marked post (posts pane) / toggle all filtered marks / batch operations |

### Configuration and empty-state recovery (current source; unreleased)

Press **`,`** from any pane to see the active site/root, content directory,
preferred editor, and `~/.config/textorium/config.json`. The footer advertises
comma/help even in narrow terminals. The view scrolls and keeps its edit/close
controls visible. Press **`e`** (or Enter) to edit the actual JSON file; **`r`**
validates and reloads an independently edited file. Esc closes the view.

On first edit, an absent file gets a legacy-format scaffold with an **empty
`site_path`**: enter an existing site root and its relative `content_dir`.
Textorium does not guess paths or create site/content directories. Both legacy
flat and multi-site JSON are accepted. Malformed startup config remains editable.
Invalid JSON, missing roots/content directories, or an unmatched `active_site`
are reported; reload retains the previous working collection. Save or revert
all unsaved post edits before configuration edit/reload. External edits are
written by your editor, not by Ctrl+S; no JSON reserialization occurs, so unknown
fields remain intact. A failed/nonzero editor does not trigger reload, even if
it already wrote the file; re-edit or explicitly reload when ready.

Editor precedence is a nonblank `editor` in config, then `VISUAL`, then `EDITOR`,
then `nano`. Commands accept literal quoted arguments and backslash escapes,
**not shell expansion** (`~`, `$HOME`, pipes, or substitutions). Terminal editors
use the same terminal. Desktop editors must be invoked with their explicit wait
option, for example `"editor": "code --wait"` or
`"editor": "open -W -a TextEdit"` on macOS. Close/return from the editor to reload.
A path containing spaces can be quoted inside JSON, e.g.
`"editor": "\"/path/to/editor with spaces\" --wait"`.

Unconfigured/missing-site/missing-content states differ from a valid empty site
and from search/draft/property filters with no matches. `textorium list` and
`list --json` return nonzero with a diagnostic on stderr and **no stdout** for
invalid JSON or unconfigured/missing roots. A valid empty collection (or filters
with no matches) returns exit 0; `--json` emits `[]`. Individual post parse errors
retain the existing skip-with-stderr-warning behavior.

Run the additional stdlib-only synthetic acceptance suite:
`python3 tests/config_smoke.py /absolute/path/to/textorium /new/receipt-directory`.
It exercises real PTY comma/edit/reload, first-run/stale/malformed configs,
preferred-editor argv, failure restoration, dirty guards, empty states, and
40×12 layout. It uses a synthetic blocking editor, never a GUI or real content.

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

# Register and switch sites
textorium sites add ~/Projects/my-blog --name blog
textorium sites list
textorium sites use blog
textorium sites remove old-site  # Cannot remove the active site
```

## Supported SSGs

| SSG | Detection | Content directory | Frontmatter | Default dev port |
|-----|-----------|-------------------|-------------|-----------------|
| Hugo | `hugo.toml`, `hugo.yaml`, `config.toml` | `content/` | YAML, TOML | 1313 |
| Jekyll | `_config.yml` | `_posts/` **and** sibling `_drafts/` | YAML | 4000 |
| Eleventy | `.eleventy.js/.cjs`, `eleventy.config.js/.cjs/.mjs`, or exact `@11ty/eleventy` package dependency | `posts/`, otherwise `src/`, otherwise site root | YAML | 8080 |
| Astro | `astro.config.mjs/.ts/.js/.cjs`, or exact `astro` package dependency | `src/content/` | YAML | 4321 |

Explicit marker priority is Hugo → Jekyll → Eleventy → Astro; exact dependency/devDependency keys are consulted only when no marker matches. Unknown sites retain a Hugo-compatible registry type and use `content/` if present, otherwise the site root for Markdown fallback. The scanner includes Hugo leaf-bundle `index.md`, excluding branch `_index.md`; Jekyll's standard `_posts` root also includes `_drafts`. It never follows symlinks and prunes `.git`, `node_modules`, `vendor`, `target`, `dist`, `public`, `_site`, `_output`, `.next`, `.astro`, and `.textorium` directories at any depth. Eleventy config JavaScript is **not executed** to discover arbitrary custom input/output paths; set the site's `content_dir` in the JSON registry for nonconventional roots (standard ignored directory names remain pruned).

Hugo leaf preview URLs point at the bundle directory rather than `/index/`. An optional per-site `server_url` in `~/.config/textorium/config.json` overrides the default preview base, e.g. `"server_url": "http://localhost:9123/custom/"`. This is a preview setting, not automatic dev-server port discovery. Other previews remain content-relative approximations: custom generator permalinks and Jekyll date/category routing are not inferred.

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
