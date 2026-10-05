# Textorium TUI

## Canonical source and operational memory

Read [MEMORY.md](MEMORY.md) before project work. This public repository (`pwelty/textorium-tui`) is the sole maintained Rust TUI source used by Homebrew. Build and contribute here; do not replace it with the obsolete private `pwelty/Textorium/tui-rust` implementation. The native SwiftUI app is independent and shares no implementation code.

[Canonical-source migration](docs/canonical-source-migration.md) records synthetic coverage, provenance, exclusions, and deferred work. Store durable shared project rules in MEMORY.md, not chat or tool-specific auto-memory. Do not record secrets, raw environment values, private prose, transient progress, or test-output dumps.

Distribution: `brew install pwelty/tap/textorium` (`pwelty/homebrew-tap`). Version 1.1.0 packages the accumulated features and safety/discovery pass. Verify published release assets and formula state separately; a source version alone is not publication.

## Architecture

```text
src/
├── main.rs              # Entry point
├── cli.rs               # Clap parsing and command execution
├── core/
│   ├── config.rs        # JSON site registry, editor preference, SSG detection
│   ├── posts.rs         # Markdown parsing, scanning, field sync, saves, smart quotes
│   ├── templates.rs     # Site-local YAML post templates
│   └── filters.rs       # Property filter parsing and evaluation
└── tui/app.rs           # Three-pane UI, input, state, batch operations
```

- No subcommand launches the synchronous ratatui/crossterm TUI. Comma opens configuration; edit/reload uses the preferred external editor and validates before replacing state. Run `tests/config_smoke.py` alongside the safety smoke using a disposable receipt directory.
- Config is `~/.config/textorium/config.json`; multi-site format supports an active site and editor preference, with legacy flat-config compatibility.
- Posts on disk are the source of truth; no database or persistent post cache. All metadata/body/batch edits are in memory until Ctrl+S. Refresh refuses dirty posts; batch undo stages field-only inverse changes and never writes disk.
- YAML frontmatter uses `serde_yaml`; TOML uses `toml`. Preserve current dependencies unless separately authorized.
- SSG marker detection priority: Hugo → Jekyll → Eleventy → Astro; unknown sites retain a Hugo-compatible type but fall back to site-root Markdown when `content` is absent. Hugo includes leaf `index.md` but not branch `_index.md`; Jekyll scans `_posts` and sibling `_drafts`; Eleventy uses `posts`/`src`/site root, Astro `src/content`. Exact package dependency hints supplement markers; do not execute generator config code.
- Default dev URLs use ports 1313 / 4000 / 8080 / 4321 respectively. Per-site `server_url` overrides the preview base; Hugo leaf bundles route to their directory. See README for pruning, optimistic save protection, and remaining permalink limitations.

## Current source capabilities

Three-pane posts table, metadata editor, and content preview; search across title/content/categories/tags; sorting, draft and property filters; YAML/TOML frontmatter; smart quotes; external editor; per-post revert; save-all; browser preview; templates; site switching; and batch frontmatter operations.

CLI commands: `use`, `new`, `list`, `publish`, `templates`, `sites`, `serve`, `build`. There is no `idea`/Notion command. Version 1.1.0 includes the Astro/templates/sites/filters/batch features that were not in v1.0.2.

## Build and verify

From an isolated checkout of **this** repository:

```bash
cargo fmt -- --check
cargo clippy --locked -- -D warnings
cargo test --locked
cargo build --locked
cargo build --release --locked
# Optional local install, only when authorized:
cargo install --path . --locked
```

Use disposable synthetic content for tests. No GUI launch or live-site mutation is needed for consolidation. Read the actual diff and preserve non-test production Rust and Cargo files for a coverage/docs-only change.

## Release process (separate authorization)

Follow [the release and Homebrew handoff](docs/releases.md) for publication gates, actual-artifact verification, partial-failure recovery, and cleanup. Tagging automatically publishes assets **and updates the Homebrew tap**.

1. Qualify the intended source and update its version/changelog.
2. Commit and push the reviewed source.
3. Push a `vX.Y.Z` tag only with release authority.
4. GitHub Actions builds arm64/x86_64 macOS binaries, publishes release tarballs, and updates the Homebrew tap hashes.
5. Users consume the released version via `brew upgrade textorium`.

A source/docs PR is not a release; do not trigger tag/formula/website changes as a consolidation side effect. Skopos owns review/merge and private-source retirement for issue #131.

## Commit messages

Title-cased category prefixes: `Feature:`, `Fix:`, `Docs:`, `Refactor:`, `Chore:` with a brief sentence-case description.

## Related

- [Textorium native app](https://apps.apple.com/us/app/textorium/id6756587260) — independent SwiftUI companion; private repository `pwelty/Textorium`.
- [Homebrew tap](https://github.com/pwelty/homebrew-tap)
- [Website](https://textorium.app)
