[CmdletBinding()]
param([string]$ReportPath)
. (Join-Path $PSScriptRoot 'install-common.ps1')
$directory=Join-Path ([IO.Path]::GetTempPath()) ('moon-upgrade-check-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($directory)
$script:journalPath=Join-Path $directory 'journal.json'
$script:identity=[MoonInstallNative]::ProcessIdentity([uint32]$PID)
$script:revision=5;$script:queries=0;$script:layouts=0;$script:scenario='lag'
function New-TestWorld([long]$Revision) {
    $world=@'
{"revision":5,"focused":101,"active_monitor":1,"active":{"1":0},"workspaces":[{"id":0,"monitor":1,"focused":101,"tree":{"type":"split","axis":"horizontal","share":1,"children":[{"type":"window","id":101,"share":0.5,"hint":null},{"type":"window","id":102,"share":0.5,"hint":null}]}},{"id":1,"monitor":1,"focused":null,"tree":{"type":"split","axis":"horizontal","share":1,"children":[]}}],"windows":{"101":{"workspace":0,"floating":false,"fullscreen":false,"suspended":false,"floating_rect":{"x":0,"y":0,"width":50,"height":100}},"102":{"workspace":0,"floating":false,"fullscreen":false,"suspended":false,"floating_rect":{"x":50,"y":0,"width":50,"height":100}}}}
'@ | ConvertFrom-Json
    $world.revision=$Revision
    return $world
}
$script:placements=@'
[{"id":101,"visible":true,"floating":false,"fullscreen":false,"rect":{"x":0,"y":0,"width":50,"height":100}},{"id":102,"visible":true,"floating":false,"fullscreen":false,"rect":{"x":50,"y":0,"width":50,"height":100}}]
'@ | ConvertFrom-Json
function Write-TestJournal([long]$Revision,[long]$Birth=$script:identity.Birth) {
    $record=@{version=1;owner=$PID;owner_birth=$Birth;world=@{revision=$Revision}}
    [IO.File]::WriteAllText($script:journalPath,($record | ConvertTo-Json -Depth 4),[Text.UTF8Encoding]::new($false))
}
# Override only this disposable test process's RPC functions. No daemon or
# installed state is contacted; the actual snapshot reader operates on our file.
function Invoke-MoonRpc([string]$Method,[string]$Query) {
    if ($Query -eq 'windows') {
        $script:layouts++
        if ($script:scenario -eq 'lag' -and $script:layouts -eq 2) {Write-TestJournal $script:revision}
        return $script:placements
    }
    $script:queries++
    if ($script:scenario -eq 'changing' -and $script:queries -eq 2) {
        $script:revision=6;Write-TestJournal 6
    }
    New-TestWorld $script:revision
}
try {
    Write-TestJournal 4
    $snapshot=Get-MoonUpgradeSnapshot $script:journalPath ([uint32]$PID)
    $saved=([Text.Encoding]::UTF8.GetString($snapshot.journal)) | ConvertFrom-Json
    $expected=Get-MoonLayoutFingerprint -World (New-TestWorld 5) -Placements $script:placements
    if ($script:queries -lt 4 -or $saved.world.revision -ne 5 -or $snapshot.layout -cne $expected) {throw 'A lagging journal was accepted before it matched the queried layout.'}
    $script:scenario='changing';$script:queries=0;$script:layouts=0
    Write-TestJournal 5
    $snapshot=Get-MoonUpgradeSnapshot $script:journalPath ([uint32]$PID)
    if ($script:queries -lt 4 -or $snapshot.revision -ne 6 -or $snapshot.layout -cne $expected) {throw 'A layout revision changed during capture without a retry.'}
    $script:scenario='wrong-owner'
    Write-TestJournal 6 ($script:identity.Birth+1)
    $hash=(Get-FileHash -LiteralPath $script:journalPath -Algorithm SHA256).Hash
    $rejected=$false
    try {Get-MoonUpgradeSnapshot $script:journalPath ([uint32]$PID) -TimeoutMilliseconds 100 | Out-Null} catch {$rejected=$true}
    if (!$rejected -or (Get-FileHash -LiteralPath $script:journalPath -Algorithm SHA256).Hash -ne $hash) {throw 'A stale owner identity was accepted or its journal was changed.'}
    $checks=[Collections.Generic.List[string]]::new()
    foreach ($check in @('Lagging checkpoint retries until matching layout','Revision changes during capture retry','Stale owner creation time refuses unchanged')) {$checks.Add($check)}
    if ((Get-MoonLayoutFingerprint -World (New-TestWorld 1000) -Placements $script:placements) -cne $expected) {throw 'A revision-only change altered the layout fingerprint.'}
    $checks.Add('Revision-only changes are excluded from layout semantics')
    foreach ($mutation in @(
        @{name='Global focus';change={param($world) $world.focused=102}},
        @{name='Active workspace';change={param($world) $world.active.'1'=1}},
        @{name='Tree child order';change={param($world) $world.workspaces[0].tree.children=@($world.workspaces[0].tree.children[1],$world.workspaces[0].tree.children[0])}},
        @{name='Window workspace membership';change={param($world) $world.windows.'101'.workspace=1}}
    )) {
        $changed=New-TestWorld 5
        & $mutation.change $changed
        if ((Get-MoonLayoutFingerprint -World $changed -Placements $script:placements) -ceq $expected) {throw ($mutation.name+' change was omitted from the upgrade fingerprint.')}
        $checks.Add($mutation.name+' changes are detected with identical rectangles')
    }
    $legacy=[pscustomobject]@{install_id=('a'*32);owner_sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value}
    if ((Get-MoonLaunchArguments $legacy) -cne 'start --background' -or (Get-MoonLaunchArguments $legacy -Resume) -cne 'start --background --resume' -or (Get-MoonLaunchArguments $legacy -Settings) -cne '') {throw 'Legacy launch arguments changed.'}
    $checks.Add('Legacy metadata retains identical startup, recovery and Settings arguments')
    $config=Join-Path $directory "my settings ' moon.yaml"
    [IO.File]::WriteAllText($config,'theme: linen')
    $configured=[pscustomobject]@{install_id=$legacy.install_id;owner_sid=$legacy.owner_sid;config_path=$config}
    $quoted=ConvertTo-MoonArgument $config
    if ((Get-MoonLaunchArguments $configured) -cne ('start --background --config '+$quoted) -or (Get-MoonLaunchArguments $configured -Resume) -cne ('start --background --resume --config '+$quoted) -or (Get-MoonLaunchArguments $configured -Settings) -cne ('--config '+$quoted)) {throw 'Configured launch arguments lost their path or resume boundary.'}
    $checks.Add('Custom config paths with spaces and apostrophes survive startup, one-shot resume and Settings quoting')
    foreach($bad in @('relative.yaml','C:relative.yaml','\rooted-but-no-drive.yaml','',42,$false)) {
        $refused=$false
        try {Get-MoonLaunchArguments ([pscustomobject]@{config_path=$bad}) | Out-Null} catch {$refused=$true}
        if(!$refused){throw 'Malformed configuration metadata was accepted.'}
    }
    $checks.Add('Malformed, relative and drive-relative config metadata is refused')
    $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $target='C:\Program Files\moon\moon.exe'
    function New-TestTask([string]$Launch) {
        [pscustomobject]@{Description="moon installation $($legacy.install_id); owner $($legacy.owner_sid)";Principal=[pscustomobject]@{RunLevel='Limited';UserId=$legacy.owner_sid};Actions=@([pscustomobject]@{Execute=$powershell;Arguments=('-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand '+[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($Launch)))})}
    }
    $oldLaunch="Start-Process -FilePath '$target' -ArgumentList 'start --background' -WindowStyle Hidden"
    Assert-MoonOwnedTask (New-TestTask $oldLaunch) $legacy $target
    Assert-MoonOwnedTask (New-TestTask (Get-MoonTaskLaunchScript $target)) $legacy $target
    Assert-MoonOwnedTask (New-TestTask (Get-MoonTaskLaunchScript $target (Get-MoonLaunchArguments $configured))) $configured $target
    foreach($launch in @($oldLaunch,(Get-MoonTaskLaunchScript $target),(Get-MoonTaskLaunchScript $target (Get-MoonLaunchArguments $configured -Resume)))) {
        $refused=$false
        try {Assert-MoonOwnedTask (New-TestTask $launch) $configured $target} catch {$refused=$true}
        if(!$refused){throw 'A configured task accepted another config or a logon resume operation.'}
    }
    $checks.Add('Task ownership accepts legacy/current shapes but rejects missing custom config and logon --resume')
    $tokens=$null;$parseErrors=$null
    $installer=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'install.ps1'),[ref]$tokens,[ref]$parseErrors)
    if($parseErrors.Count){throw ($parseErrors|Out-String)}
    $selection=$installer.Find({param($node) $node -is [Management.Automation.Language.AssignmentStatementAst] -and $node.Left.Extent.Text -eq '$state.config_path' -and $node.Right.Extent.Text -match 'Get-MoonSelectedConfigPath'},$true)
    if(!$selection){throw 'Installer does not derive its launch selection from the live daemon.'}
    $deriveSelection=[scriptblock]::Create($selection.Extent.Text)
    $selectionFile=Join-Path $directory 'default-config.toml'
    foreach($exists in @($false,$true)) {
        if($exists){[IO.File]::WriteAllText($selectionFile,'config-version = 1')}
        foreach($explicit in @($false,$true)) {
            $daemon=[pscustomobject]@{config_path=$selectionFile;config_explicit=$explicit}
            $state=[pscustomobject]@{config_path=$config}
            . $deriveSelection
            $suffix=if($explicit){' --config '+(ConvertTo-MoonArgument $selectionFile)}else{''}
            if((Get-MoonLaunchArguments $state) -cne ('start --background'+$suffix) -or (Get-MoonLaunchArguments $state -Resume) -cne ('start --background --resume'+$suffix) -or (Get-MoonLaunchArguments $state -Settings) -cne $suffix.TrimStart()){throw 'Installer changed implicit/explicit settings selection in a launch argument.'}
            $actualIdentity=Get-MoonConfigIdentity $daemon
            if($actualIdentity.config_path -ne $selectionFile -or $actualIdentity.config_exists -ne $exists){throw 'Launch selection lost the actual path used for configuration change detection.'}
            if((Test-Path -LiteralPath $selectionFile) -ne $exists){throw 'Setup created an absent configuration file.'}
        }
    }
    if([IO.File]::ReadAllText($selectionFile) -ne 'config-version = 1'){throw 'Setup modified the existing configuration.'}
    $checks.Add('Actual installer selection preserves implicit defaults and explicit files across startup, resume and Settings, whether the file exists or not')
    foreach($oldDaemon in @([pscustomobject]@{config_path=$selectionFile},[pscustomobject]@{config_path=$selectionFile;config_explicit='false'},[pscustomobject]@{config_path=$selectionFile;config_explicit=$null})) {
        $daemon=$oldDaemon;$state=[pscustomobject]@{config_path=$config};$refused=$false
        try {. $deriveSelection} catch {$refused=$true}
        if(!$refused -or $state.config_path -ne $config){throw 'An unknown or malformed live selection was guessed or persisted.'}
    }
    $checks.Add('Older or malformed live selection metadata fails closed without guessing from the path or changing prior state')
    $daemon=[pscustomobject]@{process_id=$PID;executable=$script:identity.Path;config_path=$config;config_explicit=$true}
    $captured=Get-MoonDesktopIdentity $daemon
    Assert-MoonDesktopIdentity $captured $daemon
    foreach($field in @('process_id','process_birth','path','sha256','launch_config_path')) {
        $altered=$captured | Select-Object *
        switch($field){'process_id'{$altered.process_id++};'process_birth'{$altered.process_birth++};'path'{$altered.path+='-changed'};'sha256'{$altered.sha256='0'*64};'launch_config_path'{$altered.launch_config_path=$null}}
        $refused=$false
        try {Assert-MoonDesktopIdentity $altered $daemon} catch {$refused=$true}
        if(!$refused){throw "A changed captured identity was accepted: $field"}
    }
    $checks.Add('Captured daemon PID, birth, native executable path and file hash are rechecked')
    [IO.File]::WriteAllText($config,'theme: changed')
    $refused=$false
    try {Assert-MoonDesktopIdentity $captured $daemon} catch {$refused=$true}
    if(!$refused){throw 'A changed config was accepted before shutdown.'}
    Remove-Item -LiteralPath $config
    $missing=Get-MoonConfigIdentity $daemon
    if($missing.config_exists -or $missing.config_sha256){throw 'An absent default config was not retained as absent.'}
    [IO.File]::WriteAllText($config,'theme: linen')
    $refused=$false
    try {Assert-MoonConfigIdentity $missing $daemon} catch {$refused=$true}
    if(!$refused){throw 'A newly created config was accepted as an unchanged absent file.'}
    $checks.Add('Config edits and creation are detected without rewriting or creating the user configuration')
    $script:scenario='stable';$script:queries=0
    Write-TestJournal $script:revision
    $snapshot=Get-MoonUpgradeSnapshot $script:journalPath ([uint32]$PID)
    $checkpoint=Save-MoonDesktopCheckpoint $snapshot $captured (Join-Path $directory 'retained')
    if ((Get-MoonBytesHash (Read-MoonDesktopCheckpoint $checkpoint)) -ne (Get-MoonBytesHash $snapshot.journal) -or !(Test-Path -LiteralPath $checkpoint.receipt_path)) {throw 'Exact recovery bytes were not durably retained with their receipt.'}
    $checks.Add('Recovery checkpoint bytes and identity receipt persist before any shutdown operation')
    $wrongSnapshot=$snapshot | Select-Object *;$wrongSnapshot.revision++
    $refused=$false
    try {Save-MoonDesktopCheckpoint $wrongSnapshot $captured (Join-Path $directory 'invalid') | Out-Null} catch {$refused=$true}
    if(!$refused -or (Test-Path -LiteralPath (Join-Path $directory 'invalid'))){throw 'A mismatched checkpoint created recovery state.'}
    $checks.Add('Mismatched checkpoint identity/revision fails before writing recovery state')
    $destination=Join-Path $directory 'resume.json'
    Publish-MoonDesktopCheckpoint $checkpoint $destination
    $published=(Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash
    $refused=$false
    try {Publish-MoonDesktopCheckpoint $checkpoint $destination} catch {$refused=$true}
    if(!$refused -or (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ne $published){throw 'Publication replaced an existing live journal.'}
    $checks.Add('Checkpoint publication is atomic and refuses to overwrite an existing recovery journal')
    [IO.File]::WriteAllText($checkpoint.journal_path,'changed')
    $refused=$false
    try {Publish-MoonDesktopCheckpoint $checkpoint (Join-Path $directory 'tampered.json')} catch {$refused=$true}
    if(!$refused -or (Test-Path -LiteralPath (Join-Path $directory 'tampered.json'))){throw 'Modified retained bytes were replayed.'}
    $checks.Add('Changed checkpoint bytes remain retained and are never replayed')
    $report=[pscustomobject]@{passed=$checks.Count;checks=$checks.ToArray();daemon_contacted=$false;installation_changed=$false;uac_requested=$false}
    if($ReportPath){[IO.File]::WriteAllText([IO.Path]::GetFullPath($ReportPath),($report|ConvertTo-Json -Depth 4),[Text.UTF8Encoding]::new($false))}
    $report | ConvertTo-Json -Depth 4
} finally {Remove-Item -LiteralPath $directory -Recurse -Force}
