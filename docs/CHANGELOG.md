# OfficeTabs changelog

## 0.33 Native Preview

Runtime behaviour is unchanged: tab drawing, docking, drag/detach and close prompts
behave as in 0.32.

Maintenance and performance:

- Removed the dead `Model::placements` map (never written, only pruned).
- `Model::snapshot`/`reorder` sort against a rank table instead of rescanning the
  order vector per comparison; `view_iter` lets callers compare a frame's documents
  without cloning them.
- Strips rebuild tab captions and widths once per layout, and repaints reuse one
  brush per role instead of creating and deleting brushes per tab.
- Manager and Setup share the window chrome from `ui.rs` (`control`, `create`,
  `paint`, `ctlcolor`, `dpi_changed`, `destroy_and_quit`, `run_loop`); neither binary
  registers window classes, sets DPI context or pumps messages itself any more.
- Version strings come from `CARGO_PKG_VERSION`; the debug log no longer says 0.30.
- Dropped the unused `Win32_System_Threading` Cargo feature and narrowed `pub`
  visibility on internal helpers (`settings`, `log`, `fill`, `scale`, `initialize`).
- `CHANGES-0.31.txt` and `CHANGES-0.32.txt` are folded into this file, and
  `CONTINUE_PROMPT.txt` no longer sits in the repository root.
- Build: the release link no longer emits the LNK4104 notice (the COM exports have
  to stay public, so the specific diagnostic is ignored in `build.rs`).
- Build: `build.ps1` now produces a complete `dist/` — MSI (1.9 MB, product version
  0.33.0, `OfficeTabs.cab` stream) plus a `Setup.exe` with that MSI embedded
  (2.63 MB), and `audit-artifacts.py` verifies the embedding.

Verification: `cargo check`/`cargo clippy --all-targets` clean, 27 model tests,
3 setup tests, 1 COM test and 1 placement test pass without Office installed.

## 0.32 Native Preview

Reported: switching a docked document jumps out of the manually arranged/Snap position.

Normal/snapped switches now transfer current screen bounds (`GetWindowRect`), not
the pre-snap restore rectangle in `WINDOWPLACEMENT.rcNormalPosition`. Maximized
placement uses current monitor bounds and preserves raw workspace coordinates.
Bounds are applied before and after Office `Activate`; unchanged geometry is skipped.
No clamping of workspace coordinates as though they were screen coordinates.

Regression test: real hidden Win32 windows with deliberately different restore
and current rectangles; repeated switches at positive and negative coordinates.
No Office or user documents used. Live Office/Snap Layout testing still required.
This preserves visible bounds; it does not promise transfer of Windows Shell Snap
Group membership to a different Office HWND. Existing 0.31 UI is unchanged.

Reference: <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-iswindowarranged>
Reference: <https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-windowplacement>

## 0.31 Native Preview

Correction after user feedback.

User-reported regression: dock disappears when switching document tabs.
Code changes: reconcile workspace reservation after synchronous Office relayout;
recreate invalid strip/workspace handles; show hidden target before `Activate`;
keep the current frame when the target workspace is unavailable. Two regression
tests cover reservation restoration and repeated/resize layout without accumulating
inset. Live Office switching was NOT verified for this correction.

UI: restore light Manager cards with Word/Excel accents and right-side controls;
restore installer header, icon, footer, license and Back/Next navigation; correct
client-area DPI sizing; remove the overlapping status label on the license page;
avoid repainting unchanged progress text. Manager Cancel and installer navigation
were tested through real windows. No install or registry mutation performed during
UI QA.

Do not claim all Rust regressions fixed. The user's remaining bug list is unknown.
No changes to user documents or original ExtendOffice files.
