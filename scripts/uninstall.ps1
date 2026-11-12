[CmdletBinding()]
param([string]$OwnerSid,[switch]$Interactive,[string]$OperationId,
    [ValidatePattern('^[a-f0-9]{32}$')][string]$ExpectedInstallId,
    [ValidatePattern('^[a-f0-9]{32}$')][string]$ExpectedHandoverId)
. (Join-Path $PSScriptRoot 'install-common.ps1')
if ($Interactive) {& (Join-Path $PSScriptRoot 'setup.ps1') -Mode Uninstall;return}
$removalStarted=$false
trap {
    $cancelled=$_.Exception -is [OperationCanceledException]
    $status=if($cancelled){'cancelled'}elseif($removalStarted){'recovery_required'}else{'failed'}
    $message=if($cancelled){'Moon is still installed.'}elseif($removalStarted){'Removal could not finish. Your recovery files were kept.'}else{'Moon could not be removed. Nothing has been deleted.'}
    Write-MoonSetupStatus $OperationId $status $message ($_ | Out-String)
    Write-Error $_ -ErrorAction Continue
    exit $(if($cancelled){1223}else{1})
}
$currentSid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
if (!$OwnerSid) {$OwnerSid=$currentSid}
if ($OwnerSid -ne $currentSid) {throw 'Uninstall must be elevated by the same Windows account that owns moon.'}
Test-MoonSetupCancellation $OperationId
if (!(Test-MoonAdministrator)) {
    $arguments=@('-NoProfile','-ExecutionPolicy','Bypass','-File',$PSCommandPath,'-OwnerSid',$OwnerSid)
    if ($OperationId) {$arguments+=@('-OperationId',$OperationId)}
    if ($ExpectedInstallId) {$arguments+=@('-ExpectedInstallId',$ExpectedInstallId)}
    if ($ExpectedHandoverId) {$arguments+=@('-ExpectedHandoverId',$ExpectedHandoverId)}
    Write-MoonSetupStatus $OperationId 'waiting' 'Approve the Windows prompt to remove Moon.'
    try {$exitCode=Invoke-MoonShellProcess (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') $arguments -Elevate}
    catch {if (Test-MoonApprovalCancelled $_) {throw [OperationCanceledException]::new('The Windows approval prompt was canceled.')}else{throw}}
    exit $exitCode
}
$installationLock=Enter-MoonInstallLock
try {
$install=Get-MoonInstallDirectory
if (!(Test-Path -LiteralPath $install)) {Write-MoonSetupStatus $OperationId 'succeeded' 'Moon is already removed.';Write-Output 'moon is not installed.'; return}
$state=Read-MoonInstallState $install
Assert-MoonExpectedInstallation $state $ExpectedInstallId $ExpectedHandoverId
$target=Join-Path $install 'moon.exe'
$task=Get-ScheduledTask -TaskName $state.task_name -ErrorAction SilentlyContinue
Assert-MoonOwnedTask $task $state $target
$session=[Diagnostics.Process]::GetCurrentProcess().SessionId
$otherSessions=@(Get-CimInstance Win32_Process | Where-Object {$_.SessionId -ne $session -and $_.ExecutablePath -and $_.ExecutablePath.StartsWith($install+'\',[StringComparison]::OrdinalIgnoreCase)})
if ($otherSessions.Count -gt 0) {throw 'moon is running in another Windows session. Close it there before uninstalling this shared executable.'}
if (@(Get-Process -Name moon-settings -ErrorAction SilentlyContinue | Where-Object {$_.Path -and [string]::Equals($_.Path,(Join-Path $install 'moon-settings.exe'),[StringComparison]::OrdinalIgnoreCase)}).Count) {throw 'Close Moon Settings, then try removing Moon again.'}
Test-MoonSetupCancellation $OperationId
Write-MoonSetupStatus $OperationId 'working' 'Restoring your windows before Moon leaves.'
$daemon=Get-MoonDaemon
if ($daemon) {
    if (![string]::Equals($daemon.executable,$target,[StringComparison]::OrdinalIgnoreCase)) {throw 'A different development daemon is running; its windows and process were left unchanged.'}
    Invoke-MoonRpc command -Commands @('exit') | Out-Null
    Wait-MoonCondition -Description 'the installed daemon to restore its windows and exit' -Condition {!(Get-MoonDaemon)}
}
$journal=Join-Path $env:LOCALAPPDATA ('moon\journal-v1-session'+$session+'.json')
if (Test-Path -LiteralPath $journal) {
    # ShellExecute is required for a UIAccess image; exit code is read from Process,
    # never PowerShell's pipeline-dependent LASTEXITCODE.
    if (!(Test-Path -LiteralPath $target)) {throw 'Recovery journal exists but the installed executable is missing. Installation evidence was retained.'}
    $recovery=Start-Process -FilePath $target -ArgumentList 'restore' -WindowStyle Hidden -PassThru
    try {
        if (!$recovery.WaitForExit(15000)) {throw 'Window recovery did not finish; installation and trust retained.'}
        if ($recovery.ExitCode -ne 0 -or (Test-Path -LiteralPath $journal)) {throw 'Window recovery failed; installation and trust retained.'}
    } finally {$recovery.Dispose()}
}
Wait-MoonCondition -Description 'installed moon processes and watchdogs to exit' -Condition {
    @((Get-CimInstance Win32_Process) | Where-Object {$_.ExecutablePath -and $_.ExecutablePath.StartsWith($install+'\',[StringComparison]::OrdinalIgnoreCase)}).Count -eq 0
}
# Validate the entire removal list before changing startup, trust or files.
foreach ($file in $state.files) {
    if ($file -notin @('moon.exe','moon-settings.exe','install-common.ps1','uninstall.ps1','setup.ps1') -and $file -notmatch '^(moon\.(previous|next)\.[a-f0-9]{32}\.exe|moon-settings\.next\.[a-f0-9]{32}\.exe|(install-common|uninstall|setup)\.next\.[a-f0-9]{32}\.ps1)$') {throw "Unrecognised owned installation file: $file"}
    Assert-MoonNoReparse (Join-Path $install $file)
}
foreach ($thumbprint in $state.certificate_thumbprints) {
    if ($thumbprint -notmatch '^[A-Fa-f0-9]{40}$') {throw 'Invalid owned certificate thumbprint.'}
    foreach ($store in @('LocalMachine\My','LocalMachine\Root','LocalMachine\TrustedPublisher')) {
        $path="Cert:\$store\$thumbprint"
        if (Test-Path -LiteralPath $path) {
            if ((Get-Item -LiteralPath $path).Subject -ne "CN=moon code signing $($state.install_id)") {throw 'A recorded certificate does not belong to this installation; nothing was removed.'}
        }
    }
}
Test-MoonSetupCancellation $OperationId
# Removal becomes irreversible only after every recovery and ownership check.
# A cancellation after this boundary waits for safe completion.
$removalStarted=$true
Write-MoonSetupStatus $OperationId 'removing' 'Removing Moon. Your personal settings will stay.'
Remove-MoonShellIntegration $state
if ($state.PSObject.Properties['user_path']) {Remove-MoonOwnedUserPath $state.user_path}
if ($task) {Unregister-ScheduledTask -TaskName $state.task_name -Confirm:$false}
foreach ($thumbprint in $state.certificate_thumbprints) {
    foreach ($store in @('LocalMachine\Root','LocalMachine\TrustedPublisher')) {
        $path="Cert:\$store\$thumbprint"; if (Test-Path -LiteralPath $path) {Remove-Item -LiteralPath $path -Force}
    }
    $key="Cert:\LocalMachine\My\$thumbprint"; if (Test-Path -LiteralPath $key) {Remove-Item -LiteralPath $key -DeleteKey -Force}
}
foreach ($file in $state.files) { $path=Join-Path $install $file; if (Test-Path -LiteralPath $path) {Remove-Item -LiteralPath $path -Force} }
Remove-Item -LiteralPath (Join-Path $install 'installation.json') -Force
if (@(Get-ChildItem -Force -LiteralPath $install).Count -eq 0) {Remove-Item -LiteralPath $install}
Write-MoonSetupStatus $OperationId 'succeeded' 'Moon has been removed. Your settings are still on this PC.'
Write-Output 'moon, its owned PATH entry, startup task and signing trust were removed. Your configuration and validation records remain in your profile.'
} finally {Exit-MoonInstallLock $installationLock}
