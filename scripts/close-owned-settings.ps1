[CmdletBinding()]
param(
    [string]$RecoveryPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\owned-settings-recovery.json'),
    [string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\owned-settings-cleanup.json')
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
& (Join-Path $PSScriptRoot 'application-integration.ps1') -CompileOnly | Out-Null
$saved=Get-Content -Raw -LiteralPath $RecoveryPath | ConvertFrom-Json
$expectedPath=Join-Path $env:ProgramFiles 'PowerToys\WinUI3Apps\PowerToys.Settings.exe'
if ($saved.class -ne 'WinUIDesktopWin32WindowClass' -or ![string]::Equals($saved.path,$expectedPath,[StringComparison]::OrdinalIgnoreCase)) {throw 'The recovery record is not the owned PowerToys Settings test window.'}
$window=@([MoonApplicationNative]::Windows() | Where-Object {$_.Id -eq $saved.id})
if ($window.Count -ne 1 -or $window[0].ProcessId -ne $saved.process_id -or $window[0].Class -ne $saved.class -or $window[0].Title -ne $saved.title -or [MoonApplicationNative]::Birth([uint32]$saved.process_id) -ne $saved.process_birth -or ![string]::Equals([MoonApplicationNative]::ImagePath([uint32]$saved.process_id),$expectedPath,[StringComparison]::OrdinalIgnoreCase)) {throw 'The saved test window identity changed; no close command was sent.'}
$runnerBirth=[MoonApplicationNative]::Birth([uint32]$saved.parent_process_id)
$daemon=Invoke-MoonRpc query stats -Installed
$target=Join-Path (Get-MoonInstallDirectory) 'moon.exe'
if (!$daemon.token.ui_access -or ![string]::Equals($daemon.executable,$target,[StringComparison]::OrdinalIgnoreCase)) {throw 'Cleanup requires the signed installed UIAccess daemon.'}
if ([MoonElevatedNative]::GetForegroundWindow().ToInt64() -ne $saved.id) {
    if (![MoonElevatedNative]::SetForegroundWindow([IntPtr][long]$saved.id)) {throw 'Windows did not select the owned Settings window; no close command was sent.'}
}
Wait-MoonCondition -Description 'the owned Settings window to be the native and managed foreground' -Seconds 3 -Condition {
    [MoonElevatedNative]::GetForegroundWindow().ToInt64() -eq $saved.id -and (Invoke-MoonRpc query world -ExpectedPeer $daemon.pipe_peer).focused -eq $saved.id
}
if ([MoonApplicationNative]::ProcessId([long]$saved.id) -ne $saved.process_id -or [MoonApplicationNative]::Birth([uint32]$saved.process_id) -ne $saved.process_birth) {throw 'The test window identity changed before closing.'}
Invoke-MoonRpc command -ExpectedPeer $daemon.pipe_peer -Commands @('close') -ExpectedFocus ([long]$saved.id) | Out-Null
Wait-MoonCondition -Description 'the exact owned Settings window to close' -Seconds 10 -Condition {@([MoonApplicationNative]::Windows() | Where-Object Id -eq $saved.id).Count -eq 0}
if ([MoonApplicationNative]::Birth([uint32]$saved.parent_process_id) -ne $runnerBirth) {throw 'The preexisting PowerToys runner identity changed.'}
$report=[pscustomobject]@{recorded_at=[DateTime]::UtcNow.ToString('o');passed=$true;closed_hwnd=$saved.id;window_destroyed=(![MoonElevatedNative]::IsWindow([IntPtr][long]$saved.id));window_visible=$false;process_id=$saved.process_id;process_birth=$saved.process_birth;expected_focus_precondition=$true;daemon_uiaccess=$daemon.token.ui_access;preexisting_runner_preserved=$true;runner_process_id=$saved.parent_process_id}
[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
[IO.File]::WriteAllText($ReportPath,($report | ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
$report | ConvertTo-Json -Depth 8
