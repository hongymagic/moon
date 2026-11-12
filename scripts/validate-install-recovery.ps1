[CmdletBinding()]
param([ValidateSet('Check','Probe','Hold')][string]$Mode='Check',[string]$LockName,[string]$SignalPath,[string]$ReportPath)
. (Join-Path $PSScriptRoot 'install-common.ps1')
function New-TestMutexSecurity {
    $security=[Security.AccessControl.MutexSecurity]::new()
    $security.AddAccessRule([Security.AccessControl.MutexAccessRule]::new([Security.Principal.WindowsIdentity]::GetCurrent().User,[Security.AccessControl.MutexRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow))
    $security
}
if ($Mode -eq 'Probe') {
    $lock=$null
    try {$lock=Enter-MoonInstallLock -Name $LockName -Security (New-TestMutexSecurity);'acquired'}
    catch {if ($_.Exception.Message -notlike '*Another Moon setup*') {throw};'busy'}
    finally {Exit-MoonInstallLock $lock}
    return
}
if ($Mode -eq 'Hold') {
    $lock=Enter-MoonInstallLock -Name $LockName -Security (New-TestMutexSecurity)
    try {[IO.File]::WriteAllText($SignalPath,'ready');Start-Sleep -Seconds 30} finally {Exit-MoonInstallLock $lock}
    return
}
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-install-recovery-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
$checks=[Collections.Generic.List[string]]::new()
$powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$lockName='Local\moon-install-test-'+[guid]::NewGuid().ToString('N')
$lock=$null;$child=$null;$keeper=$null
try {
    $security=New-MoonInstallMutexSecurity
    $rules=@($security.GetAccessRules($true,$false,[Security.Principal.SecurityIdentifier]))
    if ($rules.Count -ne 2 -or $security.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne 'S-1-5-32-544' -or !$security.AreAccessRulesProtected) {throw 'Production setup mutex has unexpected ownership or inheritance.'}
    foreach($rule in $rules) {if($rule.IdentityReference.Value -notin @('S-1-5-32-544','S-1-5-18') -or $rule.AccessControlType -ne 'Allow' -or $rule.MutexRights -ne 'FullControl'){throw 'Production setup mutex grants unexpected access.'}}
    $checks.Add('Production mutex descriptor restricts ownership and access to Administrators and SYSTEM')

    $lock=Enter-MoonInstallLock -Name $lockName -Security (New-TestMutexSecurity)
    $arguments=@('-NoProfile','-ExecutionPolicy','Bypass','-File',$PSCommandPath,'-Mode','Probe','-LockName',$lockName)
    $blocked=Invoke-MoonNative $powershell $arguments
    if ($blocked.Output.Trim() -ne 'busy') {throw 'A second setup process entered the active transaction.'}
    Exit-MoonInstallLock $lock;$lock=$null
    $released=Invoke-MoonNative $powershell $arguments
    if ($released.Output.Trim() -ne 'acquired') {throw 'A completed transaction retained the installation mutex.'}
    $checks.Add('A real second process is rejected while setup owns the mutex and can enter after release')
    $lock=Enter-MoonInstallLock -Name $lockName -Security (New-MoonHandoverMutexSecurity)
    $blocked=Invoke-MoonNative $powershell $arguments
    Exit-MoonInstallLock $lock;$lock=$null
    if ($blocked.Output.Trim() -ne 'busy') {throw 'An ordinary second handover entered active preflight.'}
    $checks.Add('Ordinary handover mutex excludes a real concurrent preflight without requiring elevation')

    $signal=Join-Path $temporary 'owner-ready'
    $arguments=@('-NoProfile','-ExecutionPolicy','Bypass','-File',$PSCommandPath,'-Mode','Hold','-LockName',$lockName,'-SignalPath',$signal)
    $start=[Diagnostics.ProcessStartInfo]::new();$start.FileName=$powershell;$start.UseShellExecute=$false;$start.CreateNoWindow=$true
    $start.Arguments=($arguments|ForEach-Object {ConvertTo-MoonArgument $_}) -join ' '
    $child=[Diagnostics.Process]::Start($start)
    Wait-MoonCondition -Description 'the isolated mutex owner' -Condition {Test-Path -LiteralPath $signal} -Seconds 10
    $keeper=[Threading.Mutex]::OpenExisting($lockName)
    $child.Kill();$child.WaitForExit();$child.Dispose();$child=$null
    $lock=Enter-MoonInstallLock -Name $lockName -Security (New-TestMutexSecurity)
    Exit-MoonInstallLock $lock;$lock=$null;$keeper.Dispose();$keeper=$null
    $checks.Add('An abandoned transaction mutex is acquired for recovery after its exact owned helper exits')

    $operation=[guid]::NewGuid().ToString('N')
    function Get-MoonSetupFiles([string]$OperationId) {[pscustomobject]@{directory=$temporary;status=(Join-Path $temporary 'status.json');cancel=(Join-Path $temporary 'cancel')}}
    Write-MoonSetupStatus $operation 'working' 'Initial progress'
    $statusPath=(Get-MoonSetupFiles $operation).status
    $reader=[IO.FileStream]::new($statusPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    try {
        Write-MoonSetupStatus $operation 'working' 'Updated while previous snapshot is open'
        if ((Read-MoonSetupStatus $operation).message -ne 'Updated while previous snapshot is open') {throw 'Status publication was blocked by its reader.'}
    } finally {$reader.Dispose()}
    $checks.Add('Status publication replaces an existing file while its old snapshot is still being read')

    $tokens=$null;$errors=$null
    $setupAst=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'setup.ps1'),[ref]$tokens,[ref]$errors)
    if ($errors.Count) {throw ($errors|Out-String)}
    $tick=@($setupAst.FindAll({param($node) $node -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $node.Member.Value -eq 'Add_Tick'},$true))
    if ($tick.Count -ne 1) {throw 'Expected exactly one production setup completion handler.'}
    $tickText=$tick[0].Arguments[0].ScriptBlock.Extent.Text
    $completion=[scriptblock]::Create($tickText.Substring(1,$tickText.Length-2))
    & {
        function Read-MoonSetupStatus {param($OperationId) throw [IO.IOException]::new('Injected unreadable progress')}
        $process=[pscustomobject]@{HasExited=$true;ExitCode=1};$process|Add-Member ScriptMethod Dispose {}
        $timer=[pscustomobject]@{stopped=$false};$timer|Add-Member ScriptMethod Stop {$this.stopped=$true}
        $context=@{busy=$true;process=$process;operation=$operation;status='working';outcome=$null}
        $ui=@{};foreach($name in @('Status','Progress','Primary','Secondary','Details')){$ui[$name]=[pscustomobject]@{Text='';Visibility='';IsEnabled=$false;Content='';IsCancel=$false}}
        . $completion
        if ($context.busy -or $context.process -or !$timer.stopped -or !$ui.Primary.IsEnabled -or $ui.Details.Visibility -ne 'Visible') {throw 'Unreadable progress prevented the setup UI from handling worker exit.'}
    }
    $checks.Add('Actual setup completion handler handles worker failure even when progress cannot be read')

    # Execute the production rollback body, not a duplicate implementation. Only
    # desktop/process/task operations are replaced; every recovery file is private.
    $tokens=$null;$errors=$null
    $ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'install.ps1'),[ref]$tokens,[ref]$errors)
    if ($errors.Count) {throw ($errors|Out-String)}
    $recovery=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.CatchClauseAst] -and $node.Body.Extent.Text -match '\$recoveryComplete=\$false'},$true))
    if ($recovery.Count -ne 1) {throw 'Expected exactly one production installer recovery body.'}
    $body=$recovery[0].Body.Extent.Text
    $rollback=[scriptblock]::Create($body.Substring(1,$body.Length-2))
    $publish=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.ForEachStatementAst] -and $node.Body.Extent.Text -match '\$supportPublished.Add'},$true))
    if ($publish.Count -ne 1) {throw 'Expected exactly one production support publication body.'}
    # An extracted AST has no source-file directory; supply the same package
    # directory that the actual installer gets from its automatic variable.
    $publishBody=[scriptblock]::Create($publish[0].Extent.Text.Replace('$PSScriptRoot',("'"+$PSScriptRoot.Replace("'","''")+"'")))
    $install=Join-Path $temporary 'replacement-attempt';[void][IO.Directory]::CreateDirectory($install)
    $published=Join-Path $install 'setup.ps1';[IO.File]::WriteAllText($published,'previous setup')
    $support=@('setup.ps1');$supportTemporary=@{'setup.ps1'='setup.next.fixture.ps1'}
    $supportPublished=[Collections.Generic.List[string]]::new();$state=[pscustomobject]@{files=@('setup.ps1','setup.next.fixture.ps1')};$certificate=$null
    $blocked=[IO.FileStream]::new($published,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    try {
        $rejected=& {
            # No protected ACL or trust changes in this test. Exercise the real
            # copy and Windows replacement, with only elevation/signing mocked.
            function Set-Acl {param($LiteralPath,$AclObject)}
            function Assert-MoonSecurePath {param($Path)}
            function Set-AuthenticodeSignature {param($FilePath,$Certificate,$HashAlgorithm) [pscustomobject]@{Status='Valid'}}
            try {. $publishBody;[pscustomobject]@{failed=$false;message=''}} catch {[pscustomobject]@{failed=$true;message=$_.Exception.Message}}
        }
        if (!$rejected.failed -or !$supportPublished.Contains('setup.ps1')) {throw ('A failed support replacement escaped rollback ownership: '+$rejected.message)}
        if ([IO.File]::ReadAllText($published) -ne 'previous setup') {throw 'A locked support file lost its original content.'}
    } finally {$blocked.Dispose()}
    $checks.Add('Actual support publication records rollback ownership before a Windows sharing violation')
    $writeJson=${function:Write-MoonJsonFile}
    function Write-MoonJsonFile([string]$Path,$Value) {
        if ($Path -eq $statusPath) {throw [IO.IOException]::new('Injected status publication failure')}
        & $writeJson $Path $Value
    }
    function Get-MoonDaemon {if ($script:fixtureDaemon) {$script:fixtureDaemon}}
    function Get-CimInstance {param($ClassName) @()}
    function Invoke-MoonRpc {
        param($Method,[string[]]$Commands)
        if ($Method -ne 'command' -or $Commands.Count -ne 1 -or $Commands[0] -ne 'exit') {throw 'Recovery issued an unexpected desktop command.'}
        if ([IO.File]::ReadAllText($target) -ne 'new executable') {throw 'The replacement was removed before its daemon restored windows.'}
        $script:fixtureDaemon=$null;$script:fixtureExit=$true
    }
    foreach($kind in @('failure','cancelled')) {
        $install=Join-Path $temporary $kind;[void][IO.Directory]::CreateDirectory($install)
        $target=Join-Path $install 'moon.exe';$backup=Join-Path $install 'moon.previous.fixture.exe'
        [IO.File]::WriteAllText($target,'new executable');[IO.File]::WriteAllText($backup,'previous executable')
        $supportPublished=[Collections.Generic.List[string]]::new();$supportPublished.Add('moon-settings.exe');$supportPublished.Add('setup.ps1')
        # Simulate documented ReplaceFile error 1176: original support file is
        # absent even though replacement did not return success.
        $supportBefore=@{'moon-settings.exe'=[Text.Encoding]::UTF8.GetBytes('previous settings');'setup.ps1'=$null}
        $savedSecurity=[Security.AccessControl.FileSecurity]::new()
        $savedSecurity.SetAccessRuleProtection($true,$false)
        $testOwner=[Security.Principal.WindowsIdentity]::GetCurrent().User
        $savedSecurity.SetOwner($testOwner)
        $savedSecurity.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($testOwner,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow))
        $supportSecurity=@{'moon-settings.exe'=$savedSecurity}
        [IO.File]::WriteAllText((Join-Path $install 'setup.ps1'),'new setup')
        $previousState=[pscustomobject]@{version=1;fixture='previous metadata'}
        $targetReplaced=$true;$backupCreated=$true;$oldPid=123;$oldProcesses=@();$taskChanged=$false;$pathAdded=$false;$shellStarted=$false;$newCertificate=$null;$oldJournal=$null;$recoveryComplete=$false
        $script:fixtureDaemon=[pscustomobject]@{process_id=456;executable=$target};$script:fixtureExit=$false
        $OperationId=$operation
        $original=if($kind -eq 'cancelled'){[OperationCanceledException]::new('Injected cancellation checkpoint')}else{[IO.IOException]::new('Injected support replacement failure')}
        $caught=$null
        try {try {throw $original} catch {. $rollback}} catch {$caught=$_.Exception}
        if (!$caught -or $caught.Message -ne $original.Message -or $caught.GetType() -ne $original.GetType()) {throw 'Progress publication replaced the original transaction failure.'}
        if (!$recoveryComplete -or !$script:fixtureExit -or $script:fixtureDaemon) {throw 'Recovery did not finish after stopping the replacement daemon.'}
        if ([IO.File]::ReadAllText($target) -ne 'previous executable' -or [IO.File]::ReadAllText((Join-Path $install 'moon-settings.exe')) -ne 'previous settings') {throw 'Executable or failed support replacement was not restored.'}
        $restoredSecurity=Get-Acl -LiteralPath (Join-Path $install 'moon-settings.exe')
        $expectedRules=$savedSecurity.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])|Select-Object IdentityReference,FileSystemRights,AccessControlType,IsInherited,InheritanceFlags,PropagationFlags|ConvertTo-Json -Depth 4 -Compress
        $actualRules=$restoredSecurity.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])|Select-Object IdentityReference,FileSystemRights,AccessControlType,IsInherited,InheritanceFlags,PropagationFlags|ConvertTo-Json -Depth 4 -Compress
        if ($restoredSecurity.GetOwner([Security.Principal.SecurityIdentifier]) -ne $savedSecurity.GetOwner([Security.Principal.SecurityIdentifier]) -or $restoredSecurity.AreAccessRulesProtected -ne $savedSecurity.AreAccessRulesProtected -or $actualRules -ne $expectedRules) {throw 'Recreated support file lost its saved owner or access rules.'}
        if (Test-Path -LiteralPath (Join-Path $install 'setup.ps1')) {throw 'First-install support file survived rollback.'}
        if ((Get-Content -Raw -LiteralPath (Join-Path $install 'installation.json')|ConvertFrom-Json).fixture -ne 'previous metadata') {throw 'Previous ownership metadata was not restored.'}
        $checks.Add("Actual installer recovery preserves $kind despite progress I/O failure; RPC exit precedes binary restoration, missing support is restored, and new support is removed")
    }

    $tokens=$null;$errors=$null
    $handoverAst=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'handover.ps1'),[ref]$tokens,[ref]$errors)
    if ($errors.Count) {throw ($errors|Out-String)}
    $handoverRecovery=@($handoverAst.FindAll({param($node) $node -is [Management.Automation.Language.CatchClauseAst] -and $node.Body.Extent.Text -match '\$saved.status=''recovery_required'''},$true))
    if ($handoverRecovery.Count -ne 1) {throw 'Expected exactly one production handover recovery body.'}
    $handoverText=$handoverRecovery[0].Body.Extent.Text
    $handoverRollback=[scriptblock]::Create($handoverText.Substring(1,$handoverText.Length-2).Replace('$PSScriptRoot',("'"+$PSScriptRoot.Replace("'","''")+"'")))
    & {
        $fixtureInstall=Join-Path $temporary 'handover';[void][IO.Directory]::CreateDirectory($fixtureInstall)
        function Get-MoonInstallDirectory {$fixtureInstall}
        function Read-MoonInstallState {param($Directory) Get-Content -Raw -LiteralPath (Join-Path $Directory 'installation.json')|ConvertFrom-Json}
        function Restore-PreviousDesktop {param($Saved) $script:restoredDesktop=$true}
        function Invoke-MoonRpc {throw 'Handover must not command an unverified daemon during cleanup.'}
        function Invoke-MoonNative {
            param($Path,[string[]]$Arguments,[int]$TimeoutSeconds)
            if ($Arguments -notcontains '-ExpectedInstallId' -or $Arguments -notcontains '-ExpectedHandoverId') {throw 'Handover cleanup omitted its checked ownership guards.'}
            $expectedId=$Arguments[[Array]::IndexOf($Arguments,'-ExpectedInstallId')+1]
            $expectedHandover=$Arguments[[Array]::IndexOf($Arguments,'-ExpectedHandoverId')+1]
            $current=Read-MoonInstallState $fixtureInstall
            if ($script:replaceBeforeRemoval) {$current.install_id=[guid]::NewGuid().ToString('N')}
            Assert-MoonExpectedInstallation $current $expectedId $expectedHandover
            $script:uninstalledOwned=$true;Remove-Item -LiteralPath (Join-Path $fixtureInstall 'installation.json')
        }
        foreach($kind in @('absent','unrelated','own-lost-response','changed-install-id','changed-before-removal')) {
            $token=[guid]::NewGuid().ToString('N');$id=[guid]::NewGuid().ToString('N')
            $saved=[pscustomobject]@{handover_id=$token;installation_id=$null;status='prepared';finished_at=$null}
            $StatePath=Join-Path $fixtureInstall 'handover.json'
            $metadata=Join-Path $fixtureInstall 'installation.json'
            if (Test-Path -LiteralPath $metadata) {Remove-Item -LiteralPath $metadata}
            $state=[pscustomobject]@{install_id=$id;handover_id=$token}
            if ($kind -eq 'unrelated') {$state.handover_id=[guid]::NewGuid().ToString('N')}
            if ($kind -eq 'changed-install-id') {$saved.installation_id=[guid]::NewGuid().ToString('N')}
            if ($kind -ne 'absent') {Write-MoonJsonFile $metadata $state}
            $script:restoredDesktop=$false;$script:uninstalledOwned=$false;$script:replaceBeforeRemoval=$kind -eq 'changed-before-removal'
            $caught=$null
            try {try {throw [IO.IOException]::new('Injected handover child failure')} catch {. $handoverRollback}} catch {$caught=$_.Exception}
            $shouldUninstall=$kind -eq 'own-lost-response'
            if ($script:uninstalledOwned -ne $shouldUninstall) {throw "Handover cleanup claimed the wrong installation in $kind."}
            if ($kind -eq 'changed-before-removal') {
                if ($script:restoredDesktop -or !$caught -or $caught.Message -notlike '*installation changed*') {throw 'A changed install was not rejected inside removal before desktop restoration.'}
            } elseif (!$script:restoredDesktop -or !$caught -or $caught.Message -ne 'Injected handover child failure') {throw "Handover recovery did not preserve its original failure in $kind."}
            if ($kind -in @('unrelated','changed-install-id','changed-before-removal') -and !(Test-Path -LiteralPath $metadata)) {throw 'Handover removed a different installation.'}
            $checks.Add("Actual handover recovery respects installation ownership: $kind")
        }
    }
    $report=[pscustomobject]@{passed=$true;recorded_at=[DateTime]::UtcNow.ToString('o');checks=$checks.ToArray();scope='Private files, private named mutex, and exact owned noninteractive helpers; production rollback body with desktop RPC mocked; no elevation or real installation'}
    if ($ReportPath) {[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))}
    $report|ConvertTo-Json -Depth 5
} finally {
    Exit-MoonInstallLock $lock
    if($child){if(!$child.HasExited){$child.Kill();$child.WaitForExit()};$child.Dispose()}
    if($keeper){$keeper.Dispose()}
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
