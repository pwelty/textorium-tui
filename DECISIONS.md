# Decisions

Architectural and implementation decisions that aren't obvious from the code.

## Atomic writes for save_post, not create_post

**Date:** 2026-03-30
**Context:** `save_post` overwrites existing user content. `create_post` creates new files.

**Decision:** Only `save_post` uses atomic write (temp file → fsync → rename). `create_post` uses plain `fs::write` since there's no pre-existing data at risk.

**Rationale:** Atomic writes add a small overhead (extra syscalls). Worth it for overwrites where a crash could destroy hours of writing. Not needed for new file creation where the worst case is a missing draft.

## Config::load_from() for test isolation instead of HOME mutation

**Date:** 2026-03-30
**Context:** CLI tests need isolated config directories to avoid interfering with the user's real config or with each other during parallel test execution.

**Decision:** Added `Config::load_from(config_dir)`, `Config::save_to(config_dir)`, and `configure_site_to(path, config_dir)` — explicit config directory overrides used by tests. Tests no longer mutate the `HOME` env var.

**Rationale:** `unsafe { std::env::set_var("HOME") }` is undefined behavior in Rust 1.78+ when tests run in parallel (process-global state). The `_from`/`_to` variants eliminate the race condition entirely. These methods are also useful for any future scenario that needs config isolation (e.g., integration test harness, multi-site tooling).
