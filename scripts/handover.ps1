[CmdletBinding()]
param(
    [string]$Binary,
    [string]$SettingsBinary,
    [string]$InventoryPath,
    [string]$RecoveryPath,
    [switch]$Rollback,
    [switch]$KeepPreviousDesktopStopped,
    [string]$StatePath=(Join-Path $env:LOCALAPPDATA 'moon\handover.json'),
    [string]$OperationId
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
if (!$OperationId) {$OperationId=[guid]::NewGuid().ToString('N')}
trap {
    $failure=$_
    $current=$null
    try {if(Test-Path -LiteralPath $StatePath){$current=Get-Content -Raw -LiteralPath $StatePath|ConvertFrom-Json}} catch { }
    $currentStatus=if($current -and $current.PSObject.Properties['status']){$current.status}else{''}
    $restored=$currentStatus -in @('rolled_back','rolled_back_stopped')
    $operationState=$null
    try {$operationState=Read-MoonSetupStatus $OperationId} catch { }
    # The elevated installer publishes its full error before its parent receives
    # only an exit code. Retain that cause when handover reports its own failure.
    $priorStatus='';$priorDetail=''
    if($operationState -and $operationState.PSObject.Properties['operation'] -and $operationState.operation -eq $OperationId){
        if($operationState.PSObject.Properties['status']){$priorStatus=[string]$operationState.status}
        if($operationState.PSObject.Properties['detail'] -and $operationState.detail -is [string]){$priorDetail=$operationState.detail}
    }
    $detail=($failure|Out-String).TrimEnd()
    if(![string]::IsNullOrWhiteSpace($priorDetail)){$detail=$priorDetail+"`r`n`r`nHandover failure:`r`n"+$detail}
    $detail+="`r`n`r`nSetup operation: $OperationId"
    try {$detail+="`r`nDiagnostic status: $((Get-MoonSetupFiles $OperationId).status)"} catch { }
    $cancelled=$restored -and ($failure.Exception -is [OperationCanceledException] -or $priorStatus -eq 'cancelled')
    Write-MoonSetupStatus $OperationId $(if($cancelled){'cancelled'}elseif($currentStatus -eq 'recovery_required'){'recovery_required'}else{'failed'}) $(if($currentStatus -eq 'rolled_back_stopped'){'Moon setup was rolled back. Your previous desktop remains stopped.'}elseif($restored){'Your previous desktop has been restored.'}else{'Setup could not finish. Open details before trying again.'}) $detail
    Write-Error -Message $detail -ErrorAction Continue
    exit $(if($cancelled){1223}else{1})
}
if ((Test-MoonAdministrator) -or [MoonInstallNative]::Integrity() -ne 8192) {
    throw 'Run handover from ordinary PowerShell so restored desktop applications keep ordinary privileges. The installer requests its own elevation.'
}
$session=[Diagnostics.Process]::GetCurrentProcess().SessionId
$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$handoverLock=Enter-MoonInstallLock -Name ('Local\moon-handover-'+$owner+'-'+$session) -Security (New-MoonHandoverMutexSecurity)
try {
$powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$names=@('glazewm.exe','zebar.exe','tacky-borders.exe')
if (!$InventoryPath) {
    $InventoryPath=Join-Path $PSScriptRoot 'window_inventory.exe'
    if (!(Test-Path -LiteralPath $InventoryPath -PathType Leaf)) {$InventoryPath=Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-msvc\release\examples\window_inventory.exe'}
}
if (!$RecoveryPath) {
    $RecoveryPath=Join-Path $PSScriptRoot 'recover_windows.exe'
    if (!(Test-Path -LiteralPath $RecoveryPath -PathType Leaf)) {$RecoveryPath=Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-msvc\release\examples\recover_windows.exe'}
}
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
function Restore-PreviousDesktop($Saved,[switch]$KeepPreviousDesktopStopped) {
    Assert-MoonStopped
    foreach ($link in $Saved.startup_links) {
        if (!(Test-Path -LiteralPath $link.backup) -or (Get-FileHash -LiteralPath $link.backup -Algorithm SHA256).Hash -ne $link.sha256) {throw 'The saved startup shortcut failed its integrity check.'}
        if (Test-Path -LiteralPath $link.original) {
            if ((Get-FileHash -LiteralPath $link.original -Algorithm SHA256).Hash -ne $link.sha256) {throw "A new shortcut occupies $($link.original); it was not replaced."}
        } else {Copy-Item -LiteralPath $link.backup -Destination $link.original}
    }
    if($KeepPreviousDesktopStopped){return}
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
                if ($program.PSObject.Properties['working_directory'] -and $program.working_directory) {$start.WorkingDirectory=$program.working_directory}
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
        $installedState=Read-MoonInstallState $installation
        if (!(Test-MoonHandoverInstallation $saved $installedState)) {throw 'The current installation is not owned by this handover. Remove it explicitly before restoring the saved desktop.'}
        Invoke-MoonNative $powershell @('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot 'uninstall.ps1'),'-ExpectedInstallId',$installedState.install_id,'-ExpectedHandoverId',$saved.handover_id) -TimeoutSeconds 0 | Out-Null
    }
    Restore-PreviousDesktop $saved -KeepPreviousDesktopStopped:$KeepPreviousDesktopStopped
    $saved.status=if($KeepPreviousDesktopStopped){'rolled_back_stopped'}else{'rolled_back'}; $saved.finished_at=[DateTime]::UtcNow.ToString('o')
    Write-MoonJsonFile $StatePath $saved
    $saved | ConvertTo-Json -Depth 10
    return
}

if (!$Binary) {throw 'Provide the UIAccess release executable with -Binary.'}
foreach ($required in @('install.ps1','uninstall.ps1','setup.ps1','verify-uiaccess.ps1')) {
    if (!(Test-Path -LiteralPath (Join-Path $PSScriptRoot $required) -PathType Leaf)) {throw "Handover bundle is missing $required; no desktop state was changed."}
}
$Binary=(Resolve-Path -LiteralPath $Binary).ProviderPath
if (!$SettingsBinary) {$SettingsBinary=Join-Path ([IO.Path]::GetDirectoryName($Binary)) 'moon-settings.exe'}
$SettingsBinary=(Resolve-Path -LiteralPath $SettingsBinary).ProviderPath
$settingsHash=(Get-FileHash -LiteralPath $SettingsBinary -Algorithm SHA256).Hash
$manifest=[xml][MoonInstallNative]::Manifest($Binary)
$execution=$manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
if (!$execution -or $execution.uiAccess -ne 'true' -or $execution.level -ne 'asInvoker') {throw 'Handover requires the UIAccess release build.'}
if (Get-MoonDaemon) {throw 'Exit the development daemon before handing over the whole desktop.'}
Assert-MoonStopped
if (Test-Path -LiteralPath (Join-Path (Get-MoonInstallDirectory) 'installation.json')) {throw 'moon is already installed. Use install.ps1 for an upgrade or -Rollback to restore the previous desktop.'}
if (Test-Path -LiteralPath $StatePath) {
    $existing=Get-Content -Raw -LiteralPath $StatePath | ConvertFrom-Json
    if ($existing.status -notin @('rolled_back','rolled_back_stopped')) {throw 'An earlier handover record exists. Use -Rollback to restore it, or use install.ps1 for an upgrade.'}
}
$startupLinks=@(Get-MoonStartupLinks -Programs $names)
$processes=@(Get-PreviousDesktopProcesses | ForEach-Object {
    Get-MoonProcessLaunch -Process $_ -StartupLinks $startupLinks
})
$glaze=@($processes | Where-Object {$_.name -eq 'glazewm.exe'})
if ($glaze.Count -gt 1) {throw 'Multiple GlazeWM managers are running; no desktop state was changed.'}
$directory=Join-Path $env:LOCALAPPDATA ('moon\handover-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($directory)
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
foreach ($shortcut in $startupLinks) {
    $backup=Join-Path $directory ([IO.Path]::GetFileName($shortcut.original))
    Copy-Item -LiteralPath $shortcut.original -Destination $backup
    if ((Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash -ne $shortcut.sha256) {throw 'A startup shortcut changed before backup; no desktop state was changed.'}
    $links+=[pscustomobject]@{original=$shortcut.original;backup=$backup;program=$shortcut.program;sha256=$shortcut.sha256}
}
$configs=@()
foreach ($relative in @('.glzr\glazewm\config.yaml','.glzr\zebar\settings.json','.config\tacky-borders\config.yaml')) {
    $original=Join-Path $env:USERPROFILE $relative
    if (Test-Path -LiteralPath $original) {
        $backup=Join-Path $directory ($relative.Replace('\','_'))
        Copy-Item -LiteralPath $original -Destination $backup
        $configs+=[pscustomobject]@{original=$original;backup=$backup;sha256=(Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash}
    }
}
$saved=[pscustomobject]@{version=1;operation_id=$OperationId;keep_previous_desktop_stopped=[bool]$KeepPreviousDesktopStopped;owner_sid=$owner;session=$session;handover_id=[guid]::NewGuid().ToString('N');installation_id=$null;started_at=[DateTime]::UtcNow.ToString('o');finished_at=$null;status='prepared';source_sha256=(Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash;processes=$processes;startup_links=$links;config_backups=$configs;glaze_inventory=$glazeInventoryPath;glaze_owned_handles=$glazeHandles;glaze_recovery=$null;installed_stats=$null;uiaccess_report=$null}
[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($StatePath)))
Write-MoonJsonFile $StatePath $saved
try {
    Test-MoonSetupCancellation $OperationId
    Write-MoonSetupStatus $OperationId 'working' 'Saving your current desktop before the switch.'
    foreach ($process in $processes) {Assert-MoonProcessIdentity $process}
    foreach ($link in $links) {if ((Get-FileHash -LiteralPath $link.original -Algorithm SHA256).Hash -ne $link.sha256) {throw 'A startup shortcut changed before handover; it was not removed.'}}
    foreach ($link in $links) {Remove-Item -LiteralPath $link.original}
    $saved.status='previous_startup_disabled'; Write-MoonJsonFile $StatePath $saved
    if ($glaze.Count -gt 0) {
        $cli=Join-Path ([IO.Path]::GetDirectoryName($glaze[0].path)) 'cli\glazewm.exe'
        if (!(Test-Path -LiteralPath $cli)) {throw 'The GlazeWM command client is missing; the existing manager was not terminated.'}
        Assert-MoonProcessIdentity $glaze[0]
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
        # PID, creation time and image before stopping a companion Glaze left running.
        $expected=@($processes | Where-Object {$_.process_id -eq $process.ProcessId})
        if ($expected.Count -ne 1) {throw 'A previous desktop companion changed during handover; it was not stopped.'}
        Assert-MoonProcessIdentity $expected[0]
        Stop-Process -Id $process.ProcessId
    }
    Wait-MoonCondition -Description 'the previous desktop components to exit' -Condition {@(Get-PreviousDesktopProcesses).Count -eq 0}
    $saved.status='previous_stopped'; Write-MoonJsonFile $StatePath $saved
    # Installation owns its elevation prompt and must finish or roll back itself;
    # killing its waiting parent would abandon the elevated transaction.
    $installArguments=@('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot 'install.ps1'),'-Binary',$Binary,'-SettingsBinary',$SettingsBinary,'-ExpectedSha256',$saved.source_sha256,'-ExpectedSettingsSha256',$settingsHash,'-HandoverId',$saved.handover_id)
    if ($OperationId) {$installArguments+=@('-OperationId',$OperationId)}
    Invoke-MoonNative $powershell $installArguments -TimeoutSeconds 0 | Out-Null
    $installedState=Read-MoonInstallState (Get-MoonInstallDirectory)
    if (!(Test-MoonHandoverInstallation $saved $installedState)) {throw 'The installed result belongs to a different setup operation; it was not claimed.'}
    $saved.installation_id=$installedState.install_id
    Write-MoonJsonFile $StatePath $saved
    $saved.installed_stats=Invoke-MoonRpc query stats
    $report=Join-Path $directory 'uiaccess-results.json'
    $verification=Invoke-MoonNative $powershell @('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot 'verify-uiaccess.ps1'),'-ReportPath',$report) -TimeoutSeconds 60
    $saved.uiaccess_report=$report
    $saved.status='installed'; $saved.finished_at=[DateTime]::UtcNow.ToString('o')
    Write-MoonJsonFile $StatePath $saved
    Write-MoonSetupStatus $OperationId 'succeeded' 'Moon is ready. Make it yours in Settings.'
    $saved | ConvertTo-Json -Depth 10
} catch {
    $failure=$_
    $saved.status='recovery_required'; Write-MoonJsonFile $StatePath $saved
    if (Test-Path -LiteralPath (Join-Path (Get-MoonInstallDirectory) 'installation.json')) {
        $installedState=Read-MoonInstallState (Get-MoonInstallDirectory)
        if (Test-MoonHandoverInstallation $saved $installedState) {
            # The remover rechecks both identities after taking its elevated
            # transaction mutex. Presence alone never authorizes cleanup.
            Invoke-MoonNative $powershell @('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot 'uninstall.ps1'),'-ExpectedInstallId',$installedState.install_id,'-ExpectedHandoverId',$saved.handover_id) -TimeoutSeconds 0 | Out-Null
        }
    }
    Restore-PreviousDesktop $saved -KeepPreviousDesktopStopped:$KeepPreviousDesktopStopped
    $saved.status=if($KeepPreviousDesktopStopped){'rolled_back_stopped'}else{'rolled_back'}; $saved.finished_at=[DateTime]::UtcNow.ToString('o')
    Write-MoonJsonFile $StatePath $saved
    throw $failure
}
} finally {Exit-MoonInstallLock $handoverLock}
