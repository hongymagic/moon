[CmdletBinding()]
param([string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\explorer-preservation-preflight.json'))
. (Join-Path $PSScriptRoot 'install-common.ps1')
& (Join-Path $PSScriptRoot 'application-integration.ps1') -CompileOnly | Out-Null
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class MoonExplorerPreflightNative {
    [DllImport("user32.dll")] public static extern IntPtr GetShellWindow();
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string name,string title);
}
'@
$session=[Diagnostics.Process]::GetCurrentProcess().SessionId
$shellWindow=[MoonExplorerPreflightNative]::GetShellWindow().ToInt64()
if (!$shellWindow) {throw 'There is no desktop shell window; no restart should be attempted.'}
$desktopProcessId=[MoonApplicationNative]::ProcessId($shellWindow)
$desktopProcessIdentity=[MoonInstallNative]::ProcessIdentity($desktopProcessId)
if (![string]::Equals($desktopProcessIdentity.Path,(Join-Path $env:SystemRoot 'explorer.exe'),[StringComparison]::OrdinalIgnoreCase)) {throw 'The desktop shell does not belong to the expected Explorer image.'}
$taskbar=[MoonExplorerPreflightNative]::FindWindow('Shell_TrayWnd',$null).ToInt64()
if (!$taskbar -or [MoonApplicationNative]::ProcessId($taskbar) -ne $desktopProcessId) {throw 'Desktop and taskbar process ownership do not agree.'}
$explorers=@(Get-CimInstance Win32_Process -Filter "Name='explorer.exe'" | Where-Object SessionId -eq $session | ForEach-Object {
    $identity=[MoonInstallNative]::ProcessIdentity([uint32]$_.ProcessId)
    [pscustomobject]@{process_id=$_.ProcessId;birth=$identity.Birth;path=$identity.Path}
})
$shellClasses=@('Progman','WorkerW','Shell_TrayWnd','Shell_SecondaryTrayWnd')
$applicationWindows=@([MoonApplicationNative]::Windows() | Where-Object {$explorers.process_id -contains $_.ProcessId -and $shellClasses -notcontains $_.Class} | ForEach-Object {
    [pscustomobject]@{id=$_.Id;process_id=$_.ProcessId;class=$_.Class;title=$_.Title;shares_shell_process=($_.ProcessId -eq $desktopProcessId)}
})
$report=[pscustomobject]@{
    recorded_at=[DateTime]::UtcNow.ToString('o')
    shell=[pscustomobject]@{process_id=$desktopProcessId;birth=$desktopProcessIdentity.Birth;path=$desktopProcessIdentity.Path;desktop_window=$shellWindow;taskbar_window=$taskbar}
    explorer_processes=$explorers
    application_windows=$applicationWindows
    shell_has_application_windows=(@($applicationWindows | Where-Object shares_shell_process).Count -gt 0)
    any_explorer_application_windows=($applicationWindows.Count -gt 0)
    restart_attempted=$false
    desktop_changed=$false
}
[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
[IO.File]::WriteAllText($ReportPath,($report | ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
$report | ConvertTo-Json -Depth 6
