$ErrorActionPreference='Stop'
$out=Join-Path $PSScriptRoot 'dist'
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
if($files.Count -ne 7){throw 'Unexpected native payload'}
if($files|Where-Object {$_[1] -like '*.config'}){throw '.NET config must not ship'}
$props=Query 'SELECT `Property`,`Value` FROM `Property`' 2
if(-not ($props|Where-Object {$_[0] -eq 'ProductCode' -and $_[1] -eq '{959121E7-1F35-4D07-84F6-303030303033}'})){throw 'ProductCode mismatch'}
if(-not ($props|Where-Object {$_[0] -eq 'Manufacturer' -and $_[1] -eq 'OfficeTabs'})){throw 'Publisher mismatch'}
$extract=Join-Path $PSScriptRoot 'msi\audit-extracted';if(Test-Path -LiteralPath $extract){Remove-Item -LiteralPath $extract -Recurse -Force};New-Item -ItemType Directory -Force $extract | Out-Null
# Resolve the Windows cabinet tool through SystemRoot so a PATH entry cannot shadow it.
$expand=Join-Path $env:SystemRoot 'System32\expand.exe'
& $expand '-F:*' (Join-Path $PSScriptRoot 'msi\OfficeTabs.cab') $extract | Out-Null
if($LASTEXITCODE -ne 0){throw 'Cabinet extraction failed'}
foreach($f in $files){$name=$f[1].Split('|')[-1];if((Get-FileHash (Join-Path $extract $f[0])).Hash -ne (Get-FileHash (Join-Path $out $name)).Hash){throw ('Payload mismatch: '+$name)}}
@{NativeComServers=2;PayloadFiles=7;ManagedRegistration=$false;PowerPoint=$false;CabinetHashesMatch=$true;Publisher='OfficeTabs';EndToEndInstalled=$false;Architecture='x64'} | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $out 'msi-audit.json')
Write-Output 'Native MSI structure, COM registration and cabinet payload verified.'
