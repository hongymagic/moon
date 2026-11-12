[CmdletBinding()]
param(
    [string]$InventoryPath=(Join-Path $env:LOCALAPPDATA 'moon-install-candidate\window_inventory.exe'),
    [string]$RecoveryPath=(Join-Path $env:LOCALAPPDATA 'moon-install-candidate\recover_windows.exe'),
    [string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\previous-desktop-stop.json')
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
& (Join-Path $PSScriptRoot 'application-integration.ps1') -CompileOnly | Out-Null
if ((Test-MoonAdministrator) -or @(Get-Process -Name moon,testwin -ErrorAction SilentlyContinue).Count) {throw 'Stop the previous desktop only from ordinary PowerShell with no active moon/testwin probe.'}
$InventoryPath=(Resolve-Path -LiteralPath $InventoryPath).ProviderPath
$RecoveryPath=(Resolve-Path -LiteralPath $RecoveryPath).ProviderPath
$session=[Diagnostics.Process]::GetCurrentProcess().SessionId
$names=@('glazewm.exe','zebar.exe','tacky-borders.exe')
$processes=@(Get-CimInstance Win32_Process | Where-Object {$_.SessionId -eq $session -and $names -contains $_.Name.ToLowerInvariant()} | ForEach-Object {
    [pscustomobject]@{name=$_.Name;id=$_.ProcessId;birth=[MoonApplicationNative]::Birth([uint32]$_.ProcessId);path=[MoonApplicationNative]::ImagePath([uint32]$_.ProcessId)}
})
$glaze=@($processes | Where-Object name -eq 'glazewm.exe')
if ($glaze.Count -ne 1) {throw 'Expected exactly one existing GlazeWM manager.'}
$expectedGlaze=Join-Path $env:ProgramFiles 'glzr.io\GlazeWM\glazewm.exe'
if (![string]::Equals($glaze[0].path,$expectedGlaze,[StringComparison]::OrdinalIgnoreCase)) {throw 'Unexpected GlazeWM image path.'}
$cli=Join-Path ([IO.Path]::GetDirectoryName($expectedGlaze)) 'cli\glazewm.exe'
$startup=@(Get-ChildItem -LiteralPath ([Environment]::GetFolderPath('Startup')) -Filter '*.lnk' | ForEach-Object {[pscustomobject]@{path=$_.FullName;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}})
$query=(Invoke-MoonNative $cli @('query','windows')).Output | ConvertFrom-Json
if (!$query.success) {throw 'Glaze did not return its owned windows.'}
$inventory=(Invoke-MoonNative $InventoryPath).Output | ConvertFrom-Json
$handles=@()
foreach($window in $query.data.windows) {
    $native=@($inventory.windows | Where-Object {$_.window.id -eq $window.handle})
    if ($native.Count -eq 1) {$handles += [string]$window.handle}
    elseif ($window.state.type -ne 'minimized') {throw 'A non-minimized Glaze window is absent from the native identity snapshot.'}
}
$source=Join-Path $env:LOCALAPPDATA ('moon-dev\glaze-stop-identities-'+[guid]::NewGuid().ToString('N')+'.json')
[IO.File]::WriteAllText($source,([pscustomobject]@{native=$inventory;glaze=$query;processes=$processes;startup=$startup}|ConvertTo-Json -Depth 40),[Text.UTF8Encoding]::new($false))
if ([MoonApplicationNative]::Birth([uint32]$glaze[0].id) -ne $glaze[0].birth) {throw 'Glaze process identity changed before normal exit.'}
$exitReply=(Invoke-MoonNative $cli @('command','wm-exit')).Output | ConvertFrom-Json
if (!$exitReply.success) {throw 'Glaze refused normal exit; recovery evidence was retained.'}
Wait-MoonCondition -Description 'the captured GlazeWM process to exit' -Condition {!(Get-Process -Id $glaze[0].id -ErrorAction SilentlyContinue)}
$recovered=if($handles.Count){(Invoke-MoonNative $RecoveryPath (@($source)+$handles)).Output | ConvertFrom-Json}else{@()}
foreach($process in @($processes | Where-Object name -ne 'glazewm.exe')) {
    if (Get-Process -Id $process.id -ErrorAction SilentlyContinue) {
        if ([MoonApplicationNative]::Birth([uint32]$process.id) -ne $process.birth -or ![string]::Equals([MoonApplicationNative]::ImagePath([uint32]$process.id),$process.path,[StringComparison]::OrdinalIgnoreCase)) {throw 'A companion identity changed; it was not stopped.'}
        Stop-Process -Id $process.id
    }
}
Wait-MoonCondition -Description 'the captured desktop components to exit' -Condition {@(Get-CimInstance Win32_Process | Where-Object {$_.SessionId -eq $session -and $names -contains $_.Name.ToLowerInvariant()}).Count -eq 0}
foreach($shortcut in $startup) {if((Get-FileHash -LiteralPath $shortcut.path -Algorithm SHA256).Hash -ne $shortcut.sha256){throw 'A startup shortcut changed during desktop shutdown.'}}
$after=(Invoke-MoonNative $InventoryPath).Output | ConvertFrom-Json
foreach($handle in $handles) {
    $original=@($inventory.windows | Where-Object {$_.window.id -eq [long]$handle})[0]
    $current=@($after.windows | Where-Object {$_.window.id -eq [long]$handle -and $_.window.process_id -eq $original.window.process_id -and $_.process_birth -eq $original.process_birth})
    if($current.Count -ne 1 -or $current[0].cloaked -ne 0){throw 'A captured Glaze-owned window did not return uncloaked with its original identity.'}
}
$report=[pscustomobject]@{recorded_at=[DateTime]::UtcNow.ToString('o');passed=$true;stopped_processes=$processes;recovery_source=$source;restored_owned_handles=$handles;recovery=$recovered;startup_shortcuts_unchanged=$true;startup_shortcuts=$startup;uac_requested=$false}
[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 40),[Text.UTF8Encoding]::new($false))
$report | ConvertTo-Json -Depth 40
