[CmdletBinding()]
param([string]$Binary)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
foreach ($file in @('install-common.ps1','install.ps1','uninstall.ps1','verify-uiaccess.ps1','verify-elevated.ps1','handover.ps1','capture-inventory.ps1','compare-glaze.ps1')) {
    $tokens=$null; $errors=$null
    [void][Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $file),[ref]$tokens,[ref]$errors)
    if ($errors.Count) {throw ($errors | Out-String)}
}
. (Join-Path $PSScriptRoot 'install-common.ps1')
if (!$Binary) {$Binary=Join-Path (Get-MoonBuildDirectory debug) 'moon.exe'}
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-install-check-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
$certificate=$null
$registryPath='Software\moon-install-test-'+[guid]::NewGuid().ToString('N')
$registry=$null
$checks=[Collections.Generic.List[string]]::new()
try {
    $checks.Add('PowerShell 5.1 syntax')
    if ([MoonInstallNative]::PipeName() -notmatch '^moon-v1-S-1-5-5-[0-9]+-[0-9]+$') {throw 'Native logon identity format failed'}
    $checks.Add('Native token helper')
    $publication=[pscustomobject]@{revision=1}
    Write-MoonInstallState $temporary $publication
    $publication.revision=2; Write-MoonInstallState $temporary $publication
    if ((Get-Content -Raw -LiteralPath (Join-Path $temporary 'installation.json') | ConvertFrom-Json).revision -ne 2) {throw 'Atomic metadata replacement failed'}
    $checks.Add('Atomic installation metadata publication and replacement')
    $argumentScript=Join-Path $temporary 'arguments.ps1'
    [IO.File]::WriteAllText($argumentScript,'ConvertTo-Json -InputObject @($args) -Compress',[Text.UTF8Encoding]::new($false))
    $arguments=@('plain','two words','quote"inside','C:\trailing space\','unicode-한글','')
    $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $result=Invoke-MoonNative $powershell (@('-NoProfile','-ExecutionPolicy','Bypass','-File',$argumentScript)+$arguments)
    $actual=$result.Output | ConvertFrom-Json
    if ($actual.Count -ne $arguments.Count) {throw ('Native argument count changed: '+$result.Output)}
    for ($i=0;$i -lt $actual.Count;$i++) {if ($actual[$i] -cne $arguments[$i]) {throw "Native argument $i changed"}}
    $checks.Add('Native arguments round-trip quotes, spaces, trailing slashes, Unicode and empty strings')
    $manifest=[xml][MoonInstallNative]::Manifest((Resolve-Path -LiteralPath $Binary).ProviderPath)
    $level=$manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
    if (!$level -or $level.level -ne 'asInvoker') {throw 'PE manifest helper failed'}
    $checks.Add('Embedded manifest read without executing the image')
    $state=[pscustomobject]@{install_id=[guid]::NewGuid().ToString('N');owner_sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value}
    $target=Join-Path (Get-MoonInstallDirectory) 'moon.exe'
    $launch="Start-Process -FilePath '$target' -ArgumentList 'start --background' -WindowStyle Hidden"
    $encoded=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($launch))
    $task=[pscustomobject]@{Description="moon installation $($state.install_id); owner $($state.owner_sid)";Principal=[pscustomobject]@{RunLevel='Limited';UserId=$state.owner_sid};Actions=@([pscustomobject]@{Execute=$powershell;Arguments='-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand '+$encoded})}
    Assert-MoonOwnedTask $task $state $target
    $nativeTask=[pscustomobject]@{Description=$task.Description;Principal=(New-ScheduledTaskPrincipal -UserId $state.owner_sid -LogonType Interactive -RunLevel Limited);Actions=@(New-MoonTaskAction $target)}
    Assert-MoonOwnedTask $nativeTask $state $target
    $task.Principal.RunLevel='Highest'; $rejected=$false
    try {Assert-MoonOwnedTask $task $state $target} catch {$rejected=$true}
    if (!$rejected) {throw 'Elevated startup task was accepted'}
    $task.Principal.RunLevel='Limited';$task.Description='Unrelated task';$rejected=$false
    try {Assert-MoonOwnedTask $task $state $target} catch {$rejected=$true}
    if (!$rejected) {throw 'Unrelated startup task was accepted'}
    $checks.Add('Only owned limited-privilege startup tasks accepted')
    # Exercise the actual registry API in an isolated owned key. The user's
    # Environment key and process PATH are never written or broadcast here.
    $registry=[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($registryPath)
    $entry=Get-MoonInstallDirectory
    $raw='%USERPROFILE%\tools;C:\existing;;'
    $registry.SetValue('Path',$raw,[Microsoft.Win32.RegistryValueKind]::ExpandString)
    $addition=Get-MoonPathAddition (Read-MoonPathValue $registry) $entry $null
    Set-MoonPathChange $registry $addition
    $actual=Read-MoonPathValue $registry
    if ($actual.value -cne ($raw+$entry) -or $actual.kind -ne 'ExpandString') {throw 'PATH addition expanded or rewrote the existing registry value.'}
    Set-MoonPathChange $registry (Get-MoonPathRemoval $actual $addition.ownership)
    if ((Read-MoonPathValue $registry).value -cne $raw) {throw 'PATH removal did not preserve existing trailing separators.'}
    $checks.Add('PATH preserves raw expansions, value kind and trailing separators')
    Set-MoonPathChange $registry (Get-MoonPathAddition (Read-MoonPathValue $registry) $entry $null)
    $registry.SetValue('Path',($raw+$entry+';D:\later'),[Microsoft.Win32.RegistryValueKind]::String)
    Set-MoonPathChange $registry (Get-MoonPathRemoval (Read-MoonPathValue $registry) $addition.ownership)
    $actual=Read-MoonPathValue $registry
    if ($actual.value -cne ($raw+'D:\later') -or $actual.kind -ne 'String') {throw 'PATH removal clobbered a later user entry or registry-kind edit.'}
    $checks.Add('PATH removal preserves later user entries and registry-kind changes')
    $existing='"'+$entry.ToUpperInvariant()+'"'
    $registry.SetValue('Path',$existing,[Microsoft.Win32.RegistryValueKind]::String)
    $existingPlan=Get-MoonPathAddition (Read-MoonPathValue $registry) $entry $null
    if ($existingPlan.changed -or $existingPlan.ownership.owned) {throw 'Pre-existing equivalent PATH entry was claimed by the installer.'}
    Set-MoonPathChange $registry (Get-MoonPathRemoval (Read-MoonPathValue $registry) $existingPlan.ownership)
    if ((Read-MoonPathValue $registry).value -cne $existing) {throw 'Unowned PATH entry was removed.'}
    $checks.Add('Pre-existing equivalent PATH entries remain unowned and unchanged')
    $registry.DeleteValue('Path',$false)
    $missing=Get-MoonPathAddition (Read-MoonPathValue $registry) $entry $null
    Set-MoonPathChange $registry $missing
    Set-MoonPathChange $registry (Get-MoonPathRemoval (Read-MoonPathValue $registry) $missing.ownership)
    if ((Read-MoonPathValue $registry).exists) {throw 'An originally absent PATH value was not restored to absence.'}
    $checks.Add('Failed-first-install PATH cleanup restores an originally absent value')
    $registry.SetValue('Path','C:\before',[Microsoft.Win32.RegistryValueKind]::String)
    $stale=Get-MoonPathAddition (Read-MoonPathValue $registry) $entry $null
    $registry.SetValue('Path','D:\newer-user-edit',[Microsoft.Win32.RegistryValueKind]::String)
    $rejected=$false
    try {Set-MoonPathChange $registry $stale} catch {$rejected=$true}
    if (!$rejected -or (Read-MoonPathValue $registry).value -cne 'D:\newer-user-edit') {throw 'Concurrent PATH edit was overwritten.'}
    $checks.Add('A newer PATH edit rejects a stale installer write')
    $registry.SetValue('Path',('C:\before;'+$entry+';'+$entry+';E:\after'),[Microsoft.Win32.RegistryValueKind]::String)
    Set-MoonPathChange $registry (Get-MoonPathRemoval (Read-MoonPathValue $registry) $stale.ownership)
    if ((Read-MoonPathValue $registry).value -cne ('C:\before;'+$entry+';E:\after')) {throw 'PATH removal removed more than one owned token.'}
    $checks.Add('PATH cleanup removes one owned token and preserves duplicates added later')
    # Isolated, untrusted certificate: no machine root or publisher store is changed.
    $certificate=New-SelfSignedCertificate -Subject "CN=moon code signing $($state.install_id)" -Type CodeSigningCert -CertStoreLocation Cert:\CurrentUser\My -Provider 'Microsoft Software Key Storage Provider' -KeyAlgorithm RSA -KeyLength 2048 -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -KeyUsage DigitalSignature -NotAfter (Get-Date).AddDays(1)
    Assert-MoonCertificate $certificate $state.install_id
    $checks.Add('Actual code-signing-only, non-exportable CNG certificate accepted')
    $rejected=$false
    try {Assert-MoonCertificate $certificate ([guid]::NewGuid().ToString('N'))} catch {$rejected=$true}
    if (!$rejected) {throw 'Foreign signing certificate was accepted'}
    $checks.Add('Unrelated signing identity refused')
    [pscustomobject]@{passed=$checks.Count;checks=$checks.ToArray();manifest_ui_access=$level.uiAccess;machine_trust_changed=$false;scheduled_tasks_changed=$false;user_path_changed=$false} | ConvertTo-Json -Depth 6
} finally {
    if ($registry) {$registry.Dispose(); [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($registryPath,$false)}
    if ($certificate) {Remove-Item -LiteralPath ('Cert:\CurrentUser\My\'+$certificate.Thumbprint) -DeleteKey -Force}
    if (Test-Path -LiteralPath $temporary) {Remove-Item -LiteralPath $temporary -Recurse -Force}
}
