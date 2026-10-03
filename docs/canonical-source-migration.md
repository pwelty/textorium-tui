# Canonical TUI source migration

## Source authority and provenance

[pwelty/textorium-tui](https://github.com/pwelty/textorium-tui) is the sole maintained Rust TUI source and the source repository for Homebrew distribution. Build and contribute here, not in the obsolete `pwelty/Textorium/tui-rust` copy. The native SwiftUI app is independently maintained; the two applications do not share implementation code.

This consolidation follows [issue #131](https://github.com/pwelty/textorium-tui/issues/131), starting from public main `5e887151e92579dc1b865801e0fd3f4d832d6cc6`. The reference-only private snapshot is `pwelty/Textorium` main `41cb71d718e7b8f84f4244f209b0496b991efb2d`. Its July synthetic test commit is `8144ec604d330a5f0481cb3106ffccfc0fa0db19`, affecting `tui-rust/src/core/{config,posts}.rs`. Only the authorized synthetic test coverage is transferred, not private history, credentials, local memory, personal prose, or obsolete CLI stubs.

The public implementation already includes the newer March/April improvements: atomic post writes, content dirty tracking, single-post reload after the external editor, Astro support, templates, multiple sites, property filters, and batch frontmatter operations. The older private implementation must not overwrite them.

## Complete July test mapping

All 15 July cases are accounted for below. Public destinations are in `src/core/config.rs` or `src/core/posts.rs`, within their existing `tests` modules. Seven missing cases are ported; eight are reused, including one strengthened assertion. Existing TOML, template, filter, CLI, and TUI tests remain intact.

| Private July test | Public destination (module / test) | Disposition |
|---|---|---|
| `detect_ssg_hugo_via_hugo_toml` | config / `test_detect_ssg_hugo_toml` | Existing marker coverage |
| `detect_ssg_jekyll_via_config_yml` | config / `test_detect_ssg_jekyll` | Existing marker coverage |
| `detect_ssg_eleventy_via_eleventy_js` | config / `test_detect_ssg_eleventy` | Existing marker coverage |
| `detect_ssg_defaults_to_hugo_when_no_markers` | config / `test_detect_ssg_empty_defaults_hugo` | Existing default coverage |
| `detect_ssg_hugo_takes_precedence_over_jekyll` | config / `test_detect_ssg_hugo_wins_priority` | Existing simultaneous-marker coverage |
| `detect_content_dir_maps_each_ssg_to_its_default` | config / `test_detect_content_dir_hugo`, `test_detect_content_dir_jekyll`, `test_detect_content_dir_eleventy_default` | Existing split coverage of all three defaults |
| `detect_content_dir_eleventy_prefers_src_when_present` | config / `test_detect_content_dir_eleventy_with_src` | Existing `src` fallback with no `posts` directory |
| `parse_frontmatter_well_formed` | posts / `parse_frontmatter_well_formed` | Ported: title, bool, tags, numeric custom field, body |
| `parse_frontmatter_no_leading_delimiter_returns_whole_content` | posts / `test_no_frontmatter_still_works` | Reused through `read_post`; strengthened to exact full-body equality, including trailing newline |
| `parse_frontmatter_single_delimiter_is_not_frontmatter` | posts / `parse_frontmatter_single_delimiter_is_not_frontmatter` | Ported: incomplete fence returns empty metadata and whole content |
| `parse_frontmatter_empty_body` | posts / `parse_frontmatter_empty_body` | Ported: metadata-only input |
| `read_post_parses_rfc3339_date` | posts / `read_post_parses_rfc3339_date` | Ported: exact UTC timestamp |
| `read_post_parses_iso_date` | posts / `read_post_parses_iso_date` | Ported: exact midnight UTC date |
| `read_post_invalid_date_is_none` | posts / `read_post_invalid_date_is_none` | Ported: invalid date has no parsed value |
| `save_then_read_roundtrips_without_corruption` | posts / `save_then_read_roundtrips_without_corruption` | Ported: title, draft, tags, date, body, flattened custom `weight` survive read/save/read |

API adaptations are limited to ignoring the public parser's additional raw-frontmatter/format return values and reusing the public `NamedTempFile` helper instead of a private directory/file helper. The no-leading-delimiter case exercises the parser through the public reader. Round-trip coverage establishes those synthetic field semantics, **not** arbitrary file-byte fidelity, editor conflict safety, or UI acceptance.

## Deliberately not transferred

Private commit `ec988cf51bf702e34de6208b08ab9724c5ee8777` replaced `serde_yaml` with `serde_yml`. That dependency replacement is deliberately **not adopted**: parser maintenance is a separate decision requiring current evaluation. The old revision remains recoverable in private Git history; it is not lost. Production Rust, `Cargo.toml`, and `Cargo.lock` remain unchanged in this consolidation.

Private-repository retirement and its documentation/automation changes belong to Skopos, not this worker. The public change does not delete the private tree or alter native Swift/website files.

## Deferred safety and parity work

The earlier source audit identified follow-ups, not repairs delivered by this migration:

- Refresh can discard unsaved edits ([#125](https://github.com/pwelty/textorium-tui/issues/125)).
- Frontmatter parsing trims bodies and saving reconstructs whitespace; metadata-only edits are not generally byte-preserving.
- Atomic writes do not detect external changes since load.
- Batch operations write immediately; undo/partial failures and preexisting unsaved/external edits need a defined safety contract.
- Smart quotes need stronger fenced-code and multi-backtick protection.
- Discovery needs explicit decisions/fixtures for Hugo leaf bundles, Jekyll drafts, Astro markers, unknown sites, and Eleventy roots; preview routing also needs edge-case coverage.
- Saved views and cross-app template interoperability remain separate features. TUI site-local YAML templates are not native app template storage.

## Verification and release boundary

Run from this repository in an isolated checkout, with disposable synthetic files only:

```bash
cargo fmt -- --check
cargo clippy --locked -- -D warnings
cargo test --locked
cargo build --locked
```

Review the complete diff and verify non-test Rust and dependency files against the pinned public base. The imported tests should pass on unchanged production behavior; this is coverage consolidation, not a bug fix. A disposable negative-control mutation can prove the custom-field tests detect dropped flattened metadata, but no such mutation belongs in the published branch.

At consolidation time, the latest published release and Homebrew formula are v1.0.2 (2026-03-28); newer source features are not thereby released. `Cargo.toml` still reports 1.0.2 even when building newer source. No version bump, tag, release, formula mutation, website deployment, GUI launch, or real-site mutation is part of this work. Skopos owns review, merge, private retirement, and any later release decision.
