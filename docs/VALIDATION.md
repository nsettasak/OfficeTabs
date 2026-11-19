# Repository preparation validation

Checked locally on 2026-11-19 using Rust/Cargo 1.99.1 on Windows x64:

- Clean build with automatic Windows SDK discovery: passed.
- Automated tests: 32 passed, none failed.
- Native x64 executable headers, COM exports and embedded MSI: passed.
- MSI structure, publisher, registrations and cabinet payload hashes: passed.
- Source and release identity/path pattern checks: passed.
- Relative Markdown document links: passed.

GitHub Actions has not been executed on GitHub. Its configuration was reviewed
locally; only a pushed repository can establish the hosted-runner result.
Live Word/Excel behavior and end-to-end install/uninstall were not tested during
repository preparation. No remote repository or public release exists; one local
commit may exist and nothing has been pushed. `dist/` output is generated build
output and is ignored by Git. No ZIP archive was generated.
This record is not a security certification, legal clearance or anonymity guarantee.

## Re-verification after the 0.33 code cleanup

Same toolchain and date. `cargo fmt --check` clean, `cargo clippy --all-targets`
reports 0 warnings, 32 tests pass, `build.ps1` produces a complete `dist/`
(`OfficeTabs.msi` plus a `Setup.exe` with that MSI embedded byte-for-byte), and
`audit-msi.ps1`, `audit-artifacts.py`, `privacy_audit.py --source` and
`privacy_audit.py` all pass. `OfficeTabs.Manager.exe` was launched and closed as a
smoke test. Still not code-signed and still not tested against live Office.
