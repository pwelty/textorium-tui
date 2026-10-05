# Changelog

All notable changes to Textorium TUI are documented here.

## [Unreleased]

- Add comma configuration view with active site/path, editor and config location, scrollable setup guidance, and external edit/reload (#139).
- Resolve editors as config → VISUAL → EDITOR → nano; support literal quoted arguments without a shell and restore the TUI after failed launches/nonzero exits. Share the launcher with post editing and CLI new.
- Retain unsaved posts and previous working config on invalid reload; create only a path-free first-edit config scaffold, preserve externally edited JSON/unknown fields, and keep malformed startup config recoverable.
- Make configuration/help discoverable in narrow footers and distinguish valid empty collections from missing roots/content. CLI list now fails with stderr/no stdout for missing paths, including JSON mode.
- Add focused Rust and synthetic real PTY/CLI config regressions; no version or release change.

## [1.1.0] — 2026-10-04

First release of the accumulated source features and the bounded safety/discovery fixes since v1.0.2.

### Features

- Astro detection, content creation, dev server, and build support (#126)
- Site-local YAML post templates and CLI management (#127)
- Multi-site registry, switching, and legacy config migration (#128)
- Property filters in the TUI and repeatable CLI `--filter` (#129)
- Batch frontmatter operations with confirmation and batch revert (#130); staging/undo semantics are hardened below

### Safety and discovery (#133, #125)

- Refuse refresh while any post has unsaved metadata/body edits; guard collection-changing reloads and dirty selected-post editor handoffs.
- Preserve exact body bytes on metadata saves, make no-op saves nonwriting, keep plain body edits plain, and recognize only whole-line frontmatter delimiters. Retain absent/empty metadata fields rather than inventing or dropping them.
- Refuse changed/deleted/replaced/symlinked files using loaded bytes and Unix identity. Use exclusive unique temp files, preserve file permissions, and recheck before atomic replacement (optimistic, not transactional).
- Stage batch edits until Ctrl+S. Undo before/after save is in-memory and field-owned; preserve later edits/body and report partial save errors.
- Include Hugo leaf bundles and Jekyll drafts; extend Astro/Eleventy markers and exact package hints, define Markdown/Eleventy fallback roots, prune generated/dependency/Git trees and symlinks, fix Hugo leaf previews, and preserve optional per-site preview server bases.
- Protect fenced and multi-backtick inline code from smart quotes. Preserve code-significant indentation in the loaded YAML/TOML body and use the same blank-line boundaries on save. Recognize blockquote space/tab prefixes by Markdown columns without turning indented fence-like text into a fence. Add integrated read/transform/save regressions and a shared synthetic fixture set with real PTY/CLI smoke; preserve preexisting test coverage.

### Improvements

- **Word-boundary content wrapping** — content pane now wraps at word boundaries instead of mid-word, with trimmed continuation lines for cleaner reading (#110)
- Atomic post writes, content dirty tracking, and reload of only the externally edited post, preserving other posts' unsaved changes

### Maintenance

- Declare this repository the sole maintained Rust TUI/Homebrew source; map all 15 private July synthetic core tests, port seven missing cases, and strengthen existing no-frontmatter coverage (#131)
- Reconcile contributor/source-build documentation, remove stale Notion-command claims, and correct the native companion link without pricing claims
- Preserve the original consolidation scope and parser decision in [the migration document](docs/canonical-source-migration.md); this release also includes the subsequently completed safety/discovery pass.
- Build release binaries with the committed lockfile and fail asset downloads on HTTP errors before updating Homebrew.

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
