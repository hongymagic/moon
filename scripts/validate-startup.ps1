# Private in-memory task fixtures only: never connects to or modifies Task Scheduler.
[CmdletBinding()]
param([string]$ReportPath)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'install-common.ps1')
$checks=[Collections.Generic.List[string]]::new()
$directory=Join-Path ([IO.Path]::GetTempPath()) ('moon-startup-check-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($directory)
$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$state=[pscustomobject]@{version=1;install_id=('a'*32);owner_sid=$owner;task_name=('moon-'+$owner);config_path=$null}
$target='C:\Program Files\moon\moon.exe'
$originalSddl='O:BAG:SYD:(A;;FA;;;BA)(A;;FA;;;SY)(A;;FR;;;'+$owner+')'
$script:fixtureSddl=$originalSddl;$script:securityFlags=$null
$action=[pscustomobject]@{Type=0;Path=(Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe');Arguments=('-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand '+[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes((Get-MoonTaskLaunchScript $target (Get-MoonLaunchArguments $state)))))}
$actions=[pscustomobject]@{Count=1;Value=$action}
$actions|Add-Member ScriptMethod Item {param($index) if($index -ne 1){throw 'Unexpected action index.'};$this.Value}
$script:taskFixture=[pscustomobject]@{Enabled=$true;Definition=[pscustomobject]@{Actions=$actions;RegistrationInfo=[pscustomobject]@{Description="moon installation $($state.install_id); owner $owner"};Principal=[pscustomobject]@{RunLevel=0;UserId=$owner}}}
$taskFixture|Add-Member ScriptMethod GetSecurityDescriptor {param($sections) if($sections -ne 7){throw 'Unexpected security sections.'};$script:fixtureSddl}
$taskFixture|Add-Member ScriptMethod SetSecurityDescriptor {param($sddl,$flags) $script:fixtureSddl=$sddl;$script:securityFlags=$flags}
$folder=[pscustomobject]@{}
$folder|Add-Member ScriptMethod GetTask {param($name) if($name -ne $script:state.task_name){throw 'Unexpected task name.'};$script:taskFixture}
$service=[pscustomobject]@{}
$service|Add-Member ScriptMethod Connect {}
$service|Add-Member ScriptMethod GetFolder {param($path) if($path -ne '\'){throw 'Unexpected task folder.'};$script:folder}
# Replacing only this disposable process's COM constructor prevents all native task access.
function New-Object {param([string]$ComObject) if($ComObject -ne 'Schedule.Service'){throw 'Unexpected COM constructor.'};$script:service}
function Get-MoonInstallDirectory {$script:directory}
function Read-MoonInstallState([string]$Directory) {if($Directory -ne $script:directory){throw 'Unexpected metadata path.'};$script:state}
try {
    $grant=Get-MoonStartupTaskSecurity $originalSddl $owner
    $before=[Security.AccessControl.RawSecurityDescriptor]::new($originalSddl)
    $after=[Security.AccessControl.RawSecurityDescriptor]::new($grant)
    if($before.Owner -ne $after.Owner -or $before.Group -ne $after.Group -or $before.ControlFlags -ne $after.ControlFlags){throw 'The startup grant changed descriptor ownership or flags.'}
    $oldAces=@($before.DiscretionaryAcl | ForEach-Object {$bytes=[byte[]]::new($_.BinaryLength);$_.GetBinaryForm($bytes,0);[Convert]::ToBase64String($bytes)})
    $newAces=@($after.DiscretionaryAcl | ForEach-Object {$bytes=[byte[]]::new($_.BinaryLength);$_.GetBinaryForm($bytes,0);[Convert]::ToBase64String($bytes)})
    foreach($ace in $oldAces){if($newAces -notcontains $ace){throw 'An existing task ACE was removed or modified.'}}
    $added=@($after.DiscretionaryAcl | Where-Object {$_.SecurityIdentifier.Value -eq $owner -and $_.AccessMask -eq 0x1201bf})
    if($added.Count -ne 1 -or $after.DiscretionaryAcl.Count -ne ($before.DiscretionaryAcl.Count+1) -or ($added[0].AccessMask -band 0xd0000) -ne 0){throw 'The task grant changed another principal or granted delete/ACL/owner rights.'}
    if((Get-MoonStartupTaskSecurity $grant $owner) -cne $grant){throw 'The grant is not idempotent.'}
    $checks.Add('Owner read/write/execute grant retains every original ACE, owner, group and flags; no delete/WRITE_DAC/WRITE_OWNER and no duplicate grant')
    Set-MoonTaskSecurity $state $target $grant
    if($script:fixtureSddl -cne $grant -or $script:securityFlags -ne 16){throw 'The production setter did not preserve explicit ACL ownership semantics.'}
    $checks.Add('Validated task security setter uses TASK_DONT_ADD_PRINCIPAL_ACE')
    if(!(Get-MoonTaskEnabled $state $target)){throw 'The fixture should initially be enabled.'}
    Set-MoonTaskEnabled $state $target $false
    if($taskFixture.Enabled){throw 'Disabling startup did not persist.'}
    Set-MoonTaskEnabled $state $target $true
    if(!$taskFixture.Enabled){throw 'Enabling startup did not persist.'}
    $checks.Add('Actual ownership adapter and public Enabled setter support both directions without task replacement')
    $originalPath=$action.Path;$action.Path='C:\unrelated.exe';$refused=$false
    try{Set-MoonTaskEnabled $state $target $false}catch{$refused=$true}
    if(!$refused -or !$taskFixture.Enabled){throw 'An unrelated task action was changed.'};$action.Path=$originalPath
    $taskFixture.Definition.Principal.RunLevel=1;$refused=$false
    try{Set-MoonTaskEnabled $state $target $false}catch{$refused=$true}
    if(!$refused -or !$taskFixture.Enabled){throw 'A highest-privilege task was changed.'};$taskFixture.Definition.Principal.RunLevel=0
    $checks.Add('Foreign action and elevated task refusal occur before enabled-state mutation')
    $status=Get-MoonStartupStatus
    if($status.available -or $status.enabled){throw 'Missing installation was advertised as controllable.'}
    [IO.File]::WriteAllText((Join-Path $directory 'installation.json'),'private fixture')
    # The real helper resolves moon.exe beneath the mock installation directory.
    $target=Join-Path $directory 'moon.exe'
    $action.Arguments='-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand '+[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes((Get-MoonTaskLaunchScript $target (Get-MoonLaunchArguments $state))))
    $status=Set-MoonStartupEnabled $false
    if(!$status.available -or $status.enabled -or $status.message -notmatch 'does not stop'){throw 'Disabled startup status is not accurate.'}
    $status=Set-MoonStartupEnabled $true
    if(!$status.available -or !$status.enabled){throw 'Enabled startup status is not accurate.'}
    $checks.Add('Public status distinguishes missing installation and both saved preferences without stopping any process')
    $tokens=$null;$errors=$null;$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'install.ps1'),[ref]$tokens,[ref]$errors)
    if($errors.Count){throw ($errors|Out-String)}
    $settings=@($ast.FindAll({param($n)$n -is [Management.Automation.Language.CommandAst] -and $n.GetCommandName() -eq 'New-ScheduledTaskSettingsSet'},$true))
    if($settings.Count -ne 1){throw 'Expected one persistent-task settings construction.'}
    function New-ScheduledTaskSettingsSet {param([switch]$AllowStartIfOnBatteries,[switch]$DontStopIfGoingOnBatteries,$ExecutionTimeLimit,$MultipleInstances,[switch]$Disable) [pscustomobject]@{Enabled=(-not [bool]$Disable)}}
    foreach($startupEnabled in @($true,$false)) {
        $prepared=& ([scriptblock]::Create($settings[0].Extent.Text))
        if($prepared.Enabled -ne $startupEnabled){throw 'Upgrade registration lost the saved startup preference.'}
    }
    $checks.Add('Actual installer settings preserve both enabled and disabled preferences at registration')
    $rollback=@($ast.FindAll({param($n)$n -is [Management.Automation.Language.IfStatementAst] -and $n.Clauses[0].Item1.Extent.Text -eq '$taskChanged'},$true))
    if($rollback.Count -ne 1){throw 'Expected one task rollback branch.'}
    $script:restoredXml=$null
    function Register-ScheduledTask {param($TaskName,$Xml,[switch]$Force) if($TaskName -ne $script:state.task_name){throw 'Unexpected rollback task.'};$script:restoredXml=$Xml}
    function Unregister-ScheduledTask {throw 'The existing task must be restored, not deleted.'}
    $taskChanged=$true;$taskName=$state.task_name;$oldTaskXml='<Task>private exact previous XML</Task>';$oldTaskSecurity=$originalSddl;$previousState=$state
    & ([scriptblock]::Create($rollback[0].Extent.Text))
    if($script:restoredXml -cne $oldTaskXml -or $script:fixtureSddl -cne $oldTaskSecurity){throw 'Installer failure did not restore exact XML and security descriptor.'}
    $checks.Add('Actual installation failure branch restores original XML and SDDL after task-access mutation')
    $launch=@($ast.FindAll({param($n)$n -is [Management.Automation.Language.IfStatementAst] -and $n.Extent.Text -match 'elseif \(\$startupEnabled\)'},$true))
    # Select the nearest statement, excluding containing try/script blocks.
    if($launch.Count -ne 1){throw 'Expected one persistent/one-shot launch selection.'}
    function Start-ScheduledTask {param($TaskName) $script:launchChoice='persistent'}
    function Start-MoonLimitedOnce {param($Target,$Arguments,$State) $script:launchChoice='one-shot'}
    $portablePeer=$null;$daemon=$null
    foreach($startupEnabled in @($true,$false)) {
        $script:launchChoice=$null
        & ([scriptblock]::Create($launch[0].Extent.Text))
        $expected=if($startupEnabled){'persistent'}else{'one-shot'}
        if($script:launchChoice -ne $expected){throw 'Disabled persistent startup prevented the authorized replacement session.'}
    }
    $checks.Add('Actual installer launch branch uses a one-shot replacement while preserving disabled sign-in startup')
    $report=[pscustomobject]@{passed=$true;checks=$checks.ToArray();count=$checks.Count;native_task_connections=0;native_task_changes=0;desktop_actions=0;recorded_at=[DateTime]::UtcNow.ToString('o')}
    if($ReportPath){[IO.File]::WriteAllText([IO.Path]::GetFullPath($ReportPath),($report|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))}
    $report|ConvertTo-Json -Depth 6
} finally {if(Test-Path -LiteralPath $directory){Remove-Item -LiteralPath $directory -Recurse -Force}}
