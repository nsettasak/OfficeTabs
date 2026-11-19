$ErrorActionPreference='Stop'
Push-Location $PSScriptRoot
$previousRustFlags=$env:CARGO_ENCODED_RUSTFLAGS
try {
 $env:CARGO_ENCODED_RUSTFLAGS=@('-C','target-feature=+crt-static',('--remap-path-prefix='+$env:USERPROFILE+'=/build-user'),('--remap-path-prefix='+$PSScriptRoot+'=/officetabs')) -join [char]31
 New-Item -ItemType Directory -Force dist | Out-Null
 Remove-Item Env:OFFICETABS_MSI -ErrorAction SilentlyContinue
 cargo test --offline --locked
 if($LASTEXITCODE -ne 0){throw 'Rust tests failed'}
 cargo build --offline --locked --release
 if($LASTEXITCODE -ne 0){throw 'Rust build failed'}
 Copy-Item target\release\officetabs.dll dist\OfficeTabs.dll
 Copy-Item target\release\manager.exe dist\OfficeTabs.Manager.exe
 Copy-Item target\release\setup.exe dist\uninstaller.exe
 Copy-Item assets\OfficeTabs.ico,LICENSE.txt,NOTICE.txt,README.txt dist
 & powershell -NoProfile -File (Join-Path $PSScriptRoot 'build-msi.ps1')
 if($LASTEXITCODE -ne 0){throw 'MSI build failed'}
 $env:OFFICETABS_MSI=Join-Path $PSScriptRoot 'dist\OfficeTabs.msi'
 cargo build --offline --locked --release --bin setup
 if($LASTEXITCODE -ne 0){throw 'Standalone installer build failed'}
 Copy-Item target\release\setup.exe dist\Setup.exe
} finally {Remove-Item Env:OFFICETABS_MSI -ErrorAction SilentlyContinue;$env:CARGO_ENCODED_RUSTFLAGS=$previousRustFlags;Pop-Location}
