$ErrorActionPreference='Stop'
# One entry point. Nothing stays behind except a single installer in the repository
# root: the DLL, Manager, MSI and cabinet are staged in %TEMP% and removed again.
# Only the finished installer is kept, inside target/ next to the other build output.
$root=Split-Path $PSScriptRoot -Parent
Push-Location $root
$stage=Join-Path ([IO.Path]::GetTempPath()) ('OfficeTabs-build-'+[guid]::NewGuid().ToString('N'))
$previousRustFlags=$env:CARGO_ENCODED_RUSTFLAGS
try {
 $env:CARGO_ENCODED_RUSTFLAGS=@('-C','target-feature=+crt-static',('--remap-path-prefix='+$env:USERPROFILE+'=/build-user'),('--remap-path-prefix='+$root+'=/officetabs')) -join [char]31
 New-Item -ItemType Directory -Force $stage | Out-Null
 Remove-Item Env:OFFICETABS_MSI -ErrorAction SilentlyContinue
 # --release so the tests reuse the same dependency tree as the product build;
 # a debug copy of windows-sys alone is ~1.5 GB of build cache.
 cargo test --locked --release
 if($LASTEXITCODE -ne 0){throw 'Rust tests failed'}
 cargo build --locked --release
 if($LASTEXITCODE -ne 0){throw 'Rust build failed'}
 Copy-Item target\release\officetabs.dll $stage\OfficeTabs.dll
 Copy-Item target\release\manager.exe $stage\OfficeTabs.Manager.exe
 Copy-Item target\release\setup.exe $stage\uninstaller.exe
 Copy-Item assets\OfficeTabs.ico,LICENSE.txt $stage
 & powershell -NoProfile -File (Join-Path $PSScriptRoot 'build-msi.ps1') -Stage $stage
 if($LASTEXITCODE -ne 0){throw 'MSI build failed'}
 $env:OFFICETABS_MSI=Join-Path $stage 'OfficeTabs.msi'
 cargo build --locked --release --bin setup
 if($LASTEXITCODE -ne 0){throw 'Standalone installer build failed'}
 $version=[regex]::Match((Get-Content -Raw (Join-Path $root 'Cargo.toml')),'(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
 $product=Join-Path $root ('target\OfficeTabs-Rust-'+$version+'-Setup.exe')
 Copy-Item target\release\setup.exe $product -Force
 & powershell -NoProfile -File (Join-Path $PSScriptRoot 'audit-msi.ps1') -Stage $stage
 if($LASTEXITCODE -ne 0){throw 'MSI audit failed'}
 python (Join-Path $PSScriptRoot 'audit-artifacts.py') $stage
 if($LASTEXITCODE -ne 0){throw 'Artifact audit failed'}
 python (Join-Path $PSScriptRoot 'privacy_audit.py') --payload $stage
 if($LASTEXITCODE -ne 0){throw 'Payload privacy audit failed'}
 $hash=(Get-FileHash $product -Algorithm SHA256).Hash.ToLower()
 Write-Output ('Built '+(Split-Path $product -Leaf)+'  '+((Get-Item $product).Length)+' bytes')
 Write-Output ('SHA-256 '+$hash)
} finally {
 Remove-Item Env:OFFICETABS_MSI -ErrorAction SilentlyContinue
 $env:CARGO_ENCODED_RUSTFLAGS=$previousRustFlags
 Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
 Pop-Location
}
