[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$ReceiptPath,
    [string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\previous-desktop-restored.json')
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
if ((Test-MoonAdministrator) -or @(Get-Process -Name moon,testwin -ErrorAction SilentlyContinue).Count) {
    throw 'Restore the previous desktop from an ordinary session after all moon probes have exited.'
}
$saved=Get-Content -Raw -LiteralPath $ReceiptPath | ConvertFrom-Json
if (!$saved.passed -or !$saved.startup_shortcuts_unchanged) {throw 'A verified previous-desktop shutdown receipt is required.'}
$glaze=@($saved.stopped_processes | Where-Object name -eq 'glazewm.exe')
if ($glaze.Count -ne 1) {throw 'The receipt must identify exactly one previous GlazeWM executable.'}
$shell=New-Object -ComObject WScript.Shell
$launchShortcuts=@()
try {
    foreach ($entry in $saved.startup_shortcuts) {
        if ((Get-FileHash -LiteralPath $entry.path -Algorithm SHA256).Hash -ne $entry.sha256) {throw 'A startup shortcut changed after shutdown; it was not launched.'}
        $shortcut=$shell.CreateShortcut($entry.path)
        try {
            if ([string]::Equals($shortcut.TargetPath,$glaze[0].path,[StringComparison]::OrdinalIgnoreCase)) {$launchShortcuts+= $entry.path}
        } finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shortcut)}
    }
} finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)}
if ($launchShortcuts.Count -ne 1) {throw 'The unchanged startup shortcut does not identify the previous GlazeWM executable uniquely.'}
$session=[Diagnostics.Process]::GetCurrentProcess().SessionId
function Get-PreviousComponents {
    @(Get-CimInstance Win32_Process | Where-Object {$_.SessionId -eq $session -and $_.Name -in @('glazewm.exe','zebar.exe','tacky-borders.exe')})
}
if (@(Get-PreviousComponents | Where-Object Name -eq 'glazewm.exe').Count -eq 0) {
    Start-Process -FilePath $launchShortcuts[0]
}
$identities=@()
Wait-MoonCondition -Description 'the previous desktop components to resume through their unchanged startup shortcut' -Condition {
    $running=Get-PreviousComponents
    foreach ($expected in $saved.stopped_processes) {
        $found=@($running | Where-Object Name -eq $expected.name)
        if ($found.Count -ne 1) {return $false}
        $identity=[MoonInstallNative]::ProcessIdentity([uint32]$found[0].ProcessId)
        if (![string]::Equals($identity.Path,$expected.path,[StringComparison]::OrdinalIgnoreCase)) {throw 'A desktop component has an unexpected executable path.'}
    }
    return $true
}
foreach ($component in Get-PreviousComponents) {
    $identity=[MoonInstallNative]::ProcessIdentity([uint32]$component.ProcessId)
    $identities+=[pscustomobject]@{name=$component.Name;process_id=$component.ProcessId;birth=$identity.Birth;path=$identity.Path}
}
$report=[pscustomobject]@{recorded_at=[DateTime]::UtcNow.ToString('o');passed=$true;startup_shortcut=$launchShortcuts[0];components=$identities;receipt=$ReceiptPath;startup_shortcuts_unchanged=$true;uac_requested=$false}
[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
$report | ConvertTo-Json -Depth 6
