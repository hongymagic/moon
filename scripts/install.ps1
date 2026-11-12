[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Binary,
    [string]$OwnerSid,
    [string]$ExpectedSha256
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
$Binary=(Resolve-Path -LiteralPath $Binary).ProviderPath
if (!$ExpectedSha256) {$ExpectedSha256=(Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash}
if ($ExpectedSha256 -notmatch '^[A-Fa-f0-9]{64}$') {throw 'Invalid expected release SHA-256.'}
$currentSid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
if (!$OwnerSid) { $OwnerSid=$currentSid }
if ($OwnerSid -ne $currentSid) { throw 'Install must be elevated by the same Windows account that will run moon.' }
if (!(Test-MoonAdministrator)) {
    $arguments=@('-NoProfile','-ExecutionPolicy','Bypass','-File',$PSCommandPath,'-Binary',$Binary,'-OwnerSid',$OwnerSid,'-ExpectedSha256',$ExpectedSha256)
    $process=Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -Verb RunAs -ArgumentList (($arguments | ForEach-Object {ConvertTo-MoonArgument $_}) -join ' ') -PassThru -Wait
    exit $process.ExitCode
}

$install=Get-MoonInstallDirectory
$target=Join-Path $install 'moon.exe'
$statePath=Join-Path $install 'installation.json'
Assert-MoonNoReparse $install
$createdDirectory=!(Test-Path -LiteralPath $install)
if ($createdDirectory) { [void][IO.Directory]::CreateDirectory($install); Protect-MoonDirectory $install }
else { Assert-MoonSecurePath $install }
$previousState=$null
if (Test-Path -LiteralPath $statePath) { $previousState=Read-MoonInstallState $install }
elseif (@(Get-ChildItem -Force -LiteralPath $install).Count -ne 0) { throw 'Program Files\moon contains files without owned installation metadata; nothing was replaced.' }
$installId=if ($previousState) {$previousState.install_id} else {[guid]::NewGuid().ToString('N')}
$taskName='moon-'+$OwnerSid
$state=[pscustomobject]@{version=1;install_id=$installId;owner_sid=$OwnerSid;task_name=$taskName;certificate_thumbprints=@();active_certificate='';files=@('moon.exe');installed_at='';user_path=$null}
if ($previousState) { $state.certificate_thumbprints=@($previousState.certificate_thumbprints); $state.files=@($previousState.files) }
if ($previousState -and $previousState.PSObject.Properties['user_path']) {$state.user_path=$previousState.user_path}
$oldTask=Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
Assert-MoonOwnedTask $oldTask $state $target
$oldTaskXml=if ($oldTask) {Export-ScheduledTask -TaskName $taskName} else {$null}
$daemon=Get-MoonDaemon
if ($daemon -and ![string]::Equals([IO.Path]::GetFullPath($daemon.executable),$target,[StringComparison]::OrdinalIgnoreCase)) {
    throw "A development daemon is running from $($daemon.executable). Exit that daemon before the first signed installation; it cannot restart into a different binary."
}
$session=[Diagnostics.Process]::GetCurrentProcess().SessionId
if (!$daemon) {
    $unknown=@(Get-CimInstance Win32_Process -Filter "Name='moon.exe'" | Where-Object {$_.SessionId -eq $session})
    if ($unknown.Count -gt 0) { throw 'A moon process is running but did not answer IPC. Recover or stop it before installing.' }
}
$journalPath=Join-Path $env:LOCALAPPDATA ('moon\journal-v1-session'+$session+'.json')
$oldSnapshot=if ($daemon) {Get-MoonUpgradeSnapshot -JournalPath $journalPath -ProcessId ([uint32]$daemon.process_id)} else {$null}
$oldLayout=if ($oldSnapshot) {$oldSnapshot.layout} else {$null}
$oldJournal=if ($oldSnapshot) {,$oldSnapshot.journal} else {$null}
$oldPid=if ($daemon) {[int]$daemon.process_id} else {0}
$oldProcesses=@(Get-CimInstance Win32_Process | Where-Object {$_.ExecutablePath -and $_.ExecutablePath.StartsWith($install+'\',[StringComparison]::OrdinalIgnoreCase)} | ForEach-Object {[int]$_.ProcessId})
$transaction=[guid]::NewGuid().ToString('N')
$candidateName='moon.next.'+$transaction+'.exe'; $candidate=Join-Path $install $candidateName
$backupName='moon.previous.'+$transaction+'.exe'; $backup=Join-Path $install $backupName
$newCertificate=$null; $targetReplaced=$false; $backupCreated=$false; $taskChanged=$false; $pathAdded=$false; $committed=$false
try {
    $certificate=$null
    if ($previousState) {
        if ($previousState.active_certificate -notmatch '^[A-Fa-f0-9]{40}$') { throw 'Invalid signing certificate thumbprint in installation metadata.' }
        $certificate=Get-Item -LiteralPath ('Cert:\LocalMachine\My\'+$previousState.active_certificate) -ErrorAction SilentlyContinue
        Assert-MoonCertificate $certificate $installId
    }
    if (!$certificate -or $certificate.NotAfter -lt (Get-Date).AddDays(30)) {
        $certificate=New-SelfSignedCertificate -Subject "CN=moon code signing $installId" -Type CodeSigningCert -CertStoreLocation Cert:\LocalMachine\My `
            -Provider 'Microsoft Software Key Storage Provider' -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -KeyUsage DigitalSignature -NotAfter (Get-Date).AddYears(3)
        $newCertificate=$certificate
    }
    Assert-MoonCertificate $certificate $installId
    $state.active_certificate=$certificate.Thumbprint
    $state.certificate_thumbprints=@(@($state.certificate_thumbprints)+$certificate.Thumbprint | Select-Object -Unique)
    # Persist ownership before adding any machine-wide trust. A crash cannot leave
    # an imported root whose owner is known only in this installer's memory.
    $state.files+= $candidateName
    if (Test-Path -LiteralPath $target) {$state.files+= $backupName}
    Write-MoonInstallState $install $state
    $certificatePath=Join-Path $install ('certificate.'+$transaction+'.cer')
    try {
        Export-Certificate -Cert $certificate -FilePath $certificatePath | Out-Null
        Import-Certificate -FilePath $certificatePath -CertStoreLocation Cert:\LocalMachine\Root | Out-Null
        Import-Certificate -FilePath $certificatePath -CertStoreLocation Cert:\LocalMachine\TrustedPublisher | Out-Null
    } finally { if (Test-Path -LiteralPath $certificatePath) {Remove-Item -LiteralPath $certificatePath -Force} }
    Copy-Item -LiteralPath $Binary -Destination $candidate
    if ((Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash -ne $ExpectedSha256) {throw 'The release executable changed during installation; the unreviewed candidate was not signed.'}
    # Copy-Item inherits the protected directory ACL. Fix its owner as well;
    # UIAccess must never depend on a file that an ordinary user can rewrite.
    $candidateAcl=Get-Acl -LiteralPath $candidate
    $candidateAcl.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
    Set-Acl -LiteralPath $candidate -AclObject $candidateAcl
    Assert-MoonSecurePath $candidate
    $manifest=[xml][MoonInstallNative]::Manifest($candidate)
    $execution=$manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
    if (!$execution -or $execution.uiAccess -ne 'true' -or $execution.level -ne 'asInvoker') { throw 'Install requires the uiaccess release build with asInvoker execution level.' }
    $signature=Set-AuthenticodeSignature -FilePath $candidate -Certificate $certificate -HashAlgorithm SHA256
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $certificate.Thumbprint) { throw "Candidate signature validation failed: $($signature.StatusMessage)" }
    Assert-MoonNoReparse $target
    if (Test-Path -LiteralPath $target) {
        if (!$previousState) { throw 'An unowned target executable already exists.' }
        Assert-MoonSecurePath $target
        Move-Item -LiteralPath $target -Destination $backup
        $backupCreated=$true
    }
    Move-Item -LiteralPath $candidate -Destination $target
    $targetReplaced=$true
    $state.files=@($state.files | Where-Object {$_ -ne $candidateName} | Select-Object -Unique)
    $action=New-MoonTaskAction $target
    $trigger=New-ScheduledTaskTrigger -AtLogOn -User $OwnerSid
    $principal=New-ScheduledTaskPrincipal -UserId $OwnerSid -LogonType Interactive -RunLevel Limited
    $settings=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName $taskName -Description "moon installation $installId; owner $OwnerSid" -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
    $taskChanged=$true
    $state.installed_at=[DateTime]::UtcNow.ToString('o')
    Write-MoonInstallState $install $state
    if ($daemon) { Invoke-MoonRpc command -Commands @('restart') | Out-Null }
    else { Start-ScheduledTask -TaskName $taskName }
    Wait-MoonCondition -Description 'the signed replacement daemon with UIAccess' -Condition {
        $running=Get-MoonDaemon
        $running -and $running.process_id -ne $oldPid -and [string]::Equals($running.executable,$target,[StringComparison]::OrdinalIgnoreCase) -and $running.token.ui_access
    }
    if ($null -ne $oldLayout) {
        Wait-MoonCondition -Description 'the preserved window layout after upgrade' -Condition {(Get-MoonLayoutFingerprint) -eq $oldLayout}
    }
    $environment=[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    try {
        $pathChange=Get-MoonPathAddition (Read-MoonPathValue $environment) $install $state.user_path
        $state.user_path=$pathChange.ownership
        # Record ownership before editing the user environment. Removal always
        # reads the current value and removes only our entry, never a saved PATH.
        Write-MoonInstallState $install $state
        Set-MoonPathChange $environment $pathChange
        $pathAdded=$pathChange.changed
        if ($pathAdded) {[MoonInstallNative]::EnvironmentChanged()}
    } finally {$environment.Dispose()}
    # Older owned backups are expendable only after replacement and layout verification.
    if ($previousState) {
        foreach ($file in $previousState.files) {
            if ($file -match '^moon\.previous\.[a-f0-9]{32}\.exe$' -and $file -ne $backupName) {
                $old=Join-Path $install $file; Assert-MoonNoReparse $old
                try { if (Test-Path -LiteralPath $old) {Remove-Item -LiteralPath $old -Force}; $state.files=@($state.files | Where-Object {$_ -ne $file}) }
                catch { Write-Verbose "Retaining a previous executable still in use: $file" }
            }
        }
        Write-MoonInstallState $install $state
    }
    $committed=$true
    [pscustomobject]@{installed=$target;task=$taskName;signature='Valid';ui_access=$true;upgrade_layout_verified=($null -ne $oldLayout);certificate=$state.active_certificate;user_path_entry=$install;open_new_terminal=$true} | ConvertTo-Json
} catch {
    $failure=$_
    if ($targetReplaced -or $backupCreated) {
        $running=Get-MoonDaemon
        if ($running -and $running.process_id -ne $oldPid -and [string]::Equals($running.executable,$target,[StringComparison]::OrdinalIgnoreCase)) {
            try {Invoke-MoonRpc command -Commands @('exit') | Out-Null; Wait-MoonCondition -Description 'failed replacement shutdown' -Condition {!(Get-MoonDaemon)}} catch {throw "Install failed and replacement recovery could not finish; binaries and trust retained for recovery. Original failure: $failure; recovery: $_"}
        }
        # IPC closes before a replacement's recovery watchdog has necessarily
        # exited. Do not remove an executable while that watchdog still needs it.
        Wait-MoonCondition -Description 'failed replacement processes and recovery watchdogs' -Condition {
            @((Get-CimInstance Win32_Process) | Where-Object {$_.ExecutablePath -and $_.ExecutablePath.StartsWith($install+'\',[StringComparison]::OrdinalIgnoreCase) -and $oldProcesses -notcontains [int]$_.ProcessId}).Count -eq 0
        }
        if (Test-Path -LiteralPath $target) {Remove-Item -LiteralPath $target -Force}
        if (Test-Path -LiteralPath $backup) {Move-Item -LiteralPath $backup -Destination $target}
    }
    if ($taskChanged) {
        if ($oldTaskXml) { Register-ScheduledTask -TaskName $taskName -Xml $oldTaskXml -Force | Out-Null }
        else {Unregister-ScheduledTask -TaskName $taskName -Confirm:$false}
    }
    if ($pathAdded) {Remove-MoonOwnedUserPath $state.user_path}
    if ($previousState) {Write-MoonInstallState $install $previousState}
    elseif (Test-Path -LiteralPath $statePath) {Remove-Item -LiteralPath $statePath -Force}
    if ($newCertificate) {
        foreach ($store in @('LocalMachine\Root','LocalMachine\TrustedPublisher')) {
            $certificateItem="Cert:\$store\$($newCertificate.Thumbprint)"
            if (Test-Path -LiteralPath $certificateItem) {Remove-Item -LiteralPath $certificateItem -Force}
        }
        Remove-Item -LiteralPath "Cert:\LocalMachine\My\$($newCertificate.Thumbprint)" -DeleteKey -Force
    }
    if ($oldJournal -and !(Get-MoonDaemon)) {
        Wait-MoonCondition -Description 'old recovery watchdogs before rollback adoption' -Condition {
            @((Get-CimInstance Win32_Process) | Where-Object {$_.SessionId -eq $session -and $_.ExecutablePath -and $_.ExecutablePath.StartsWith($install+'\',[StringComparison]::OrdinalIgnoreCase)}).Count -eq 0
        }
        $journalTemporary=$journalPath+'.install-rollback.tmp'
        [IO.File]::WriteAllBytes($journalTemporary,$oldJournal)
        if (Test-Path -LiteralPath $journalPath) {[IO.File]::Replace($journalTemporary,$journalPath,[NullString]::Value)} else {[IO.File]::Move($journalTemporary,$journalPath)}
        Start-MoonLimitedOnce $target 'start --background --resume' $state
        Wait-MoonCondition -Description 'the previous layout after rollback' -Condition {(Get-MoonLayoutFingerprint) -eq $oldLayout}
    }
    throw $failure
} finally {
    if (Test-Path -LiteralPath $candidate) {Remove-Item -LiteralPath $candidate -Force}
    if (!$committed -and $createdDirectory -and @(Get-ChildItem -Force -LiteralPath $install).Count -eq 0) {Remove-Item -LiteralPath $install}
}
