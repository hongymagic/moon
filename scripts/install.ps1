[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Binary,
    [string]$SettingsBinary,
    [string]$OwnerSid,
    [string]$ExpectedSha256,
    [string]$ExpectedSettingsSha256,
    [string]$OperationId,
    [ValidatePattern('^[a-f0-9]{32}$')][string]$HandoverId
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
$recoveryComplete=$true
trap {
    $cancelled=$_.Exception -is [OperationCanceledException]
    $message=if ($cancelled) {'Setup was canceled. Your previous setup is unchanged.'} elseif ($recoveryComplete) {'Moon could not finish setup. Your previous setup was preserved.'} else {'Moon needs to finish recovering your desktop. Open details before trying again.'}
    Write-MoonSetupStatus $OperationId $(if($cancelled){'cancelled'}elseif($recoveryComplete){'failed'}else{'recovery_required'}) $message ($_ | Out-String)
    Write-Error $_ -ErrorAction Continue
    exit $(if($cancelled){1223}else{1})
}
$Binary=(Resolve-Path -LiteralPath $Binary).ProviderPath
if (!$SettingsBinary) {$SettingsBinary=Join-Path ([IO.Path]::GetDirectoryName($Binary)) 'moon-settings.exe'}
$SettingsBinary=(Resolve-Path -LiteralPath $SettingsBinary).ProviderPath
if (!$ExpectedSha256) {$ExpectedSha256=(Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash}
if (!$ExpectedSettingsSha256) {$ExpectedSettingsSha256=(Get-FileHash -LiteralPath $SettingsBinary -Algorithm SHA256).Hash}
if ($ExpectedSha256 -notmatch '^[A-Fa-f0-9]{64}$' -or $ExpectedSettingsSha256 -notmatch '^[A-Fa-f0-9]{64}$') {throw 'Invalid expected release SHA-256.'}
$currentSid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
if (!$OwnerSid) { $OwnerSid=$currentSid }
if ($OwnerSid -ne $currentSid) { throw 'Install must be elevated by the same Windows account that will run moon.' }
Test-MoonSetupCancellation $OperationId
if (!(Test-MoonAdministrator)) {
    $arguments=@('-NoProfile','-ExecutionPolicy','Bypass','-File',$PSCommandPath,'-Binary',$Binary,'-SettingsBinary',$SettingsBinary,'-OwnerSid',$OwnerSid,'-ExpectedSha256',$ExpectedSha256,'-ExpectedSettingsSha256',$ExpectedSettingsSha256)
    if ($OperationId) {$arguments+=@('-OperationId',$OperationId)}
    if ($HandoverId) {$arguments+=@('-HandoverId',$HandoverId)}
    Write-MoonSetupStatus $OperationId 'waiting' 'Approve the Windows prompt to continue.'
    try {$process=Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -Verb RunAs -WindowStyle Hidden -ArgumentList (($arguments | ForEach-Object {ConvertTo-MoonArgument $_}) -join ' ') -PassThru -Wait}
    catch {if (Test-MoonApprovalCancelled $_) {throw [OperationCanceledException]::new('The Windows approval prompt was canceled.')} else {throw}}
    exit $process.ExitCode
}
Write-MoonSetupStatus $OperationId 'working' 'Preparing a safe place for Moon.'

$installationLock=Enter-MoonInstallLock
try {
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
if ($HandoverId -and $previousState) {throw 'Another Moon installation appeared during handover. It was left unchanged.'}
$installId=if ($previousState) {$previousState.install_id} else {[guid]::NewGuid().ToString('N')}
$taskName='moon-'+$OwnerSid
$state=[pscustomobject]@{version=1;install_id=$installId;handover_id=$HandoverId;owner_sid=$OwnerSid;task_name=$taskName;certificate_thumbprints=@();active_certificate='';files=@('moon.exe');installed_at='';user_path=$null;shell=$null}
if ($previousState) { $state.certificate_thumbprints=@($previousState.certificate_thumbprints); $state.files=@($previousState.files) }
if ($previousState -and $previousState.PSObject.Properties['user_path']) {$state.user_path=$previousState.user_path}
if ($previousState -and $previousState.PSObject.Properties['shell']) {$state.shell=$previousState.shell}
$support=@('moon-settings.exe','install-common.ps1','uninstall.ps1','setup.ps1')
$supportBefore=@{}
$supportSecurity=@{}
$supportTemporary=@{}
foreach ($file in $support) {
    $source=if($file -eq 'moon-settings.exe'){$SettingsBinary}else{Join-Path $PSScriptRoot $file}
    if (!(Test-Path -LiteralPath $source -PathType Leaf)) {throw "The setup package is missing $file. Download or build the complete package."}
    $path=Join-Path $install $file;Assert-MoonNoReparse $path
    if (Test-Path -LiteralPath $path) {
        if (!$previousState -or $previousState.files -notcontains $file) {throw "An unowned file occupies $path; it was left unchanged."}
        Assert-MoonSecurePath $path
        $supportBefore[$file]=[IO.File]::ReadAllBytes($path)
        $supportSecurity[$file]=Get-Acl -LiteralPath $path
    } else {$supportBefore[$file]=$null}
    $supportTemporary[$file]=[IO.Path]::GetFileNameWithoutExtension($file)+'.next.'+[guid]::NewGuid().ToString('N')+[IO.Path]::GetExtension($file)
}
$powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$uninstallCommand=((@($powershell,'-NoProfile','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File',(Join-Path $install 'uninstall.ps1'),'-Interactive') | ForEach-Object {ConvertTo-MoonArgument $_}) -join ' ')
$version=[Diagnostics.FileVersionInfo]::GetVersionInfo($Binary).ProductVersion
if (!$version) {$version='0.1.0'}
$shellPlan=New-MoonShellPlan -Directory $install -State $state -Version $version -ShortcutArguments '' -UninstallCommand $uninstallCommand
$oldTask=Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
Assert-MoonOwnedTask $oldTask $state $target
$oldTaskXml=if ($oldTask) {Export-ScheduledTask -TaskName $taskName} else {$null}
if (@(Get-Process -Name moon-settings -ErrorAction SilentlyContinue | Where-Object {$_.Path -and [string]::Equals($_.Path,(Join-Path $install 'moon-settings.exe'),[StringComparison]::OrdinalIgnoreCase)}).Count) {throw 'Close Moon Settings, then try updating Moon again.'}
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
$newCertificate=$null; $targetReplaced=$false; $backupCreated=$false; $taskChanged=$false; $pathAdded=$false; $committed=$false; $shellStarted=$false
$supportPublished=[Collections.Generic.List[string]]::new()
try {
    Test-MoonSetupCancellation $OperationId
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
    $state.files=@(@($state.files)+$candidateName+$support+@($supportTemporary.Values) | Select-Object -Unique)
    if (Test-Path -LiteralPath $target) {$state.files+= $backupName}
    Write-MoonInstallState $install $state
    Write-MoonSetupStatus $OperationId 'working' 'Installing Moon and its settings app.'
    Test-MoonSetupCancellation $OperationId
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
    foreach ($file in $support) {
        $source=if($file -eq 'moon-settings.exe'){$SettingsBinary}else{Join-Path $PSScriptRoot $file}
        $destination=Join-Path $install $supportTemporary[$file]
        Copy-Item -LiteralPath $source -Destination $destination -Force
        $acl=Get-Acl -LiteralPath $destination;$acl.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'));Set-Acl -LiteralPath $destination -AclObject $acl
        Assert-MoonSecurePath $destination
        if ($file -eq 'moon-settings.exe') {
            if ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ne $ExpectedSettingsSha256) {throw 'The settings application changed during setup.'}
            $settingsManifest=[xml][MoonInstallNative]::Manifest($destination)
            $settingsLevel=$settingsManifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
            if (!$settingsLevel -or $settingsLevel.level -ne 'asInvoker' -or $settingsLevel.uiAccess -ne 'false') {throw 'The settings application must run with ordinary permissions.'}
        }
        $supportSignature=Set-AuthenticodeSignature -FilePath $destination -Certificate $certificate -HashAlgorithm SHA256
        if ($supportSignature.Status -ne 'Valid') {throw "Could not verify the installed $file signature."}
        $published=Join-Path $install $file
        # ReplaceFile may remove the original before reporting failure. Register
        # rollback responsibility before touching the destination in every case.
        $supportPublished.Add($file)
        if (Test-Path -LiteralPath $published) {[IO.File]::Replace($destination,$published,[NullString]::Value)}else{[IO.File]::Move($destination,$published)}
        $state.files=@($state.files | Where-Object {$_ -ne $supportTemporary[$file]})
    }
    Test-MoonSetupCancellation $OperationId
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
    Write-MoonSetupStatus $OperationId 'working' 'Restoring your window layout.'
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
    Test-MoonSetupCancellation $OperationId
    Write-MoonSetupStatus $OperationId 'working' 'Adding Moon to Start and Settings.'
    $installedSize=0L;foreach($file in $state.files){$path=Join-Path $install $file;if(Test-Path -LiteralPath $path -PathType Leaf){$installedSize+=(Get-Item -LiteralPath $path).Length}}
    ($shellPlan.registry.values|Where-Object name -eq 'EstimatedSize').value=[int][Math]::Min([int]::MaxValue,[Math]::Ceiling($installedSize/1024.0))
    $state.shell=$shellPlan.ownership
    Write-MoonInstallState $install $state
    $shellStarted=$true
    Publish-MoonShellPlan $shellPlan $state
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
    Write-MoonSetupStatus $OperationId 'succeeded' 'Moon is ready. Make it yours in Settings.'
    [pscustomobject]@{installed=$target;install_id=$installId;handover_id=$HandoverId;task=$taskName;signature='Valid';ui_access=$true;upgrade_layout_verified=($null -ne $oldLayout);certificate=$state.active_certificate;user_path_entry=$install;open_new_terminal=$true} | ConvertTo-Json
} catch {
    $failure=$_
    $recoveryComplete=$false
    Write-MoonSetupStatus $OperationId 'recovering' 'Putting your previous setup back.'
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
    if ($shellStarted) {Undo-MoonShellPlan $shellPlan $state}
    foreach ($file in $supportPublished) {
        $path=Join-Path $install $file
        if ($null -ne $supportBefore[$file]) {
            [IO.File]::WriteAllBytes($path,$supportBefore[$file])
            Set-Acl -LiteralPath $path -AclObject $supportSecurity[$file]
        }
        elseif (Test-Path -LiteralPath $path) {Remove-Item -LiteralPath $path -Force}
    }
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
    $recoveryComplete=$true
    throw $failure
} finally {
    if (Test-Path -LiteralPath $candidate) {Remove-Item -LiteralPath $candidate -Force}
    foreach($name in $supportTemporary.Values){$path=Join-Path $install $name;if(Test-Path -LiteralPath $path){Remove-Item -LiteralPath $path -Force}}
    if (!$committed -and $createdDirectory -and @(Get-ChildItem -Force -LiteralPath $install).Count -eq 0) {Remove-Item -LiteralPath $install}
}
} finally {Exit-MoonInstallLock $installationLock}
