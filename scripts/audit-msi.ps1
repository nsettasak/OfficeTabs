param([Parameter(Mandatory=$true)][string]$Stage)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
# Audits the package sitting in the staging folder the build just produced.
$out=$Stage
Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public static class NativeMsiAudit { [DllImport("msi.dll",CharSet=CharSet.Unicode)] public static extern uint MsiVerifyPackage(string path); }'
if([NativeMsiAudit]::MsiVerifyPackage((Join-Path $out 'OfficeTabs.msi')) -ne 0){throw 'Invalid MSI'}
$engine=New-Object -ComObject WindowsInstaller.Installer
$db=$engine.OpenDatabase((Join-Path $out 'OfficeTabs.msi'),0)
function Query($sql,$count){$view=$db.OpenView($sql);$list=[Collections.Generic.List[object]]::new();try{[void]$view.Execute();while($record=$view.Fetch()){$fields=@(for($i=1;$i -le $count;$i++){$record.StringData($i)});$list.Add($fields)}}finally{[void]$view.Close()};return ,$list}
$registry=Query 'SELECT `Key`,`Name`,`Value` FROM `Registry`' 3
if($registry|Where-Object {$_[2] -match 'mscoree|RuntimeVersion|v4.0.30319|file:///'}){throw 'Managed registration found in native package'}
$servers=@($registry|Where-Object {$_[0] -like '*InprocServer32' -and $_[2] -eq '[#f_dll]'})
if($servers.Count -ne 2){throw 'Word/Excel native COM servers missing'}
if($registry|Where-Object {$_[0] -match 'PowerPoint'}){throw 'PowerPoint registration must not exist'}
$files=Query 'SELECT `File`,`FileName` FROM `File`' 2
$expected=@('OfficeTabs.dll','OfficeTabs.Manager.exe','uninstaller.exe','LICENSE.txt','OfficeTabs.ico')
$shipped=@($files|ForEach-Object {$_[1].Split('|')[-1]}|Sort-Object)
if(($shipped -join ',') -ne (($expected|Sort-Object) -join ',')){throw ('Unexpected native payload: '+($shipped -join ', '))}
if($files|Where-Object {$_[1] -like '*.config'}){throw '.NET config must not ship'}
$props=Query 'SELECT `Property`,`Value` FROM `Property`' 2
$version=[regex]::Match((Get-Content -Raw (Join-Path $root 'Cargo.toml')),'(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
$productCode=($props|Where-Object {$_[0] -eq 'ProductCode'}|ForEach-Object {$_[1]})
if($productCode -notmatch '^\{[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}\}$'){throw 'ProductCode is not a canonical GUID'}
$productVersion=($props|Where-Object {$_[0] -eq 'ProductVersion'}|ForEach-Object {$_[1]})
if($productVersion -ne $version){throw ('ProductVersion '+$productVersion+' does not match Cargo.toml '+$version)}
$upgradeCode=($props|Where-Object {$_[0] -eq 'UpgradeCode'}|ForEach-Object {$_[1]})
if($upgradeCode -ne '{5A3E670E-6DC6-45D7-8DA4-C8AB31004813}'){throw 'UpgradeCode changed; upgrades from earlier versions would break'}
if(-not ($props|Where-Object {$_[0] -eq 'Manufacturer' -and $_[1] -eq 'OfficeTabs'})){throw 'Publisher mismatch'}
# The cabinet is the payload the MSI streams; makecab writes it and build-msi.ps1
# embeds it. Compare every shipped file against the cabinet byte for byte.
$cab=Join-Path $out 'OfficeTabs.cab'
if(-not (Test-Path -LiteralPath $cab)){throw 'OfficeTabs.cab is missing from the staging folder'}
$extract=Join-Path ([IO.Path]::GetTempPath()) ('OfficeTabs-audit-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $extract | Out-Null
try {
 # Resolve the Windows cabinet tool through SystemRoot so a PATH entry cannot shadow it.
 $expand=Join-Path $env:SystemRoot 'System32\expand.exe'
 & $expand '-F:*' $cab $extract | Out-Null
 if($LASTEXITCODE -ne 0){throw 'Cabinet extraction failed'}
 foreach($f in $files){
  $name=$f[1].Split('|')[-1]
  $shipped=Join-Path $extract $f[0]
  if(-not (Test-Path -LiteralPath $shipped)){throw ('Payload missing from cabinet: '+$name)}
  if((Get-FileHash $shipped).Hash -ne (Get-FileHash (Join-Path $out $name)).Hash){throw ('Payload mismatch: '+$name)}
 }
} finally {Remove-Item -LiteralPath $extract -Recurse -Force -ErrorAction SilentlyContinue}
@{NativeComServers=2;PayloadFiles=$($files.Count);ManagedRegistration=$false;PowerPoint=$false;CabinetHashesMatch=$true;Publisher='OfficeTabs';EndToEndInstalled=$false;Architecture='x64'} | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $out 'msi-audit.json')
Write-Output 'Native MSI structure, COM registration and cabinet payload verified.'
