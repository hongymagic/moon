[CmdletBinding()]
param([string]$ReportPath)
. (Join-Path $PSScriptRoot 'install-common.ps1')
$checks=[Collections.Generic.List[string]]::new()
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-setup-check-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
$operation=[guid]::NewGuid().ToString('N')
try {
    $denied=[Management.Automation.ErrorRecord]::new([ComponentModel.Win32Exception]::new(5),'denied',[Management.Automation.ErrorCategory]::PermissionDenied,$null)
    $cancelled=[Management.Automation.ErrorRecord]::new([InvalidOperationException]::new('wrapped',[ComponentModel.Win32Exception]::new(1223)),'cancelled',[Management.Automation.ErrorCategory]::OperationStopped,$null)
    if((Test-MoonApprovalCancelled $denied) -or !(Test-MoonApprovalCancelled $cancelled)){throw 'Windows approval cancellation was misclassified.'}
    $checks.Add('Windows approval cancellation is distinguished from access denial through wrapped exceptions')
    $rejected=$false;try{Get-MoonSetupFiles '..\arbitrary'|Out-Null}catch{$rejected=$true}
    if(!$rejected){throw 'Setup status accepted an arbitrary output path.'}
    $checks.Add('Status output accepts only a bounded operation identity')
    # Substitute only the status location in this isolated test process.
    function Get-MoonSetupFiles([string]$OperationId) {[pscustomobject]@{directory=$temporary;status=(Join-Path $temporary ($OperationId+'.json'));cancel=(Join-Path $temporary ($OperationId+'.cancel'))}}
    Write-MoonSetupStatus $operation 'working' 'Preparing Moon.'
    Write-MoonSetupStatus $operation 'failed' 'Your previous setup was preserved.' "Detail line one`nDetail line two"
    $actual=Get-Content -Raw -LiteralPath (Get-MoonSetupFiles $operation).status|ConvertFrom-Json
    if($actual.operation -ne $operation -or $actual.status -ne 'failed' -or $actual.detail -ne "Detail line one`nDetail line two"){throw 'Setup status publication lost its identity, outcome or diagnostic detail.'}
    $checks.Add('Atomic status updates keep friendly copy separate from diagnostic details')
    Test-MoonSetupCancellation $operation
    [IO.File]::WriteAllText((Get-MoonSetupFiles $operation).cancel,'cancel')
    $rejected=$false;try{Test-MoonSetupCancellation $operation}catch{if($_.Exception -is [OperationCanceledException]){$rejected=$true}}
    if(!$rejected){throw 'Cancellation did not reach the transaction checkpoint.'}
    $checks.Add('Cooperative cancellation reaches a safe transaction checkpoint without terminating a process')
    # Run the actual production trap in a noninteractive child so its exit code
    # and stderr are checked too. Only private status files are used; no setup
    # body, task, process launch from Moon, or elevation is invoked.
    $tokens=$null;$errors=$null
    $handover=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'handover.ps1'),[ref]$tokens,[ref]$errors)
    if($errors.Count){throw ($errors|Out-String)}
    $initializers=@($handover.EndBlock.Statements|Where-Object {$_ -is [Management.Automation.Language.IfStatementAst] -and $_.Extent.Text -match '^if \(!\$OperationId\)'})
    if($initializers.Count -ne 1 -or $handover.EndBlock.Traps.Count -ne 1){throw 'Expected one production operation initializer and error trap.'}
    $initializer=$initializers[0].Extent.Text
    $trapText=$handover.EndBlock.Traps[0].Extent.Text
    $common=(Join-Path $PSScriptRoot 'install-common.ps1').Replace("'","''")
    $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    foreach($kind in @('nested-error','generated-operation','malformed-status','malformed-handover','cancelled')){
        $fixture=Join-Path $temporary $kind;[void][IO.Directory]::CreateDirectory($fixture)
        $supplied=if($kind -eq 'generated-operation'){''}else{[guid]::NewGuid().ToString('N')}
        $priorStatus=if($kind -eq 'cancelled'){'cancelled'}else{'failed'}
        $child=Join-Path $fixture 'trap.ps1'
        $childText=@"
. '$common'
`$OperationId='$supplied'
`$fixtureDirectory='$($fixture.Replace("'","''"))'
`$StatePath=Join-Path `$fixtureDirectory 'handover.json'
function Get-MoonSetupFiles([string]`$OperationId){[pscustomobject]@{directory=`$fixtureDirectory;status=(Join-Path `$fixtureDirectory 'status.json');cancel=(Join-Path `$fixtureDirectory 'cancel')}}
$initializer
[IO.File]::WriteAllText(`$StatePath,'{"status":"rolled_back"}')
Write-MoonSetupStatus `$OperationId '$priorStatus' 'Installer failed and recovered.' 'Original elevated installer cause: fixture access denied.'
if('$kind' -eq 'malformed-status'){[IO.File]::WriteAllText((Get-MoonSetupFiles `$OperationId).status,'invalid JSON')}
if('$kind' -eq 'malformed-handover'){[IO.File]::WriteAllText(`$StatePath,'invalid JSON')}
$trapText
throw [IO.IOException]::new('Generic child exit 1')
"@
        [IO.File]::WriteAllText($child,$childText,[Text.UTF8Encoding]::new($false))
        $result=Invoke-MoonNative $powershell @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',$child) -AllowFailure
        $expectedExit=if($kind -eq 'cancelled'){1223}else{1}
        if($result.ExitCode -ne $expectedExit){throw "Handover trap returned unexpected exit status for $kind : $($result.ExitCode)"}
        $actual=Get-Content -Raw -LiteralPath (Join-Path $fixture 'status.json')|ConvertFrom-Json
        if($actual.operation -notmatch '^[a-f0-9]{32}$' -or ($supplied -and $actual.operation -ne $supplied)){throw 'Handover lost or failed to create its diagnostic operation identity.'}
        if($actual.detail -notlike '*Generic child exit 1*' -or $result.Error -notlike '*Generic child exit 1*' -or $result.Error -notlike '*Setup operation:*'){throw "Handover did not preserve the caller error in both status and stderr for $kind."}
        $stderrText=$result.Error -replace '\s+',' '
        if($kind -ne 'malformed-status' -and ($actual.detail -notlike '*Original elevated installer cause*' -or $stderrText -notlike '*Original elevated installer cause*')){throw "Handover overwrote the elevated installer's diagnostic in $kind."}
        if($actual.status -ne $priorStatus){throw "Handover misclassified the final status in $kind."}
        $checks.Add("Actual handover error trap preserves status, stderr and operation identity: $kind")
    }
    $report=[pscustomobject]@{passed=$true;checks=$checks.ToArray();uac_requested=$false;installation_changed=$false}
    if($ReportPath){[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))}
    $report|ConvertTo-Json -Depth 5
}finally{Remove-Item -LiteralPath $temporary -Recurse -Force}
