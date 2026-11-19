# OfficeTabs

Native document tabs for Microsoft Word and Excel on Windows, written in Rust.

**0.33 — experimental public preview.** This is an independent project, not an
ExtendOffice or Microsoft product. It is not a stable replacement for production
workflows yet. Test with disposable documents and keep backups.

## Features

- Document tabs with adaptive widths and drag-to-reorder.
- Detach a document and drag it back into a tab group, with a docking preview.
- Word and Excel controls in a light-themed Manager: enable, disable, reset settings.
- Close-all confirmation while preserving Office's own save/cancel prompts.
- Native COM add-in, single-file Setup, and uninstaller. No .NET runtime shim.

## Requirements and limitations

- Windows x64 and **64-bit desktop Word or Excel**. 32-bit Office, PowerPoint,
  Office for Mac and browser-based Office are not supported.
- Office version compatibility is not yet certified. Native layout depends on
  Office window internals that can change between releases.
- Docking operates within an Office process. Different Office processes cannot
  share a group. Tab order is session-only.
- Multi-monitor/DPI behavior, Ribbon preservation, save cancellation and installation
  rollback still need live validation. Keeping the same visible bounds does not
  guarantee Windows Shell Snap Group membership across different document windows.
- Switching to Rust alone does not establish better performance or stability.

## Install a preview

Use `Setup.exe` from a maintainer-reviewed GitHub Release, if one is available.
Close Word, Excel and the Manager first. The package installs for the current user
under `%LOCALAPPDATA%\Programs\OfficeTabs` and registers the Word/Excel COM add-ins.
It upgrades previous OfficeTabs installations. Restart Office after installation.

Use OfficeTabs Manager to enable or disable each component. To uninstall, use
Windows Installed apps or `uninstaller.exe` in the installation folder. Personal
settings are retained. Current builds are not Authenticode-signed; this repository
does not advise bypassing security blocks.

## Build from source

Prerequisites: Rust **1.99.1** (MSVC x64, the tested toolchain), Visual Studio 2022
C++ build tools, Windows 10/11 SDK, Windows PowerShell, and Python 3.11 or newer.
Office is not required for the automated tests or build. The two Rust dependencies
are vendored and Cargo.lock is committed; Cargo runs offline after toolchain setup.

Run from the repository root in PowerShell:

```powershell
./build.ps1
./audit-msi.ps1
python ./audit-artifacts.py
python ./privacy_audit.py --source
python ./privacy_audit.py
python ./package.py
```

The build discovers the newest installed x64 Windows SDK resource compiler.
Set `RC_EXE` to its full path if SDK discovery fails.

`build.ps1` runs tests and writes `dist/Setup.exe`, the native DLL, Manager,
uninstaller and MSI. It does **not** install anything, register the add-in, launch
Office or modify documents. `package.py` copies reviewed outputs to `release/`;
use `--out <folder>` for another destination. It creates **no ZIP by default**.
Source ZIP creation is reserved for an explicitly approved final release.

The generated `target/`, `dist/`, `msi/` and `release/` folders are not source and
must not be committed. Keep `vendor/` and its license/checksum files unchanged.

## Validation

The current suite has 32 tests: pure grouping/drag/layout state, MSI progress
records, mock Office COM lifecycle, and hidden Win32 window placement. These are
not live Office tests. GitHub Actions builds and audits the package but does not
install it or publish releases. See [the manual test checklist](docs/TESTING.md);
release history is in [docs/CHANGELOG.md](docs/CHANGELOG.md).

## Source map

| File | Responsibility |
| --- | --- |
| `src/model.rs` | Grouping, drag, ordering, animation and progress state |
| `src/automation.rs` | COM dispatch and VARIANT ownership |
| `src/com_server.rs` | Native COM server and Office lifecycle |
| `src/native.rs` | Tabs, window placement, hooks and Office actions |
| `src/ui.rs` | Shared native UI and DPI support |
| `src/bin/manager.rs` | Component controls and settings |
| `src/bin/setup.rs` | Installer/uninstaller and MSI progress |

## Privacy, contributions and license

No account, telemetry service or online activation is implemented. Local runtime
and setup logs are stored in `%LOCALAPPDATA%\OfficeTabs\Logs`; review them before
sharing. Build scripts remap developer paths, and release audits check common
identity/path leaks. These checks do not guarantee anonymity.

Read [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md) and
[PUBLISHING.md](PUBLISHING.md). Licensed under the [MIT License](LICENSE.txt).
Third-party dependencies retain their [own notices](THIRD_PARTY_NOTICES.md).
