# TUI release and Homebrew handoff

This repository is the canonical Rust TUI source. Native Textorium App Store releases are separate; do not align their version numbers or touch their submission state as a TUI release step.

## Prepare

1. Obtain release authority and use an isolated checkout. Preserve ordinary dirty checkouts, installed tools, and real writing/configuration.
2. Review the complete intended source, including any known failing regressions. Exercise preservation claims through load → UI edit/transform → save → reload, not only helper tests.
3. Update `Cargo.toml`, the `textorium` package entry in `Cargo.lock`, `CHANGELOG.md`, and release-facing documentation. Do not change dependency versions incidentally.
4. Run formatting, locked clippy/tests/build, and the real terminal/CLI smoke on disposable content:

   ```sh
   cargo fmt -- --check
   cargo clippy --locked --all-targets -- -D warnings
   cargo test --locked
   cargo build --release --locked
   ./target/release/textorium --version
   python3 tests/safety_smoke.py "$PWD/target/release/textorium" /absolute/new-receipt-directory
   ```

   The receipt directory must not exist. The smoke harness uses isolated HOME/config and synthetic sites. Retain receipts privately, not in the source repository.
5. Commit, push, review, and merge the release-preparation PR. Check exact-main CI and confirm its tree matches the qualified candidate before tagging.

## Publish

Pushing `vX.Y.Z` triggers `.github/workflows/release.yml`. It:

- builds locked Apple Silicon and Intel macOS binaries;
- publishes both tarballs as GitHub Release assets;
- downloads those assets, computes SHA256, and **automatically updates** `pwelty/homebrew-tap/Formula/textorium.rb` using `HOMEBREW_TAP_TOKEN`.

Tagging is therefore authority to publish both binaries and the Homebrew update—not merely to start a build. Inspect the workflow and tap credential readiness before tagging. Never run a competing formula updater while that job is active.

## Verify distribution

- Read back the annotated tag's commit, release state, and both assets.
- Download actual published tarballs. Compare SHA256 against asset digests and the final formula; inspect archive layout and Mach-O architecture.
- Run `--version` and `tests/safety_smoke.py` against **both downloaded binaries**. Intel execution via Rosetta is valid when explicitly identified; arm64-only execution is not Intel acceptance.
- Install the published formula into an isolated Homebrew prefix, run `brew test pwelty/tap/textorium`, and compare installed binary bytes to the published asset. A custom-prefix warning is expected for this disposable test, not an instruction to alter the normal installation.
- Read back the normal installation separately. Do not silently upgrade it as part of release qualification.
- Put source/CI/release/formula links and qualification results in the tracking issue, then close it only when the requested distribution path works.

## Partial release recovery

Treat binary publication and tap update as separate outcomes. A failed final job does not mean the binaries failed or should be republished.

If tap checkout reports `Bad credentials`, record the error without exporting secrets. Do not copy an interactive OAuth token into Actions. Credential existence is not proof of validity. Track authorized credential repair separately; see [#136](https://github.com/pwelty/textorium-tui/issues/136).

After confirming the automatic job has stopped, a manual tap PR through authorized GitHub access can recover distribution: change only version, asset URLs, hashes, and version-test expectation, then verify the published formula and an isolated install. Preserve the failed job as evidence; do not describe manual recovery as repaired automation.

Do not blindly rerun an old release after manual recovery. The workflow's unconditional formula commit may fail when there is no diff, and future reruns must not replace qualified artifacts or regress the formula.

## Cleanup

Retain compact verification receipts and any unique reviewer reproductions before removing temporary worktrees, Cargo targets, or an isolated Homebrew installation. Confirm no process consumes those paths; remove registered worktrees through Git. Delete only attributable merged topic branches after confirming their exact remote heads. Do not sweep unrelated worktrees or commit preexisting local files merely to obtain a clean status.
