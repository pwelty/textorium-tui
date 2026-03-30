# Decisions

Architectural and implementation decisions that aren't obvious from the code.

## Atomic writes for save_post, not create_post

**Date:** 2026-03-30
**Context:** `save_post` overwrites existing user content. `create_post` creates new files.

**Decision:** Only `save_post` uses atomic write (temp file → fsync → rename). `create_post` uses plain `fs::write` since there's no pre-existing data at risk.

**Rationale:** Atomic writes add a small overhead (extra syscalls). Worth it for overwrites where a crash could destroy hours of writing. Not needed for new file creation where the worst case is a missing draft.
