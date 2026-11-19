# Build

Office is not needed to build OfficeTabs or to run its tests.

## You need

- Rust with the MSVC x64 toolchain. `rust-toolchain.toml` is not used, so any recent stable
  version works; the last build was made with 1.99.1.
- Visual Studio 2022 C++ build tools and a Windows 10/11 SDK, for `link.exe`, `rc.exe` and
  `makecab`.
- Windows PowerShell and Python 3.11 or newer, for the packaging scripts and audits.

`Cargo.lock` is committed. The only two runtime dependencies are `windows-sys` and
`windows-link`.

## Build it

```powershell
./scripts/build.ps1
```

One command does everything:

1. `cargo test --release`
2. builds `officetabs.dll`, `OfficeTabs.Manager.exe` and `setup.exe`
3. authors the MSI with the Windows Installer COM API — no WiX
4. embeds the MSI inside `setup.exe`
5. runs the three audits
6. prints the file name, size and SHA-256 of the result

The result is a single file, `target/OfficeTabs-Rust-<version>-Setup.exe`. Files staged along
the way (DLL, MSI, cabinet, uninstaller) live in `%TEMP%` and are deleted afterwards, so there
is no `dist/` folder and no second copy of anything. `target/` is ignored by Git and
`cargo clean` removes it.

The build looks for the newest installed x64 Windows SDK resource compiler. If that search
picks the wrong one, set `RC_EXE` to the full path of `rc.exe`.
