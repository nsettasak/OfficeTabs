param([Parameter(Mandatory=$true)][string]$Stage)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
# $Stage holds the payload the caller assembled (build.ps1); the MSI is authored there.
$out=$Stage
# makecab scratch gets its own %TEMP% directory, so the staging folder only ever holds
# the payload plus OfficeTabs.msi and OfficeTabs.cab.
$scratch=Join-Path ([IO.Path]::GetTempPath()) ('OfficeTabs-msi-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $scratch | Out-Null
$upgrade='{5A3E670E-6DC6-45D7-8DA4-C8AB31004813}'
# Cargo.toml is the only place the version is written; the ProductCode is
# derived from it, so a new version always gets a fresh product code while the
# UpgradeCode stays fixed for upgrades.
$version=[regex]::Match((Get-Content -Raw (Join-Path $root 'Cargo.toml')),'(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if(-not $version){throw 'Version not found in Cargo.toml'}
$versionMinor=($version -split '\.')[0..1] -join '.'
$msiPath=Join-Path $out 'OfficeTabs.msi'
if(Test-Path -LiteralPath $msiPath){Remove-Item -LiteralPath $msiPath}
$engine=New-Object -ComObject WindowsInstaller.Installer
$db=$engine.OpenDatabase($msiPath,3)
function Table($name,$definition){$v=$db.OpenView(('CREATE TABLE `'+$name+'` ('+$definition+')'));try{$v.Execute()}finally{$v.Close()}}
function Row($table,$columns,$values){
 $sql='INSERT INTO `'+$table+'` ('+(($columns|ForEach-Object {'`'+$_+'`'}) -join ',')+') VALUES ('+((@('?')*$columns.Count) -join ',')+')'
 $view=$db.OpenView($sql);$record=$engine.CreateRecord($values.Count)
 try{for($i=0;$i -lt $values.Count;$i++){if($null -eq $values[$i]){continue};if($values[$i] -is [int]){$record.IntegerData($i+1)=$values[$i]}else{$record.StringData($i+1)=[string]$values[$i]}};$view.Execute($record)}finally{$view.Close()}
}
function GuidFor($key){$md5=[Security.Cryptography.MD5]::Create();try{([Guid]::new($md5.ComputeHash([Text.Encoding]::UTF8.GetBytes('OfficeTabs.'+$version+'.'+$key)))).ToString('B').ToUpper()}finally{$md5.Dispose()}}
$product=GuidFor 'Product'
Table Property '`Property` CHAR(72) NOT NULL, `Value` CHAR(0) LOCALIZABLE PRIMARY KEY `Property`'
$properties=@{ProductCode=$product;UpgradeCode=$upgrade;ProductName='OfficeTabs';ProductVersion=$version;ProductLanguage='1033';Manufacturer='OfficeTabs';INSTALLLEVEL='1';ALLUSERS='';ARPNOMODIFY='1';ARPPRODUCTICON='OfficeTabs.ico';ARPHELPLINK='';ARPCOMMENTS='Document tabs for Microsoft Office';DefaultUIFont='Normal';SecureCustomProperties='PREVIOUSVERSIONS';REBOOT='ReallySuppress'}
foreach($pair in $properties.GetEnumerator()){if($pair.Value -ne ''){Row Property @('Property','Value') @($pair.Key,$pair.Value)}}
Table Directory '`Directory` CHAR(72) NOT NULL, `Directory_Parent` CHAR(72), `DefaultDir` CHAR(255) NOT NULL LOCALIZABLE PRIMARY KEY `Directory`'
foreach($r in @(@('TARGETDIR',$null,'SourceDir'),@('LocalAppDataFolder','TARGETDIR','.'),@('ProgramsFolder','LocalAppDataFolder','Programs'),@('INSTALLFOLDER','ProgramsFolder','OfficeTabs'),@('ProgramMenuFolder','TARGETDIR','.'),@('MenuFolder','ProgramMenuFolder','OfficeTabs'))){Row Directory @('Directory','Directory_Parent','DefaultDir') $r}
Row Directory @('Directory','Directory_Parent','DefaultDir') @('LegacyMenuFolder','ProgramMenuFolder','OLDTAB~1|Independent Office Tabs')
Table Component '`Component` CHAR(72) NOT NULL, `ComponentId` CHAR(38), `Directory_` CHAR(72) NOT NULL, `Attributes` SHORT NOT NULL, `Condition` CHAR(255), `KeyPath` CHAR(72) PRIMARY KEY `Component`'
Table Feature '`Feature` CHAR(38) NOT NULL, `Feature_Parent` CHAR(38), `Title` CHAR(64) LOCALIZABLE, `Description` CHAR(255) LOCALIZABLE, `Display` SHORT, `Level` SHORT NOT NULL, `Directory_` CHAR(72), `Attributes` SHORT NOT NULL PRIMARY KEY `Feature`'
Row Feature @('Feature','Title','Description','Display','Level','Directory_','Attributes') @('Complete','OfficeTabs','Tabs for Word and Excel, and Manager.',1,1,'INSTALLFOLDER',0)
Table FeatureComponents '`Feature_` CHAR(38) NOT NULL, `Component_` CHAR(72) NOT NULL PRIMARY KEY `Feature_`, `Component_`'
function Component($id,$directory,$attributes,$key){Row Component @('Component','ComponentId','Directory_','Attributes','KeyPath') @($id,(GuidFor $id),$directory,$attributes,$key);Row FeatureComponents @('Feature_','Component_') @('Complete',$id)}
Table File '`File` CHAR(72) NOT NULL, `Component_` CHAR(72) NOT NULL, `FileName` CHAR(255) NOT NULL LOCALIZABLE, `FileSize` LONG NOT NULL, `Version` CHAR(72), `Language` CHAR(20), `Attributes` SHORT, `Sequence` LONG NOT NULL PRIMARY KEY `File`'
$payload=@(@('f_dll','OfficeTabs.dll'),@('f_manager','OfficeTabs.Manager.exe'),@('f_uninstaller','uninstaller.exe'),@('f_license','LICENSE.txt'),@('f_icon','OfficeTabs.ico'))
$ddf=@('.OPTION EXPLICIT','.Set Cabinet=on','.Set Compress=on','.Set CompressionType=MSZIP','.Set CabinetNameTemplate=OfficeTabs.cab',('.Set DiskDirectoryTemplate="'+$scratch+'"'),'.Set MaxDiskSize=0',('.Set RptFileName="'+(Join-Path $scratch 'cabinet.rpt')+'"'),('.Set InfFileName="'+(Join-Path $scratch 'cabinet.inf')+'"'))
$sequence=0
foreach($item in $payload){$sequence++;$id=$item[0];$file=Get-Item -LiteralPath (Join-Path $out $item[1]);Component ('c_'+$id) 'INSTALLFOLDER' 0 $id;$version=$null;if($file.Extension -in @('.dll','.exe')){$version=$file.VersionInfo.FileVersion};Row File @('File','Component_','FileName','FileSize','Version','Attributes','Sequence') @($id,('c_'+$id),('F'+$sequence.ToString('000')+'.'+$file.Extension.TrimStart('.').Substring(0,[Math]::Min(3,$file.Extension.Length-1)).ToUpper()+'|'+$file.Name),[int]$file.Length,$version,512,$sequence);$ddf+=('"'+$file.FullName+'" '+$id)}
$ddfPath=Join-Path $scratch 'payload.ddf';$ddf|Set-Content -LiteralPath $ddfPath -Encoding ascii
& makecab.exe /F $ddfPath | Out-File -LiteralPath (Join-Path $scratch 'makecab.log')
if($LASTEXITCODE -ne 0){throw 'Cabinet creation failed'}
Table Media '`DiskId` SHORT NOT NULL, `LastSequence` LONG NOT NULL, `DiskPrompt` CHAR(64) LOCALIZABLE, `Cabinet` CHAR(255), `VolumeLabel` CHAR(32), `Source` CHAR(72) PRIMARY KEY `DiskId`'
Row Media @('DiskId','LastSequence','Cabinet') @(1,$sequence,'#OfficeTabs.cab')
$stream=$db.OpenView('INSERT INTO `_Streams` (`Name`,`Data`) VALUES (?,?)');$record=$engine.CreateRecord(2);$record.StringData(1)='OfficeTabs.cab';$record.SetStream(2,(Join-Path $scratch 'OfficeTabs.cab'));$stream.Execute($record);$stream.Close()
Table Registry '`Registry` CHAR(72) NOT NULL, `Root` SHORT NOT NULL, `Key` CHAR(255) NOT NULL LOCALIZABLE, `Name` CHAR(255) LOCALIZABLE, `Value` CHAR(0) LOCALIZABLE, `Component_` CHAR(72) NOT NULL PRIMARY KEY `Registry`'
$regNumber=0
function Reg($component,$key,$name,$value){$script:regNumber++;$id='r'+$script:regNumber;Row Registry @('Registry','Root','Key','Name','Value','Component_') @($id,1,$key,$name,$value,$component);return $id}
# Native COM registration; no CLR shim or assembly metadata.
$hosts=@(@('Word','{D6217423-507F-43AE-BF5B-5CE26989E401}','OfficeAddin'),@('Excel','{371AD550-752A-42F8-BB8D-7D17961D9F02}','ExcelAddin'))
foreach($bits in @(64)){
 $component='reg'+$bits;$firstReg='r'+($regNumber+1)
 Component $component 'INSTALLFOLDER' $(if($bits -eq 64){260}else{4}) $firstReg
 foreach($h in $hosts){
  $prog='IndependentOfficeTabs.'+$h[0];$cls='Software\Classes\CLSID\'+$h[1];$class='IndependentWordTabs.'+$h[2]
  [void](Reg $component $cls $null ('OfficeTabs for '+$h[0]));[void](Reg $component ($cls+'\ProgId') $null $prog)
  $key=$cls+'\InprocServer32'
  [void](Reg $component $key $null '[#f_dll]')
  [void](Reg $component $key 'ThreadingModel' 'Apartment')
  [void](Reg $component ('Software\Classes\'+$prog) $null ('OfficeTabs for '+$h[0]));[void](Reg $component ('Software\Classes\'+$prog+'\CLSID') $null $h[1])
  $key='Software\Microsoft\Office\'+$h[0]+'\Addins\'+$prog
  foreach($pair in @(@('FriendlyName',('OfficeTabs for '+$h[0])),@('Description','OfficeTabs document tabs'),@('LoadBehavior','#3'),@('CommandLineSafe','#0'))){[void](Reg $component $key $pair[0] $pair[1])}
 }
}
# Remove only the obsolete uninstall entry created by this project's pre-MSI installer.
Table RemoveRegistry '`RemoveRegistry` CHAR(72) NOT NULL, `Root` SHORT NOT NULL, `Key` CHAR(255) NOT NULL LOCALIZABLE, `Name` CHAR(255) LOCALIZABLE, `Component_` CHAR(72) NOT NULL PRIMARY KEY `RemoveRegistry`'
Row RemoveRegistry @('RemoveRegistry','Root','Key','Name','Component_') @('OldUninstall32',1,'Software\Microsoft\Windows\CurrentVersion\Uninstall\IndependentOfficeTabs','-','reg64')
Row RemoveRegistry @('RemoveRegistry','Root','Key','Name','Component_') @('OldUninstall64',1,'Software\Microsoft\Windows\CurrentVersion\Uninstall\IndependentOfficeTabs','-','reg64')
Table Shortcut '`Shortcut` CHAR(72) NOT NULL, `Directory_` CHAR(72) NOT NULL, `Name` CHAR(128) NOT NULL LOCALIZABLE, `Component_` CHAR(72) NOT NULL, `Target` CHAR(255) NOT NULL, `Arguments` CHAR(255), `Description` CHAR(255) LOCALIZABLE, `Hotkey` SHORT, `Icon_` CHAR(72), `IconIndex` SHORT, `ShowCmd` SHORT, `WkDir` CHAR(72) PRIMARY KEY `Shortcut`'
Table Icon '`Name` CHAR(72) NOT NULL, `Data` OBJECT NOT NULL PRIMARY KEY `Name`'
Table Binary '`Name` CHAR(72) NOT NULL, `Data` OBJECT NOT NULL PRIMARY KEY `Name`'
foreach($table in @('Icon','Binary')){
 $name=$(if($table -eq 'Icon'){'OfficeTabs.ico'}else{'OfficeTabsBrand'})
 $v=$db.OpenView(('INSERT INTO `'+$table+'` (`Name`,`Data`) VALUES (?,?)'));$r=$engine.CreateRecord(2);$r.StringData(1)=$name;[void]$r.SetStream(2,(Join-Path $out 'OfficeTabs.ico'));[void]$v.Execute($r);[void]$v.Close()
}
Row Shortcut @('Shortcut','Directory_','Name','Component_','Target','Description','ShowCmd','WkDir','Icon_','IconIndex') @('Manager','MenuFolder','OfficeTabs Manager','c_f_manager','[#f_manager]','Manage OfficeTabs components',1,'INSTALLFOLDER','OfficeTabs.ico',0)
Row Shortcut @('Shortcut','Directory_','Name','Component_','Target','Description','ShowCmd','WkDir','Icon_','IconIndex') @('Uninstall','MenuFolder','Uninstall OfficeTabs','c_f_uninstaller','[#f_uninstaller]','Remove OfficeTabs',1,'INSTALLFOLDER','OfficeTabs.ico',0)
Table RemoveFile '`FileKey` CHAR(72) NOT NULL, `Component_` CHAR(72) NOT NULL, `FileName` CHAR(255) LOCALIZABLE, `DirProperty` CHAR(72) NOT NULL, `InstallMode` SHORT NOT NULL PRIMARY KEY `FileKey`'
Row RemoveFile @('FileKey','Component_','DirProperty','InstallMode') @('RemoveMenu','c_f_manager','MenuFolder',2)
Row RemoveFile @('FileKey','Component_','DirProperty','InstallMode') @('RemoveInstallFolder','c_f_manager','INSTALLFOLDER',2)
Row RemoveFile @('FileKey','Component_','FileName','DirProperty','InstallMode') @('OldManagerShortcut','c_f_manager','Manager.lnk','LegacyMenuFolder',1)
Row RemoveFile @('FileKey','Component_','FileName','DirProperty','InstallMode') @('OldUninstallShortcut','c_f_manager','Uninstall.lnk','LegacyMenuFolder',1)
Row RemoveFile @('FileKey','Component_','DirProperty','InstallMode') @('OldMenuFolder','c_f_manager','LegacyMenuFolder',1)
Table Upgrade '`UpgradeCode` CHAR(38) NOT NULL, `VersionMin` CHAR(20), `VersionMax` CHAR(20), `Language` CHAR(255), `Attributes` LONG NOT NULL, `Remove` CHAR(255), `ActionProperty` CHAR(72) NOT NULL PRIMARY KEY `UpgradeCode`, `VersionMin`, `VersionMax`, `Language`, `Attributes`'
Row Upgrade @('UpgradeCode','VersionMax','Attributes','ActionProperty') @($upgrade,$version,513,'PREVIOUSVERSIONS')
foreach($table in @('InstallExecuteSequence','InstallUISequence')){Table $table '`Action` CHAR(72) NOT NULL, `Condition` CHAR(255), `Sequence` SHORT PRIMARY KEY `Action`'}
foreach($r in @(@('FindRelatedProducts',25),@('LaunchConditions',100),@('ValidateProductID',700),@('CostInitialize',800),@('FileCost',900),@('CostFinalize',1000),@('InstallValidate',1400),@('InstallInitialize',1500),@('RemoveExistingProducts',1510),@('ProcessComponents',1600),@('UnpublishFeatures',1800),@('RemoveShortcuts',3200),@('RemoveRegistryValues',3300),@('RemoveFiles',3500),@('InstallFiles',4000),@('CreateShortcuts',4500),@('WriteRegistryValues',5000),@('RegisterUser',6000),@('RegisterProduct',6100),@('PublishFeatures',6300),@('PublishProduct',6400),@('InstallExecute',6500),@('InstallFinalize',6600))){Row InstallExecuteSequence @('Action','Sequence') $r}
foreach($r in @(@('FindRelatedProducts',25),@('LaunchConditions',100),@('CostInitialize',800),@('FileCost',900),@('CostFinalize',1000),@('ExecuteAction',1300))){Row InstallUISequence @('Action','Sequence') $r}
Table LaunchCondition '`Condition` CHAR(255) NOT NULL, `Description` CHAR(255) NOT NULL LOCALIZABLE PRIMARY KEY `Condition`'
Row LaunchCondition @('Condition','Description') @('VersionNT64','OfficeTabs requires 64-bit Windows. It supports 32-bit and 64-bit desktop Office.')
Table TextStyle '`TextStyle` CHAR(72) NOT NULL, `FaceName` CHAR(32) NOT NULL, `Size` SHORT NOT NULL, `Color` LONG, `StyleBits` SHORT PRIMARY KEY `TextStyle`'
Row TextStyle @('TextStyle','FaceName','Size','Color','StyleBits') @('Normal','Segoe UI',9,0,0)
Row TextStyle @('TextStyle','FaceName','Size','Color','StyleBits') @('Title','Segoe UI',13,0,1)
Table Dialog '`Dialog` CHAR(72) NOT NULL, `HCentering` SHORT NOT NULL, `VCentering` SHORT NOT NULL, `Width` SHORT NOT NULL, `Height` SHORT NOT NULL, `Attributes` LONG NOT NULL, `Title` CHAR(128) LOCALIZABLE, `Control_First` CHAR(50), `Control_Default` CHAR(50), `Control_Cancel` CHAR(50) PRIMARY KEY `Dialog`'
Table Control '`Dialog_` CHAR(72) NOT NULL, `Control` CHAR(50) NOT NULL, `Type` CHAR(20) NOT NULL, `X` SHORT NOT NULL, `Y` SHORT NOT NULL, `Width` SHORT NOT NULL, `Height` SHORT NOT NULL, `Attributes` LONG, `Property` CHAR(72), `Text` CHAR(0) LOCALIZABLE, `Control_Next` CHAR(50), `Help` CHAR(50) LOCALIZABLE PRIMARY KEY `Dialog_`, `Control`'
Table ControlEvent '`Dialog_` CHAR(72) NOT NULL, `Control_` CHAR(50) NOT NULL, `Event` CHAR(50) NOT NULL, `Argument` CHAR(255) NOT NULL, `Condition` CHAR(255), `Ordering` SHORT PRIMARY KEY `Dialog_`, `Control_`, `Event`, `Argument`, `Condition`'
function Control($dialog,$id,$type,$x,$y,$w,$h,$text,$next=$null){Row Control @('Dialog_','Control','Type','X','Y','Width','Height','Attributes','Text','Control_Next') @($dialog,$id,$type,$x,$y,$w,$h,3,$text,$next)}
function Event($dialog,$button,$event,$argument,$order=1){Row ControlEvent @('Dialog_','Control_','Event','Argument','Condition','Ordering') @($dialog,$button,$event,$argument,'1',$order)}
function Dialog($id,$title,$body,$nextText,$default='Next'){
 Row Dialog @('Dialog','HCentering','VCentering','Width','Height','Attributes','Title','Control_First','Control_Default','Control_Cancel') @($id,50,50,420,285,3,'OfficeTabs Setup',$default,$default,'Cancel')
 Control $id 'Heading' 'Text' 22 18 310 38 ('{\Title}'+$title)
 Row Control @('Dialog_','Control','Type','X','Y','Width','Height','Attributes','Text') @($id,'Brand','Icon',365,13,34,34,6291457,'OfficeTabsBrand')
 Control $id 'Body' 'Text' 24 72 372 145 $body
 Control $id 'Line' 'Line' 0 245 420 1 ''
 Control $id 'HeaderLine' 'Line' 0 61 420 1 ''
 Control $id 'Next' 'PushButton' 265 257 68 19 $nextText 'Cancel'
 Control $id 'Cancel' 'PushButton' 341 257 65 19 'Cancel' $default
 Event $id 'Cancel' 'EndDialog' 'Exit'
}
Dialog 'WelcomeDlg' 'Welcome to OfficeTabs Setup' 'This wizard will guide you through the installation of OfficeTabs.

OfficeTabs adds document tabs to Word and Excel.

Save your work and close Microsoft Office before continuing.

Click Next to continue.' 'Next >'
Dialog 'FolderDlg' 'Installation Folder' 'OfficeTabs will be installed for your Windows account in:

[INSTALLFOLDER]

The installer creates Start Menu shortcuts and an entry in Windows Installed apps.' 'Next >'
Dialog 'ReadyDlg' 'Ready to Install OfficeTabs' 'Setup will install document tabs for Word and Excel, along with OfficeTabs Manager.

Destination: [INSTALLFOLDER]

Click Install to begin. Click Back to review your settings.' 'Install'
Dialog 'LicenseDlg' 'License Information' '' 'Next >'
$licenseText=Get-Content -LiteralPath (Join-Path $out 'LICENSE.txt') -Raw
$rtf='{\rtf1\ansi\deff0{\fonttbl{\f0 Segoe UI;}}\f0\fs18 '+$licenseText.Replace('\','\\').Replace('{','\{').Replace('}','\}').Replace("`r",'').Replace("`n",'\par ' )+'}'
Control 'LicenseDlg' 'LicenseText' 'ScrollableText' 24 72 372 159 $rtf
foreach($id in @('LicenseDlg','FolderDlg','ReadyDlg')){Control $id 'Back' 'PushButton' 189 257 68 19 '< Back' 'Next'}
Event 'WelcomeDlg' 'Next' 'NewDialog' 'LicenseDlg'
Event 'LicenseDlg' 'Back' 'NewDialog' 'WelcomeDlg';Event 'LicenseDlg' 'Next' 'NewDialog' 'FolderDlg'
Event 'FolderDlg' 'Back' 'NewDialog' 'LicenseDlg';Event 'FolderDlg' 'Next' 'NewDialog' 'ReadyDlg'
Event 'ReadyDlg' 'Back' 'NewDialog' 'FolderDlg';Event 'ReadyDlg' 'Next' 'EndDialog' 'Return'
Dialog 'MaintenanceDlg' 'OfficeTabs is installed' 'Use Repair to restore application files and registrations, or Remove to uninstall OfficeTabs. Close Office and Manager first.' 'Repair'
Control 'MaintenanceDlg' 'Remove' 'PushButton' 189 257 68 19 'Remove' 'Next'
Event 'MaintenanceDlg' 'Next' '[REINSTALL]' 'ALL';Event 'MaintenanceDlg' 'Next' '[REINSTALLMODE]' 'omus' 2;Event 'MaintenanceDlg' 'Next' 'EndDialog' 'Return' 3
Event 'MaintenanceDlg' 'Remove' '[REMOVE]' 'ALL';Event 'MaintenanceDlg' 'Remove' 'EndDialog' 'Return' 2
Dialog 'RemoveDlg' 'Remove OfficeTabs?' 'Click Remove to uninstall OfficeTabs and its Office add-in registrations. Your personal settings will be retained.' 'Remove'
Event 'RemoveDlg' 'Next' 'EndDialog' 'Return'
Dialog 'ExitDlg' 'OfficeTabs Setup Completed' 'The requested operation completed successfully. You can find OfficeTabs Manager in the Start Menu. Click Finish to exit Setup.' 'Finish'
Event 'ExitDlg' 'Next' 'EndDialog' 'Return'
Dialog 'ErrorDlg' 'Setup did not complete' 'Windows Installer could not complete the requested operation. No success is being reported. Close Office and try again, or check the installer log.' 'Close'
Event 'ErrorDlg' 'Next' 'EndDialog' 'Return'
Dialog 'UserExitDlg' 'Setup Cancelled' 'Setup was cancelled. Click Close to exit.' 'Close';Event 'UserExitDlg' 'Next' 'EndDialog' 'Return'
Row InstallUISequence @('Action','Condition','Sequence') @('WelcomeDlg','NOT Installed',1230)
Row InstallUISequence @('Action','Condition','Sequence') @('MaintenanceDlg','Installed AND NOT REMOVE AND NOT REINSTALL',1231)
Row InstallUISequence @('Action','Condition','Sequence') @('RemoveDlg','Installed AND REMOVE="ALL"',1232)
Row InstallUISequence @('Action','Sequence') @('ExitDlg',-1)
Row InstallUISequence @('Action','Sequence') @('UserExitDlg',-2)
Row InstallUISequence @('Action','Sequence') @('ErrorDlg',-3)
# Windows Installer provides native progress for basic UI; full UI uses this modeless progress dialog.
Row Dialog @('Dialog','HCentering','VCentering','Width','Height','Attributes','Title','Control_First','Control_Default','Control_Cancel') @('ProgressDlg',50,50,420,285,1,'OfficeTabs Setup','Cancel','Cancel','Cancel')
Control 'ProgressDlg' 'Heading' 'Text' 22 18 310 38 '{\Title}Updating OfficeTabs'
Row Control @('Dialog_','Control','Type','X','Y','Width','Height','Attributes','Text') @('ProgressDlg','Brand','Icon',365,13,34,34,6291457,'OfficeTabsBrand')
Control 'ProgressDlg' 'Status' 'Text' 24 82 370 23 'Preparing OfficeTabs...'
Control 'ProgressDlg' 'Detail' 'Text' 24 110 370 18 ''
Row Control @('Dialog_','Control','Type','X','Y','Width','Height','Attributes','Text') @('ProgressDlg','Progress','ProgressBar',24,137,370,16,1,'')
Control 'ProgressDlg' 'Preparing' 'Text' 24 170 370 18 'Preparing installation...'
Control 'ProgressDlg' 'Remaining' 'Text' 24 194 370 18 ''
Control 'ProgressDlg' 'HeaderLine' 'Line' 0 61 420 1 ''
Control 'ProgressDlg' 'Line' 'Line' 0 245 420 1 ''
Control 'ProgressDlg' 'Cancel' 'PushButton' 341 257 65 19 'Cancel'
Event 'ProgressDlg' 'Cancel' 'EndDialog' 'Exit'
Table EventMapping '`Dialog_` CHAR(72) NOT NULL, `Control_` CHAR(50) NOT NULL, `Event` CHAR(50) NOT NULL, `Attribute` CHAR(50) NOT NULL PRIMARY KEY `Dialog_`, `Control_`, `Event`'
Row EventMapping @('Dialog_','Control_','Event','Attribute') @('ProgressDlg','Progress','SetProgress','Progress')
Row EventMapping @('Dialog_','Control_','Event','Attribute') @('ProgressDlg','Status','ActionText','Text')
Row EventMapping @('Dialog_','Control_','Event','Attribute') @('ProgressDlg','Detail','ActionData','Text')
Row EventMapping @('Dialog_','Control_','Event','Attribute') @('ProgressDlg','Preparing','ScriptInProgress','Visible')
Row EventMapping @('Dialog_','Control_','Event','Attribute') @('ProgressDlg','Remaining','TimeRemaining','TimeRemaining')
Table UIText '`Key` CHAR(72) NOT NULL, `Text` CHAR(255) NOT NULL LOCALIZABLE PRIMARY KEY `Key`'
Row UIText @('Key','Text') @('ScriptInProgress','Preparing installation...')
Row UIText @('Key','Text') @('TimeRemaining','Estimated time remaining: {[1] min }{[2] sec}')
Table ActionText '`Action` CHAR(72) NOT NULL, `Description` CHAR(64) LOCALIZABLE, `Template` CHAR(128) LOCALIZABLE PRIMARY KEY `Action`'
foreach($item in @(
 @('GenerateScript','Preparing installation...','[1]'),
 @('Rollback','Restoring the previous configuration...','[1]'),
 @('RollbackCleanup','Finishing rollback...','[1]'),
 @('ProcessComponents','Preparing OfficeTabs components...',$null),
 @('RemoveExistingProducts','Updating the previous version...',$null),
 @('InstallFiles','Copying application files...','[1]'),
 @('RemoveFiles','Removing application files...','[1]'),
 @('WriteRegistryValues','Configuring OfficeTabs components...','Component settings'),
 @('RemoveRegistryValues','Removing OfficeTabs configuration...','Component settings'),
 @('CreateShortcuts','Creating Start Menu shortcuts...','[1]'),
 @('RemoveShortcuts','Removing Start Menu shortcuts...','[1]'),
 @('RegisterUser','Registering the installation...',$null),
 @('RegisterProduct','Registering OfficeTabs with Windows...',$null),
 @('PublishFeatures','Updating installed components...',$null),
 @('PublishProduct','Updating Installed apps...',$null),
 @('InstallExecute','Installing OfficeTabs...',$null),
 @('InstallFinalize','Completing setup...',$null)
)){Row ActionText @('Action','Description','Template') $item}
Row InstallUISequence @('Action','Sequence') @('ProgressDlg',1290)
foreach($id in @('ExitDlg','ErrorDlg','UserExitDlg')){
 $v=$db.OpenView('UPDATE `Control` SET `Attributes`=0 WHERE `Dialog_`=? AND `Control`=?');$r=$engine.CreateRecord(2);$r.StringData(1)=$id;$r.StringData(2)='Cancel';[void]$v.Execute($r);[void]$v.Close()
 $v=$db.OpenView('UPDATE `Dialog` SET `Control_Cancel`=? WHERE `Dialog`=?');$r=$engine.CreateRecord(2);$r.StringData(1)='Next';$r.StringData(2)=$id;[void]$v.Execute($r);[void]$v.Close()
}
# Every focusable control must belong to one circular tab-order chain.
# Clear obsolete edges first, including edges from hidden Cancel controls.
$dialogView=$db.OpenView('SELECT `Dialog` FROM `Dialog`');[void]$dialogView.Execute()
while($dialogRecord=$dialogView.Fetch()){
 $dialogId=$dialogRecord.StringData(1)
 $clear=$db.OpenView('UPDATE `Control` SET `Control_Next`=? WHERE `Dialog_`=?');$r=$engine.CreateRecord(2);$r.StringData(2)=$dialogId;[void]$clear.Execute($r);[void]$clear.Close()
 $focusView=$db.OpenView('SELECT `Control`,`Type`,`Attributes` FROM `Control` WHERE `Dialog_`=?');$r=$engine.CreateRecord(1);$r.StringData(1)=$dialogId;[void]$focusView.Execute($r)
 $active=@();while($focus=$focusView.Fetch()){if($focus.StringData(2) -eq 'PushButton' -and ($focus.IntegerData(3) -band 3) -eq 3){$active+=,$focus.StringData(1)}};[void]$focusView.Close()
 $ordered=@(@('Back','Next','Remove','Cancel') | Where-Object {$_ -in $active})
 for($i=0;$i -lt $ordered.Count;$i++){
  $edge=$db.OpenView('UPDATE `Control` SET `Control_Next`=? WHERE `Dialog_`=? AND `Control`=?');$r=$engine.CreateRecord(3);$r.StringData(1)=$ordered[($i+1)%$ordered.Count];$r.StringData(2)=$dialogId;$r.StringData(3)=$ordered[$i];[void]$edge.Execute($r);[void]$edge.Close()
 }
}
[void]$dialogView.Close()
$summary=$db.SummaryInformation(20);$summary.Property(2)='OfficeTabs Installation';$summary.Property(3)='OfficeTabs '+$versionMinor;$summary.Property(4)='OfficeTabs';$summary.Property(7)='x64;1033';$summary.Property(9)=([Guid]::NewGuid().ToString('B').ToUpper());$summary.Property(14)=200;$summary.Property(15)=2;$summary.Property(18)='OfficeTabs MSI build';$summary.Persist()
$db.Commit()
[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($db)
[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($engine)
Copy-Item -LiteralPath (Join-Path $scratch 'OfficeTabs.cab') -Destination (Join-Path $out 'OfficeTabs.cab') -Force
Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue
Write-Output 'Windows Installer package created.'
