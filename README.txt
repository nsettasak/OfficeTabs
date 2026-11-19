OfficeTabs 0.33 Native Preview — Rust rewrite
Publisher: OfficeTabs
Targets: Word and Excel, 64-bit Windows Office only. PowerPoint is excluded.

All application runtime code is Rust: COM DLL, Manager, Setup and uninstaller.
MSI authoring remains a PowerShell build step. No CLR/.NET runtime shim is used.
Setup.exe is standalone and embeds the MSI. It upgrades earlier OfficeTabs using
the existing UpgradeCode; close Office before installation. No installation has
been performed by the developer in this build. Do not treat this preview as
live-validated replacement for 0.28. Back up the previous installer before testing.

Implemented: native Office COM lifecycle, document snapshots, start-page exclusion,
adaptive tabs, document activation/new/close, tab ordering, independent windows,
mouse-captured drag and white dock preview, dock on release without dwell,
monitor-aware group placement, native Office save prompts and cancellation,
close-all/active/cancel with persistent skip setting, enable/disable/reset Manager,
MIT About, native MSI progress records including preparation and rollback.

Important limits: 32-bit Office is not supported by this build. Live Office COM
integration, native window layout, cross-monitor DPI, Ribbon behavior, perceived
smoothness and end-to-end installation/uninstallation require live validation.
Performance is not guaranteed merely by switching languages. Tab order is session
state; settings persist in LOCALAPPDATA/OfficeTabs/Settings. Manager changes to
loading take full effect after Office restart. Runtime diagnostics are written to
LOCALAPPDATA/OfficeTabs/Logs/Rust-native.log; setup logs use Native-Setup-*.log.

Build with Windows MSVC Rust toolchain, Visual Studio C++ build tools, Windows SDK.
Run build.ps1 from this folder. It runs tests, builds native files, creates MSI,
and embeds MSI into Setup.exe. It does not install, register COM or launch Office.
The generated target/, msi/ and dist/ folders are build output, not source.
build.ps1 recreates them. Run audit-msi.ps1, audit-artifacts.py and privacy_audit.py
after building, then package.py. Do not use --final until the user declares final.
The developer archive includes vendored dependencies and Cargo.lock for offline builds.
Set RC_EXE if the Windows SDK resource compiler is installed at another path.
Preserve COM CLSIDs and UpgradeCode; change ProductCode for future major upgrades.

Architecture:
src/model.rs       Pure group, ordering, drag, animation and MSI progress model.
src/automation.rs  Owned IDispatch/VARIANT boundaries and late-bound Office calls.
src/com_server.rs  Native IClassFactory and IDTExtensibility2, explicit lifetimes.
src/native.rs      STA controller, Win32 child strips, GDI buffering, native hooks.
src/bin/manager.rs Native Manager window and user-owned settings/registration.
src/bin/setup.rs   Worker-thread MSI execution, actual record progress, cancel.

Do not suppress Office save alerts, mark documents Saved, or bypass licenses.
Keep original ExtendOffice files untouched. Never run Office on behalf of tests
without user authorization. Do not install this preview automatically.
