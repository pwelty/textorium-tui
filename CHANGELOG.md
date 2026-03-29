# Changelog

All notable changes to Textorium TUI are documented here.

## [Unreleased] — v1.0.3

### Improvements

- **Word-boundary content wrapping** — content pane now wraps at word boundaries instead of mid-word, with trimmed continuation lines for cleaner reading (#110)

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
