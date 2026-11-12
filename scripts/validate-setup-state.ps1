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
    $report=[pscustomobject]@{passed=$true;checks=$checks.ToArray();uac_requested=$false;installation_changed=$false}
    if($ReportPath){[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))}
    $report|ConvertTo-Json -Depth 5
}finally{Remove-Item -LiteralPath $temporary -Recurse -Force}
