# Changelog

All notable changes to Textorium TUI are documented here.

## [Unreleased]

These changes are in source after the v1.0.2 release, not yet distributed by that release. This section does not assign a new release version.

### Features already implemented in April 2026

- Astro detection, content creation, dev server, and build support (#126)
- Site-local YAML post templates and CLI management (#127)
- Multi-site registry, switching, and legacy config migration (#128)
- Property filters in the TUI and repeatable CLI `--filter` (#129)
- Batch frontmatter operations with confirmation and batch revert (#130); these currently write immediately

### Improvements

- **Word-boundary content wrapping** — content pane now wraps at word boundaries instead of mid-word, with trimmed continuation lines for cleaner reading (#110)
- Atomic post writes, content dirty tracking, and reload of only the externally edited post, preserving other posts' unsaved changes

### Maintenance

- Declare this repository the sole maintained Rust TUI/Homebrew source; map all 15 private July synthetic core tests, port seven missing cases, and strengthen existing no-frontmatter coverage (#131)
- Reconcile contributor/source-build documentation, remove stale Notion-command claims, and correct the native companion link without pricing claims
- Record parser replacement as deliberately not transferred and safety/discovery/parity work as deferred in [the migration document](docs/canonical-source-migration.md); no production behavior, dependency, version, or release changes

## [1.0.2] — 2026-03-28

### Security

- Path traversal fix — `create_post()` validates category input to prevent writing outside content directory
- YAML injection fix — tag and category values quoted in frontmatter output

### Features

- Smart quotes (`Q` key) — curly quotes, em dashes, ellipses with code span awareness
- Help overlay (`?` key) — keybinding modal
- Save error details in status bar

### Performance

- Cached visual line count computation
- Direct `filtered_indices` lookups replacing `get_filtered_posts()` allocations
- Content pane scroll max uses visual wrapped line count

### Maintenance

- Removed tokio and ~150 transitive crates
- Extracted shared frontmatter-to-struct sync function
- Added 16 config.rs tests
- Test count: 47

## [1.0.1] — 2026-03-17

### Features

- TOML frontmatter support (Hugo `+++` delimiters)
- Batch save — Ctrl+S saves all unsaved posts
- Per-post revert (`u` key)
- Table scrolling via ratatui TableState

### Bug fixes

- Hugo preview URLs stripping content directory prefix
- Phantom `draft` and `content_type` fields injected on save
- Selection index not clamping after refresh
- Metadata cursor and content scroll not resetting on post switch
- `post.date` not syncing after editing date field
- Hugo content section path using category
- Char-boundary panics on multibyte titles
- Removed dead Notion config fields

### Performance

- Cached `get_filtered_posts()` with dirty flag

### Maintenance

- Removed 5 unused Cargo dependencies
- Symlink cycle protection (max depth 20)
- GitHub Actions workflow for Homebrew tap SHA256 updates

## [1.0.0] — 2026-03-16

### Features

- Full CLI: `new`, `list`, `publish`, `serve`, `build` commands
- Unsaved changes protection with dirty indicator and quit confirmation
- Search includes tags (title, content, categories, tags)
- Content wrapping and scrolling
- Light theme support
- Crash recovery (terminal restored on panic)

### Maintenance

- 12 tests, clippy clean, ~1.5MB binary

## [0.1.0] — 2026-02-06

- Initial release
- Three-pane TUI: posts table, metadata editor, content preview
- Hugo, Jekyll, Eleventy support with auto-detection
- Inline metadata editing
- Real-time search
- External editor integration ($EDITOR)
- Browser preview (`o` key)
