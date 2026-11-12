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
    foreach($kind in @('nested-error','generated-operation','malformed-status','malformed-handover','cancelled','stopped-rollback')){
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
[IO.File]::WriteAllText(`$StatePath,'{"status":"$(if($kind -eq 'stopped-rollback'){'rolled_back_stopped'}else{'rolled_back'})"}')
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
        if($kind -eq 'stopped-rollback' -and ($actual.message -notlike '*remains stopped*' -or $actual.message -like '*has been restored*')){throw 'Stopped rollback incorrectly claimed to restart the previous desktop.'}
        $checks.Add("Actual handover error trap preserves status, stderr and operation identity: $kind")
    }
    $restore=$handover.Find({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Restore-PreviousDesktop'},$true)
    if(!$restore){throw 'Production desktop restoration helper was not found.'}
    & {
        Invoke-Expression $restore.Extent.Text
        function Assert-MoonStopped {$script:stoppedChecked=$true}
        function Get-PreviousDesktopProcesses {if($script:fixtureRunning){[pscustomobject]@{Name='fixture.exe'}}}
        function Start-Process {param($FilePath,$WorkingDirectory,$ArgumentList) $script:previousLaunches++;$script:fixtureRunning=$true}
        $names=@('fixture.exe')
        $backup=Join-Path $temporary 'previous-shortcut.backup';$original=Join-Path $temporary 'previous-shortcut'
        [IO.File]::WriteAllText($backup,'owned previous startup bytes')
        $saved=[pscustomobject]@{startup_links=@([pscustomobject]@{backup=$backup;original=$original;sha256=(Get-FileHash $backup).Hash;program='fixture.exe'});processes=@([pscustomobject]@{name='fixture.exe';path=$backup;arguments=''})}
        foreach($keepStopped in @($false,$true)){
            if(Test-Path -LiteralPath $original){Remove-Item -LiteralPath $original}
            $script:previousLaunches=0;$script:fixtureRunning=$false;$script:stoppedChecked=$false
            Restore-PreviousDesktop $saved -KeepPreviousDesktopStopped:$keepStopped
            if(!$script:stoppedChecked -or [IO.File]::ReadAllText($original) -ne 'owned previous startup bytes'){throw 'Desktop rollback skipped safe state restoration.'}
            if($script:previousLaunches -ne $(if($keepStopped){0}else{1})){throw 'Desktop rollback ignored the explicit keep-stopped policy.'}
            $checks.Add("Actual previous-desktop restoration retains safe state and honors keep-stopped=$keepStopped")
        }
    }

    $savedLocalAppData=$env:LOCALAPPDATA
    try {
        foreach($kind in @('missing-image','child-nonzero','child-success','wrapped-launch-error','bounded-wait')){
            $fixture=Join-Path $temporary ('startup-'+$kind);[void][IO.Directory]::CreateDirectory($fixture)
            $worker=Join-Path $fixture 'worker.ps1'
            $testTarget=if($kind -eq 'missing-image'){Join-Path $fixture 'absent.exe'}else{$powershell}
            $childExit=if($kind -eq 'child-nonzero'){37}else{0}
            $testArguments='-NoProfile -NonInteractive -EncodedCommand '+[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes("exit $childExit"))
            $prefix="`$env:LOCALAPPDATA='$($fixture.Replace("'","''"))'`n"
            if($kind -eq 'wrapped-launch-error'){
                $prefix+="function Start-Process {throw [InvalidOperationException]::new('Original launch failure',[ComponentModel.Win32Exception]::new(5))}`n"
            }elseif($kind -eq 'bounded-wait'){
                $prefix+=@'
function Start-Process {
 $child=[pscustomobject]@{Id=1234;ExitCode=0}
 $child|Add-Member ScriptMethod WaitForExit {param($Milliseconds) if($Milliseconds -ne 15000){throw 'Unexpected startup wait limit'};return $false}
 $child|Add-Member ScriptMethod Kill {throw 'Slow launchers must not be terminated'}
 $child|Add-Member ScriptMethod Dispose {}
 return $child
}
'@
                $prefix+="`n"
            }
            [IO.File]::WriteAllText($worker,($prefix+(Get-MoonTaskLaunchScript $testTarget $testArguments)),[Text.UTF8Encoding]::new($false))
            $since=[DateTime]::UtcNow
            $result=Invoke-MoonNative $powershell @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',$worker) -TimeoutSeconds 20 -AllowFailure
            $status=Join-Path $fixture ('moon\startup-session'+[Diagnostics.Process]::GetCurrentProcess().SessionId+'.json')
            $actual=Get-Content -Raw -LiteralPath $status|ConvertFrom-Json
            if($actual.target -ne $testTarget -or $actual.arguments -ne $testArguments -or !$actual.environment.local_app_data -or (Get-Item $status).Length -gt 65536){throw 'Startup record lost its identity, context or bound.'}
            switch($kind){
                'missing-image' {if($result.ExitCode -ne 1 -or $actual.phase -ne 'failed' -or $actual.error -notlike ('*'+([ComponentModel.Win32Exception]::new(2)).Message+'*')){throw 'Missing image lost the original Windows launch error message.'}}
                'child-nonzero' {if($result.ExitCode -ne 37 -or $actual.phase -ne 'exited' -or $actual.exit_code -ne 37 -or !$actual.child_pid){throw 'Nonzero child status was not recorded or propagated.'}}
                'child-success' {if($result.ExitCode -ne 0 -or $actual.phase -ne 'exited' -or $actual.exit_code -ne 0 -or !$actual.child_pid){throw 'Successful child status was not recorded.'}}
                'wrapped-launch-error' {if($result.ExitCode -ne 1 -or $actual.native_error_code -ne 5 -or $actual.error -notlike '*Original launch failure*'){throw 'Wrapped Win32 cause was not preserved.'}}
                'bounded-wait' {if($result.ExitCode -ne 1460 -or $actual.phase -ne 'timed-out' -or $actual.error -notlike '*not terminated*'){throw 'Bounded wait did not preserve the running launcher.'}}
            }
            # Override only this test process's environment; no user registry or
            # task settings are changed. Exercise the real diagnostic reader.
            $env:LOCALAPPDATA=$fixture
            $diagnostic=Get-MoonTaskStartupDiagnostic $testTarget $since
            if($diagnostic -notlike '*Startup task diagnostic (*' -or $diagnostic -notlike ('*'+$actual.attempt+'*')){throw 'Installer timeout could not find its matching task diagnostic.'}
            if((Get-MoonTaskStartupDiagnostic $testTarget ([DateTime]::UtcNow.AddMinutes(1))) -notlike '*no matching diagnostic*'){throw 'An older launch record was attributed to a new attempt.'}
            [IO.File]::WriteAllText($status,'invalid JSON')
            if((Get-MoonTaskStartupDiagnostic $testTarget $since) -notlike '*could not be read*'){throw 'Malformed task diagnostics obscured the installer error.'}
            $env:LOCALAPPDATA=$savedLocalAppData
            $checks.Add("Actual startup worker records and propagates result without a task or elevation: $kind")
        }
    }finally{$env:LOCALAPPDATA=$savedLocalAppData}
    $report=[pscustomobject]@{passed=$true;checks=$checks.ToArray();uac_requested=$false;installation_changed=$false}
    if($ReportPath){[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))}
    $report|ConvertTo-Json -Depth 5
}finally{Remove-Item -LiteralPath $temporary -Recurse -Force}
