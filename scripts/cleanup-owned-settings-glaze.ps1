[CmdletBinding()]
param(
    [string]$RecoveryPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\owned-settings-recovery.json'),
    [string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\owned-settings-glaze-cleanup.json')
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
& (Join-Path $PSScriptRoot 'application-integration.ps1') -CompileOnly | Out-Null
$saved=Get-Content -Raw -LiteralPath $RecoveryPath | ConvertFrom-Json
$path=Join-Path $env:ProgramFiles 'PowerToys\WinUI3Apps\PowerToys.Settings.exe'
if ($saved.class -ne 'WinUIDesktopWin32WindowClass' -or ![string]::Equals($saved.path,$path,[StringComparison]::OrdinalIgnoreCase)) {throw 'Unexpected test recovery record.'}
function Assert-OwnedSettings {
    $window=@([MoonApplicationNative]::Windows() | Where-Object {$_.Id -eq $saved.id})
    if ($window.Count -ne 1 -or $window[0].ProcessId -ne $saved.process_id -or $window[0].Class -ne $saved.class -or $window[0].Title -ne $saved.title -or [MoonApplicationNative]::Birth([uint32]$saved.process_id) -ne $saved.process_birth -or ![string]::Equals([MoonApplicationNative]::ImagePath([uint32]$saved.process_id),$path,[StringComparison]::OrdinalIgnoreCase)) {throw 'The test window identity changed; nothing was closed.'}
}
Assert-OwnedSettings
$cli=Join-Path $env:ProgramFiles 'glzr.io\GlazeWM\cli\glazewm.exe'
$help=(Invoke-MoonNative $cli @('command','--help')).Output
if ($help -notmatch '--id <SUBJECT_CONTAINER_ID>' -or $help -notmatch '(?m)^\s+close\s') {throw 'Installed Glaze CLI does not expose the verified targeted close operation.'}
$before=(Invoke-MoonNative $cli @('query','windows')).Output | ConvertFrom-Json
if (!$before.success) {throw 'Glaze query failed.'}
$target=@($before.data.windows | Where-Object {$_.handle -eq $saved.id -and $_.className -eq $saved.class -and $_.title -eq $saved.title})
if ($target.Count -ne 1) {throw 'The test window does not map to exactly one Glaze container.'}
$container=[guid]::Parse($target[0].id).ToString()
$others=@($before.data.windows | Where-Object {$_.handle -ne $saved.id} | ForEach-Object {
    $owner=[MoonApplicationNative]::ProcessId([long]$_.handle)
    [pscustomobject]@{id=$_.id;handle=$_.handle;process_id=$owner;process_birth=[MoonApplicationNative]::Birth($owner)}
})
$runnerBirth=[MoonApplicationNative]::Birth([uint32]$saved.parent_process_id)
Assert-OwnedSettings
$reply=(Invoke-MoonNative $cli @('command','--id',$container,'close')).Output | ConvertFrom-Json
if (!$reply.success) {throw ('Glaze refused targeted close: '+$reply.error)}
Wait-MoonCondition -Description 'the owned Settings window to close through its exact Glaze container' -Seconds 10 -Condition {@([MoonApplicationNative]::Windows() | Where-Object Id -eq $saved.id).Count -eq 0}
foreach($other in $others) {
    if (![MoonElevatedNative]::IsWindow([IntPtr][long]$other.handle) -or [MoonApplicationNative]::ProcessId([long]$other.handle) -ne $other.process_id -or [MoonApplicationNative]::Birth([uint32]$other.process_id) -ne $other.process_birth) {throw 'Another managed window identity changed during cleanup.'}
}
if ([MoonApplicationNative]::Birth([uint32]$saved.parent_process_id) -ne $runnerBirth) {throw 'The preexisting PowerToys runner identity changed.'}
$report=[pscustomobject]@{recorded_at=[DateTime]::UtcNow.ToString('o');passed=$true;method='GlazeWM command --id <exact-container-uuid> close';glaze_version=(Invoke-MoonNative $cli @('--version')).Output.Trim();container_id=$container;closed_hwnd=$saved.id;process_id=$saved.process_id;process_birth=$saved.process_birth;window_destroyed=(![MoonElevatedNative]::IsWindow([IntPtr][long]$saved.id));window_visible=$false;other_window_identities_preserved=$others;preexisting_runner_preserved=$true;runner_process_id=$saved.parent_process_id;uac_requested=$false}
[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 10),[Text.UTF8Encoding]::new($false))
$report | ConvertTo-Json -Depth 10
