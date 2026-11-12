[CmdletBinding()]
param()
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
    [pscustomobject]@{passed=$checks.Count;checks=$checks.ToArray();daemon_contacted=$false;installation_changed=$false;uac_requested=$false} | ConvertTo-Json -Depth 4
} finally {Remove-Item -LiteralPath $directory -Recurse -Force}
