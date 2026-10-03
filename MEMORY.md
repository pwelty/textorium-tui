# Project memory

## Canonical source

- Maintain, test, and build the Rust TUI only in `pwelty/textorium-tui`, the Homebrew source repository. The obsolete private `pwelty/Textorium/tui-rust` copy is reference history, not an implementation to synchronize back into this tree.
- Native SwiftUI work remains independent. Common on-disk content semantics are the interoperability boundary; there is no shared Rust/Swift implementation.
- [Canonical-source migration](docs/canonical-source-migration.md) records all 15 private July synthetic test mappings, provenance, exclusions, and deferred safety work. Keep dependencies/parser maintenance separate from source consolidation.
- Source capabilities and published binaries are distinct. Documentation of newer source must not imply that Homebrew v1.0.2 includes it; a source build can still print the unchanged package version.
- Preserve ordinary checkouts and private context. Use isolated checkouts and synthetic fixtures; do not publish secrets, personal prose, or machine-local memory. Release, formula updates, private retirement, and merge require their own authority.
