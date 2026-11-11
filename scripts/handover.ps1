[CmdletBinding()]
param(
    [string]$Binary,
    [string]$InventoryPath,
    [string]$RecoveryPath,
    [switch]$Rollback,
    [string]$StatePath=(Join-Path $env:LOCALAPPDATA 'moon\handover.json')
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
if ((Test-MoonAdministrator) -or [MoonInstallNative]::Integrity() -ne 8192) {
    throw 'Run handover from ordinary PowerShell so restored desktop applications keep ordinary privileges. The installer requests its own elevation.'
}
$session=[Diagnostics.Process]::GetCurrentProcess().SessionId
$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$names=@('glazewm.exe','zebar.exe','tacky-borders.exe')
if (!$InventoryPath) {$InventoryPath=Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-msvc\release\examples\window_inventory.exe'}
if (!$RecoveryPath) {$RecoveryPath=Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-msvc\release\examples\recover_windows.exe'}
function Get-PreviousDesktopProcesses {
    @(Get-CimInstance Win32_Process | Where-Object {$_.SessionId -eq $session -and $names -contains $_.Name.ToLowerInvariant()})
}
function Assert-MoonStopped {
    Wait-MoonCondition -Description 'moon and its recovery watchdog to exit' -Condition {
        !(Get-MoonDaemon) -and @((Get-CimInstance Win32_Process) | Where-Object {$_.SessionId -eq $session -and $_.Name -eq 'moon.exe'}).Count -eq 0
    }
    if (Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA ('moon\journal-v1-session'+$session+'.json'))) {
        throw 'A moon recovery journal remains; recover those windows before restarting the previous manager.'
    }
}
function Restore-PreviousDesktop($Saved) {
    Assert-MoonStopped
    foreach ($link in $Saved.startup_links) {
        if (!(Test-Path -LiteralPath $link.backup) -or (Get-FileHash -LiteralPath $link.backup -Algorithm SHA256).Hash -ne $link.sha256) {throw 'The saved startup shortcut failed its integrity check.'}
        if (Test-Path -LiteralPath $link.original) {
            if ((Get-FileHash -LiteralPath $link.original -Algorithm SHA256).Hash -ne $link.sha256) {throw "A new shortcut occupies $($link.original); it was not replaced."}
        } else {Copy-Item -LiteralPath $link.backup -Destination $link.original}
    }
    # Glaze may start its configured bars itself. Wait briefly before restarting
    # any originally running companion that is still absent.
    foreach ($name in $names) {
        $savedProcess=@($Saved.processes | Where-Object {$_.name -eq $name})
        $savedLink=@($Saved.startup_links | Where-Object {$_.program -eq $name})
        if ($savedProcess.Count -eq 0 -and $savedLink.Count -eq 0) {continue}
        if (@(Get-PreviousDesktopProcesses | Where-Object {$_.Name -eq $name}).Count -eq 0) {
            if ($savedProcess.Count -gt 0) {
                $program=$savedProcess[0]
                if (!(Test-Path -LiteralPath $program.path)) {throw "Previous desktop application is missing: $($program.path)"}
                $start=@{FilePath=$program.path;WorkingDirectory=[IO.Path]::GetDirectoryName($program.path)}
                if ($program.arguments) {$start.ArgumentList=$program.arguments}
                Start-Process @start
            } else {
                # Validation may have already stopped the previous manager.
                # Its saved startup shortcut still records the desktop to return to.
                Start-Process -FilePath $savedLink[0].original
            }
            Wait-MoonCondition -Description "previous desktop application $name" -Condition {@(Get-PreviousDesktopProcesses | Where-Object {$_.Name -eq $name}).Count -gt 0}
            if ($name -eq 'glazewm.exe') {Start-Sleep -Seconds 2}
        }
    }
}

if ($Rollback) {
    $saved=Get-Content -Raw -LiteralPath $StatePath | ConvertFrom-Json
    if ($saved.version -ne 1 -or $saved.owner_sid -ne $owner -or $saved.session -ne $session) {throw 'The handover record belongs to another user or session.'}
    $installation=Get-MoonInstallDirectory
    if (Test-Path -LiteralPath (Join-Path $installation 'installation.json')) {
        Invoke-MoonNative $powershell @('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot 'uninstall.ps1')) -TimeoutSeconds 0 | Out-Null
    }
    Restore-PreviousDesktop $saved
    $saved.status='rolled_back'; $saved.finished_at=[DateTime]::UtcNow.ToString('o')
    Write-MoonJsonFile $StatePath $saved
    $saved | ConvertTo-Json -Depth 10
    return
}

if (!$Binary) {throw 'Provide the UIAccess release executable with -Binary.'}
$Binary=(Resolve-Path -LiteralPath $Binary).ProviderPath
$manifest=[xml][MoonInstallNative]::Manifest($Binary)
$execution=$manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
if (!$execution -or $execution.uiAccess -ne 'true' -or $execution.level -ne 'asInvoker') {throw 'Handover requires the UIAccess release build.'}
if (Get-MoonDaemon) {throw 'Exit the development daemon before handing over the whole desktop.'}
Assert-MoonStopped
if (Test-Path -LiteralPath (Join-Path (Get-MoonInstallDirectory) 'installation.json')) {throw 'moon is already installed. Use install.ps1 for an upgrade or -Rollback to restore the previous desktop.'}
if (Test-Path -LiteralPath $StatePath) {
    $existing=Get-Content -Raw -LiteralPath $StatePath | ConvertFrom-Json
    if ($existing.status -ne 'rolled_back') {throw 'An earlier handover record exists. Use -Rollback to restore it, or use install.ps1 for an upgrade.'}
}
$directory=Join-Path $env:LOCALAPPDATA ('moon\handover-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($directory)
$processes=@(Get-PreviousDesktopProcesses | ForEach-Object {
    if (!$_.ExecutablePath -or !$_.CommandLine -or $_.CommandLine -notmatch '^(?:"[^"]*"|\S+)\s*(.*)$') {throw "Cannot safely preserve launch information for $($_.Name)."}
    [pscustomobject]@{name=$_.Name.ToLowerInvariant();path=$_.ExecutablePath;arguments=$Matches[1];process_id=$_.ProcessId}
})
$glaze=@($processes | Where-Object {$_.name -eq 'glazewm.exe'})
$glazeHandles=@(); $glazeInventoryPath=$null
if ($glaze.Count -gt 0) {
    $InventoryPath=(Resolve-Path -LiteralPath $InventoryPath).ProviderPath
    $RecoveryPath=(Resolve-Path -LiteralPath $RecoveryPath).ProviderPath
    $cli=Join-Path ([IO.Path]::GetDirectoryName($glaze[0].path)) 'cli\glazewm.exe'
    $query=(Invoke-MoonNative $cli @('query','windows')).Output | ConvertFrom-Json
    if (!$query.success) {throw 'Glaze did not provide its exact owned window set; no startup or window state was changed.'}
    $inventory=(Invoke-MoonNative $InventoryPath).Output | ConvertFrom-Json
    foreach ($window in $query.data.windows) {
        $identity=@($inventory.windows | Where-Object {$_.window.id -eq $window.handle})
        if ($identity.Count -eq 1) {$glazeHandles+=[string]$window.handle}
        elseif ($window.state.type -ne 'minimized') {throw "Glaze window $($window.handle) is absent from the native identity snapshot; handover stopped before exit."}
    }
    $glazeInventoryPath=Join-Path $directory 'glaze-owned-window-inventory.json'
    Write-MoonJsonFile $glazeInventoryPath ([pscustomobject]@{native=$inventory;glaze=$query})
}
$links=@()
$shell=New-Object -ComObject WScript.Shell
try {
    foreach ($file in @(Get-ChildItem -LiteralPath ([Environment]::GetFolderPath('Startup')) -Filter '*.lnk')) {
        $shortcut=$shell.CreateShortcut($file.FullName)
        if ($names -contains [IO.Path]::GetFileName($shortcut.TargetPath).ToLowerInvariant()) {
            $backup=Join-Path $directory $file.Name
            Copy-Item -LiteralPath $file.FullName -Destination $backup
            $links+=[pscustomobject]@{original=$file.FullName;backup=$backup;program=[IO.Path]::GetFileName($shortcut.TargetPath).ToLowerInvariant();sha256=(Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash}
        }
    }
} finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)}
$configs=@()
foreach ($relative in @('.glzr\glazewm\config.yaml','.glzr\zebar\settings.json','.config\tacky-borders\config.yaml')) {
    $original=Join-Path $env:USERPROFILE $relative
    if (Test-Path -LiteralPath $original) {
        $backup=Join-Path $directory ($relative.Replace('\','_'))
        Copy-Item -LiteralPath $original -Destination $backup
        $configs+=[pscustomobject]@{original=$original;backup=$backup;sha256=(Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash}
    }
}
$saved=[pscustomobject]@{version=1;owner_sid=$owner;session=$session;started_at=[DateTime]::UtcNow.ToString('o');finished_at=$null;status='prepared';source_sha256=(Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash;processes=$processes;startup_links=$links;config_backups=$configs;glaze_inventory=$glazeInventoryPath;glaze_owned_handles=$glazeHandles;glaze_recovery=$null;installed_stats=$null;uiaccess_report=$null}
[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($StatePath)))
Write-MoonJsonFile $StatePath $saved
try {
    foreach ($link in $links) {Remove-Item -LiteralPath $link.original}
    $saved.status='previous_startup_disabled'; Write-MoonJsonFile $StatePath $saved
    if ($glaze.Count -gt 0) {
        $cli=Join-Path ([IO.Path]::GetDirectoryName($glaze[0].path)) 'cli\glazewm.exe'
        if (!(Test-Path -LiteralPath $cli)) {throw 'The GlazeWM command client is missing; the existing manager was not terminated.'}
        $reply=(Invoke-MoonNative $cli @('command','wm-exit')).Output | ConvertFrom-Json
        if (!$reply.success) {throw 'GlazeWM refused its normal exit command.'}
        Wait-MoonCondition -Description 'GlazeWM normal exit' -Condition {@(Get-PreviousDesktopProcesses | Where-Object {$_.Name -eq 'glazewm.exe'}).Count -eq 0}
        if ($glazeHandles.Count) {
            # Glaze shell cloaks can outlive even its normal wm-exit command.
            # Recover only its recorded HWND/PID/process/class identities, never
            # every cloaked shell/UWP window discovered on the desktop.
            $saved.glaze_recovery=(Invoke-MoonNative $RecoveryPath (@($glazeInventoryPath)+$glazeHandles)).Output | ConvertFrom-Json
            Write-MoonJsonFile $StatePath $saved
        }
    }
    foreach ($process in @(Get-PreviousDesktopProcesses | Where-Object {$_.Name -ne 'glazewm.exe'})) {
        # These processes only draw the old bar and borders. Match the captured
        # PID and image before stopping a companion that Glaze did not close.
        $expected=@($processes | Where-Object {$_.process_id -eq $process.ProcessId -and $_.path -eq $process.ExecutablePath})
        if ($expected.Count -eq 0) {throw 'A previous desktop companion changed during handover; it was not stopped.'}
        Stop-Process -Id $process.ProcessId
    }
    Wait-MoonCondition -Description 'the previous desktop components to exit' -Condition {@(Get-PreviousDesktopProcesses).Count -eq 0}
    $saved.status='previous_stopped'; Write-MoonJsonFile $StatePath $saved
    # Installation owns its elevation prompt and must finish or roll back itself;
    # killing its waiting parent would abandon the elevated transaction.
    Invoke-MoonNative $powershell @('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot 'install.ps1'),'-Binary',$Binary,'-ExpectedSha256',$saved.source_sha256) -TimeoutSeconds 0 | Out-Null
    $saved.installed_stats=Invoke-MoonRpc query stats
    $report=Join-Path $directory 'uiaccess-results.json'
    $verification=Invoke-MoonNative $powershell @('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot 'verify-uiaccess.ps1'),'-ReportPath',$report) -TimeoutSeconds 60
    $saved.uiaccess_report=$report
    $saved.status='installed'; $saved.finished_at=[DateTime]::UtcNow.ToString('o')
    Write-MoonJsonFile $StatePath $saved
    $saved | ConvertTo-Json -Depth 10
} catch {
    $failure=$_
    $saved.status='recovery_required'; Write-MoonJsonFile $StatePath $saved
    $daemon=Get-MoonDaemon
    if ($daemon) {Invoke-MoonRpc command -Commands @('exit') | Out-Null}
    if (Test-Path -LiteralPath (Join-Path (Get-MoonInstallDirectory) 'installation.json')) {
        Invoke-MoonNative $powershell @('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot 'uninstall.ps1')) -TimeoutSeconds 0 | Out-Null
    }
    Restore-PreviousDesktop $saved
    $saved.status='rolled_back'; $saved.finished_at=[DateTime]::UtcNow.ToString('o')
    Write-MoonJsonFile $StatePath $saved
    throw $failure
}
