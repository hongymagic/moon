[CmdletBinding()]
param([string]$ReportPath)
. (Join-Path $PSScriptRoot 'install-common.ps1')
$checks=[Collections.Generic.List[string]]::new()
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-setup-check-'+[guid]::NewGuid().ToString('N'))
$standaloneRoot=$temporary
[void][IO.Directory]::CreateDirectory($temporary)
$operation=[guid]::NewGuid().ToString('N')
try {
    $denied=[Management.Automation.ErrorRecord]::new([ComponentModel.Win32Exception]::new(5),'denied',[Management.Automation.ErrorCategory]::PermissionDenied,$null)
    $cancelled=[Management.Automation.ErrorRecord]::new([InvalidOperationException]::new('wrapped',[ComponentModel.Win32Exception]::new(1223)),'cancelled',[Management.Automation.ErrorCategory]::OperationStopped,$null)
    if((Test-MoonApprovalCancelled $denied) -or !(Test-MoonApprovalCancelled $cancelled)){throw 'Windows approval cancellation was misclassified.'}
    $checks.Add('Windows approval cancellation is distinguished from access denial through wrapped exceptions')
    Add-Type -TypeDefinition 'public static class MoonApprovalErrorFixture {public static void Fail(int code) {throw new System.ComponentModel.Win32Exception(code);}}'
    foreach($code in @(1223,5)) {
        try {[MoonApprovalErrorFixture]::Fail($code);throw 'Expected a native error fixture'}
        catch {if((Test-MoonApprovalCancelled $_) -ne ($code -eq 1223)){throw 'PowerShell method invocation lost the native cancellation classification.'}}
    }
    $messageOnly=[Management.Automation.ErrorRecord]::new([InvalidOperationException]::new('The operation was canceled by the user.'),'message-only',[Management.Automation.ErrorCategory]::OperationStopped,$null)
    if(Test-MoonApprovalCancelled $messageOnly){throw 'Cancellation was inferred from localized text without a native error code.'}
    $checks.Add('Actual PowerShell method wrappers preserve native cancellation codes; message text alone is never classified')
    $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $exitScript=Join-Path $temporary 'ordinary shell child.ps1'
    [IO.File]::WriteAllText($exitScript,'Start-Sleep -Milliseconds 150; exit 37',[Text.UTF8Encoding]::new($false))
    $clock=[Diagnostics.Stopwatch]::StartNew()
    $exitCode=Invoke-MoonShellProcess $powershell @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',$exitScript)
    if($exitCode -ne 37 -or $clock.ElapsedMilliseconds -lt 150){throw 'The production shell launcher did not wait for and preserve its ordinary child exit status.'}
    $nativeFailure=$false
    try {Invoke-MoonShellProcess (Join-Path $temporary 'missing.exe')|Out-Null}
    catch {
        $exception=$_.Exception
        while($exception){if($exception -is [ComponentModel.Win32Exception]){$nativeFailure=$exception.NativeErrorCode -eq 2};$exception=$exception.InnerException}
        if(Test-MoonApprovalCancelled $_){throw 'Missing executable was classified as cancellation.'}
    }
    if(!$nativeFailure){throw 'The production shell launcher discarded a real native launch error.'}
    $checks.Add('Production shell launcher waits for an ordinary child and retains its exact exit status and native launch failures without UAC')
    $timeoutScript=Join-Path $temporary 'bounded ordinary shell child.ps1'
    $timeoutIdentity=Join-Path $temporary 'bounded-child.json'
    [IO.File]::WriteAllText($timeoutScript,@'
param([string]$IdentityPath)
$self=[Diagnostics.Process]::GetCurrentProcess()
[IO.File]::WriteAllText($IdentityPath,(@{pid=$PID;birth=$self.StartTime.ToFileTimeUtc()}|ConvertTo-Json))
Start-Sleep -Seconds 15
'@,[Text.UTF8Encoding]::new($false))
    $timedOut=$false;$clock.Restart();$ownedChild=$null
    try {
        try {Invoke-MoonShellProcess $powershell @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',$timeoutScript,'-IdentityPath',$timeoutIdentity) -TimeoutSeconds 3|Out-Null}
        catch {if($_.Exception.Message -notlike 'Process timed out:*'){throw};$timedOut=$true}
        if(!$timedOut -or $clock.ElapsedMilliseconds -lt 3000 -or $clock.ElapsedMilliseconds -ge 12000 -or !(Test-Path -LiteralPath $timeoutIdentity)){throw 'The finite shell wait did not exercise its owned running child.'}
        $ownedIdentity=[IO.File]::ReadAllText($timeoutIdentity)|ConvertFrom-Json
        try {$ownedChild=[Diagnostics.Process]::GetProcessById([int]$ownedIdentity.pid)} catch [ArgumentException] {}
        if($ownedChild -and $ownedChild.StartTime.ToFileTimeUtc() -eq $ownedIdentity.birth -and !$ownedChild.HasExited){throw 'The timed-out owned child remained alive when the launcher returned.'}
        $checks.Add('Production finite shell wait terminates its exact ordinary child and observes exit before returning timeout; approval waits retain the unlimited default')
    } finally {
        if($ownedChild){try {if($ownedChild.StartTime.ToFileTimeUtc() -eq $ownedIdentity.birth -and !$ownedChild.HasExited){$ownedChild.Kill();[void]$ownedChild.WaitForExit(5000)}} finally {$ownedChild.Dispose()}}
    }
    foreach($file in @('install.ps1','uninstall.ps1')) {
        $tokens=$null;$errors=$null
        $ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $file),[ref]$tokens,[ref]$errors)
        if($errors.Count){throw ($errors|Out-String)}
        $clauses=@($ast.FindAll({param($node)$node -is [Management.Automation.Language.CatchClauseAst] -and $node.Body.Extent.Text -match 'Test-MoonApprovalCancelled'},$true))
        if($clauses.Count -ne 1){throw "Expected one production Windows approval catch in $file"}
        $body=$clauses[0].Body.Extent.Text
        $handler=[scriptblock]::Create('try {[MoonApprovalErrorFixture]::Fail($code)} catch '+$body)
        foreach($code in @(1223,5)) {
            $caught=$false
            try {& $handler} catch {
                $caught=$true
                if($code -eq 1223 -and $_.Exception -isnot [OperationCanceledException]){throw 'Native cancellation did not reach the setup cancellation path.'}
                if($code -eq 5 -and $_.Exception -is [OperationCanceledException]){throw 'Access denial was converted to cancellation.'}
            }
            if(!$caught){throw 'Approval error was swallowed.'}
        }
        $checks.Add("Actual $file approval catch converts native cancellation and preserves other launch errors")
    }
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
    foreach($kind in @('nested-error','generated-operation','malformed-status','malformed-handover','cancelled','stopped-rollback','stopped-cancelled')){
        $fixture=Join-Path $temporary $kind;[void][IO.Directory]::CreateDirectory($fixture)
        $supplied=if($kind -eq 'generated-operation'){''}else{[guid]::NewGuid().ToString('N')}
        $priorStatus=if($kind -in @('cancelled','stopped-cancelled')){'cancelled'}else{'failed'}
        $child=Join-Path $fixture 'trap.ps1'
        $childText=@"
. '$common'
`$OperationId='$supplied'
`$fixtureDirectory='$($fixture.Replace("'","''"))'
`$StatePath=Join-Path `$fixtureDirectory 'handover.json'
function Get-MoonSetupFiles([string]`$OperationId){[pscustomobject]@{directory=`$fixtureDirectory;status=(Join-Path `$fixtureDirectory 'status.json');cancel=(Join-Path `$fixtureDirectory 'cancel')}}
$initializer
[IO.File]::WriteAllText(`$StatePath,'{"status":"$(if($kind -in @('stopped-rollback','stopped-cancelled')){'rolled_back_stopped'}else{'rolled_back'})"}')
Write-MoonSetupStatus `$OperationId '$priorStatus' 'Installer failed and recovered.' 'Original elevated installer cause: fixture access denied.'
if('$kind' -eq 'malformed-status'){[IO.File]::WriteAllText((Get-MoonSetupFiles `$OperationId).status,'invalid JSON')}
if('$kind' -eq 'malformed-handover'){[IO.File]::WriteAllText(`$StatePath,'invalid JSON')}
$trapText
throw [IO.IOException]::new('Generic child exit 1')
"@
        [IO.File]::WriteAllText($child,$childText,[Text.UTF8Encoding]::new($false))
        $result=Invoke-MoonNative $powershell @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',$child) -AllowFailure
        $expectedExit=if($kind -in @('cancelled','stopped-cancelled')){1223}else{1}
        if($result.ExitCode -ne $expectedExit){throw "Handover trap returned unexpected exit status for $kind : $($result.ExitCode)"}
        $actual=Get-Content -Raw -LiteralPath (Join-Path $fixture 'status.json')|ConvertFrom-Json
        if($actual.operation -notmatch '^[a-f0-9]{32}$' -or ($supplied -and $actual.operation -ne $supplied)){throw 'Handover lost or failed to create its diagnostic operation identity.'}
        if($actual.detail -notlike '*Generic child exit 1*' -or $result.Error -notlike '*Generic child exit 1*' -or $result.Error -notlike '*Setup operation:*'){throw "Handover did not preserve the caller error in both status and stderr for $kind."}
        $stderrText=$result.Error -replace '\s+',' '
        if($kind -ne 'malformed-status' -and ($actual.detail -notlike '*Original elevated installer cause*' -or $stderrText -notlike '*Original elevated installer cause*')){throw "Handover overwrote the elevated installer's diagnostic in $kind."}
        if($actual.status -ne $priorStatus){throw "Handover misclassified the final status in $kind."}
        if($kind -in @('stopped-rollback','stopped-cancelled') -and ($actual.message -notlike '*remains stopped*' -or $actual.message -like '*has been restored*')){throw 'Stopped rollback incorrectly claimed to restart the previous desktop.'}
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
    # Exercise the production setup helpers and event bodies without creating a
    # window or starting the installer. Only worker launch/status are substituted.
    $setupPath=Join-Path $PSScriptRoot 'setup.ps1'
    $tokens=$null;$errors=$null
    $setup=[Management.Automation.Language.Parser]::ParseFile($setupPath,[ref]$tokens,[ref]$errors)
    if($errors.Count){throw ($errors|Out-String)}
    & {
        foreach($name in @('Get-MoonSetupHandoverArguments','Test-MoonSetupPayload')) {
            $definition=$setup.Find({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name},$true)
            if(!$definition){throw "Missing production setup helper: $name"}
            Invoke-Expression $definition.Extent.Text
        }
        $receipt=Join-Path $temporary 'preference.json'
        $saved=[pscustomobject]@{version=1;owner_sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value;session=[Diagnostics.Process]::GetCurrentProcess().SessionId;keep_previous_desktop_stopped=$true}
        [IO.File]::WriteAllText($receipt,($saved|ConvertTo-Json))
        if((Get-MoonSetupHandoverArguments -StatePath $receipt) -ne '-KeepPreviousDesktopStopped'){throw 'Guided retry lost the saved no-fallback preference.'}
        $saved.session+=100;[IO.File]::WriteAllText($receipt,($saved|ConvertTo-Json))
        if((Get-MoonSetupHandoverArguments -StatePath $receipt) -ne '-KeepPreviousDesktopStopped'){throw 'Signing in again lost the same-account no-fallback preference.'}
        $saved.keep_previous_desktop_stopped=$false;[IO.File]::WriteAllText($receipt,($saved|ConvertTo-Json))
        if(@(Get-MoonSetupHandoverArguments -StatePath $receipt).Count){throw 'Guided setup invented a recovery preference.'}
        $saved.owner_sid='S-1-5-18';[IO.File]::WriteAllText($receipt,($saved|ConvertTo-Json))
        $rejected=$false;try{Get-MoonSetupHandoverArguments -StatePath $receipt|Out-Null}catch{$rejected=$true}
        if(!$rejected){throw 'Guided retry trusted a preference from another account.'}
        [IO.File]::WriteAllText($receipt,'invalid JSON')
        $rejected=$false;try{Get-MoonSetupHandoverArguments -StatePath $receipt|Out-Null}catch{$rejected=$true}
        if(!$rejected){throw 'Guided retry ignored an unreadable recovery preference.'}
        if((Get-MoonSetupHandoverArguments -StatePath $receipt -KeepStopped) -ne '-KeepPreviousDesktopStopped'){throw 'Explicit no-fallback instruction was lost.'}
        $checks.Add('Guided retry preserves explicit or same-account no-fallback across sign-ins and rejects untrusted records')
        $payload=Join-Path $temporary 'payload';[void][IO.Directory]::CreateDirectory($payload)
        foreach($file in @('moon.exe','moon-settings.exe','install.ps1','uninstall.ps1','setup.ps1','handover.ps1','verify-uiaccess.ps1','window_inventory.exe','recover_windows.exe')){[IO.File]::WriteAllText((Join-Path $payload $file),'owned existence fixture; never executed')}
        Test-MoonSetupPayload $payload (Join-Path $payload 'moon.exe') (Join-Path $payload 'moon-settings.exe')
        & {
            $manifest=Join-Path $payload 'package-manifest.json'
            [IO.File]::WriteAllText($manifest,'{"target":"aarch64-pc-windows-msvc"}')
            function Get-MoonNativeWindowsTarget {'aarch64-pc-windows-msvc'}
            Test-MoonSetupPayload $payload (Join-Path $payload 'moon.exe') (Join-Path $payload 'moon-settings.exe')
            function Get-MoonNativeWindowsTarget {'x86_64-pc-windows-msvc'}
            $rejected=$false
            try {Test-MoonSetupPayload $payload (Join-Path $payload 'moon.exe') (Join-Path $payload 'moon-settings.exe')} catch [PlatformNotSupportedException] {$rejected=$true}
            if(!$rejected){throw 'Guided setup accepted Arm64 executables on an x64 PC.'}
            [IO.File]::Delete($manifest)
        }
        $checks.Add('Guided preflight accepts the Arm64 package on Arm64 and rejects it on x64 before handover')
        Remove-Item -LiteralPath (Join-Path $payload 'recover_windows.exe')
        $rejected=$false;try{Test-MoonSetupPayload $payload (Join-Path $payload 'moon.exe') (Join-Path $payload 'moon-settings.exe')}catch{$rejected=$true}
        if(!$rejected){throw 'Incomplete first-install payload passed guided preflight.'}
        $checks.Add('Guided first-install preflight requires both applications and handover recovery dependencies')
    }
    & {
        Add-Type -AssemblyName PresentationFramework
        $primary=$setup.Find({param($node)$node -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $node.Extent.Text.StartsWith('$ui.Primary.Add_Click(')},$true)
        $tick=$setup.Find({param($node)$node -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $node.Member.Value -eq 'Add_Tick'},$true)
        if(!$primary -or !$tick){throw 'Expected the production primary and completion handlers.'}
        $body=$primary.Arguments[0].ScriptBlock.Extent.Text
        $sourceDirectory=([IO.Path]::GetDirectoryName($setupPath)).Replace("'","''")
        $primaryHandler=[scriptblock]::Create($body.Substring(1,$body.Length-2).Replace('$PSScriptRoot',("'"+$sourceDirectory+"'")))
        $body=$tick.Arguments[0].ScriptBlock.Extent.Text;$tickHandler=[scriptblock]::Create($body.Substring(1,$body.Length-2))
        $ui=@{};foreach($name in @('Heading','Status','Progress','Primary','Secondary','Details','RuntimeNote')){$ui[$name]=[pscustomobject]@{Text='';Visibility='Visible';IsEnabled=$true;Content='';IsCancel=$false;IsDefault=$false;IsIndeterminate=$false}}
        $timer=[pscustomobject]@{started=$false};$timer|Add-Member ScriptMethod Start {$this.started=$true};$timer|Add-Member ScriptMethod Stop {$this.started=$false}
        $oldLog=Join-Path $temporary 'earlier-attempt.log';[IO.File]::WriteAllText($oldLog,'earlier diagnostics')
        $context=@{busy=$false;done=$false;process=$null;files=$null;operation='old';detail=$oldLog;status='failed';outcome=[pscustomobject]@{detail='old failure'}}
        $Mode='Install';$installed=$false;$Binary='owned-moon.exe';$SettingsBinary='owned-settings.exe';$PortableBinary='C:\Owned portable\moon.exe';$KeepPreviousDesktopStopped=$true
        $ui.Secondary.IsEnabled=$false
        function Test-MoonAdministrator {$false}
        function Test-MoonSetupPayload {param($PackageDirectory,$Daemon,$Settings,$Installed)}
        function Get-MoonSetupHandoverArguments {param([switch]$KeepStopped) if(!$KeepStopped){throw 'No-fallback option not forwarded'};'-KeepPreviousDesktopStopped'}
        function Show-MoonSetupProblem {param($Message,$Failure) $script:setupReportedProblem=$Message;$ui.Status.Text=$Message}
        function Start-Process {param($FilePath,$ArgumentList,$WindowStyle,$RedirectStandardOutput,$RedirectStandardError,[switch]$PassThru) $script:setupWorkerArguments=$ArgumentList;$child=[pscustomobject]@{HasExited=$false;ExitCode=0};$child|Add-Member ScriptMethod Dispose {};return $child}
        . $primaryHandler
        if (!(Test-Path -LiteralPath $context.workerMarker)) {throw 'Standalone setup did not retain the worker payload.'}
        if(!$context.busy -or $context.outcome -or $context.status -ne 'working' -or !$ui.Secondary.IsEnabled -or $ui.Details.Visibility -ne 'Collapsed' -or $script:setupWorkerArguments -notlike '*-KeepPreviousDesktopStopped*' -or $script:setupWorkerArguments -notlike ('*-PortableBinary '+(ConvertTo-MoonArgument $PortableBinary)+'*')){throw 'Actual retry did not reset stale UI state or forward no-fallback.'}
        if([IO.File]::ReadAllText($oldLog) -ne 'earlier diagnostics'){throw 'Retry deleted earlier diagnostics.'}
        $checks.Add('Actual guided retry resets stale controls/outcome, retains diagnostic history and forwards no-fallback plus the selected portable copy')
        function Read-MoonSetupStatus {param($OperationId) $script:setupFixtureStatus}
        $script:setupFixtureStatus=[pscustomobject]@{operation=$context.operation;status='cancelled';message='Moon setup was rolled back. Your previous desktop remains stopped.'}
        $context.process.HasExited=$true;$context.process.ExitCode=1223
        . $tickHandler
        if ((Test-Path -LiteralPath $context.workerMarker) -or $context.exitCode -ne 1223) {throw 'Standalone cancellation did not release the worker payload and preserve its exit code.'}
        if($context.busy -or $ui.Status.Text -ne $script:setupFixtureStatus.message -or $ui.Primary.Content -ne 'Try again'){throw 'Cancellation replaced the actual desktop recovery outcome.'}
        $checks.Add('Actual cancellation completion preserves the accurate previous-desktop-stopped outcome')
        $context.busy=$true;$context.detail=Join-Path $temporary 'missing-directory\failure.log'
        $context.process=[pscustomobject]@{HasExited=$true;ExitCode=1};$context.process|Add-Member ScriptMethod Dispose {}
        $script:setupFixtureStatus=[pscustomobject]@{operation=$context.operation;status='failed';message='Setup could not finish.';detail='Failure details'}
        . $tickHandler
        if ($context.exitCode -ne 1) {throw 'Standalone setup lost its failed worker exit code.'}
        if($context.busy -or !$ui.Primary.IsEnabled -or $ui.Details.Visibility -ne 'Visible' -or $ui.Status.Text -ne $script:setupFixtureStatus.message){throw 'Diagnostic append failure left setup apparently busy or prevented retry.'}
        $checks.Add('Actual failed completion remains actionable when diagnostics cannot be appended')
        $context.done=$true;$Mode='Install'
        $window=[pscustomobject]@{closed=$false};$window|Add-Member ScriptMethod Close {$this.closed=$true}
        function Get-MoonWebViewRuntime {'fixture-runtime'}
        function Get-MoonInstallDirectory {Join-Path $temporary 'owned-installed-fixture'}
        function Read-MoonInstallState {param($Directory) $script:setupInstalledFixture}
        foreach($configuration in @($null,(Join-Path $temporary 'custom settings.yaml'))) {
            $script:setupInstalledFixture=[pscustomobject]@{config_path=$configuration}
            $script:settingsArguments=$null;$script:settingsTarget=$null
            function Start-Process {param($FilePath,$ArgumentList) $script:settingsTarget=$FilePath;$script:settingsArguments=$ArgumentList}
            . $primaryHandler
            $expected=Get-MoonLaunchArguments $script:setupInstalledFixture -Settings
            if(!$window.closed -or $script:settingsTarget -ne (Join-Path (Get-MoonInstallDirectory) 'moon-settings.exe') -or [string]$script:settingsArguments -cne [string]$expected){throw 'Open Settings did not preserve the installation configuration path.'}
            $window.closed=$false
        }
        $checks.Add('Actual Open Settings carries the recorded config path and preserves legacy argument-free launch')
        function Start-Process {param($FilePath,$ArgumentList) throw [ComponentModel.Win32Exception]::new(2)}
        $script:setupReportedProblem=$null
        . $primaryHandler
        if(!$script:setupReportedProblem -or $window.closed -or !$context.done){throw 'A Settings launch failure escaped completion UI or restarted installation.'}
        $checks.Add('Actual Open Settings failure stays in the completed setup without closing or reinstalling')
        function Get-MoonWebViewRuntime {$null}
        function Start-Process {param($FilePath) $script:setupOpenedTarget=$FilePath}
        . $primaryHandler
        if($window.closed -or $script:setupOpenedTarget -ne 'https://developer.microsoft.com/microsoft-edge/webview2/consumer/' -or $ui.Status.Text -notlike '*return here*'){throw 'Missing WebView2 did not leave a clear route back to Settings.'}
        $checks.Add('Actual missing-runtime completion opens only the Microsoft page and keeps setup available')
        $Mode='Uninstall';$script:setupOpenedTarget=$null
        . $primaryHandler
        if(!$window.closed -or $script:setupOpenedTarget){throw 'Completed removal did not close without another launch.'}
        $checks.Add('Completed removal closes without requesting another approval or launching another process')
        $activation=$setup.Find({param($node)$node -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $node.Member.Value -eq 'Add_Activated'},$true)
        $body=$activation.Arguments[0].ScriptBlock.Extent.Text;$activationHandler=[scriptblock]::Create($body.Substring(1,$body.Length-2))
        function Get-MoonWebViewRuntime {'fixture-runtime'}
        $Mode='Install';$ui.RuntimeNote.Visibility='Collapsed';$ui.Status.Text='Settings could not open.'
        . $activationHandler
        if($ui.Status.Text -ne 'Settings could not open.'){throw 'Returning to setup erased an unresolved launch error.'}
        $ui.RuntimeNote.Visibility='Visible'
        . $activationHandler
        if($ui.Primary.Content -ne 'Open Settings' -or $ui.RuntimeNote.Visibility -ne 'Collapsed' -or $ui.Status.Text -notlike '*ready*'){throw 'Installing the missing runtime did not refresh the completed setup action.'}
        $checks.Add('Returning to setup refreshes a newly installed runtime without erasing unrelated launch errors')
    }
    & {
        $removed=$setup.Find({param($node)$node -is [Management.Automation.Language.StatementBlockAst] -and $node.Parent -is [Management.Automation.Language.IfStatementAst] -and $node.Extent.Text.Contains("'Moon is already removed.'")},$true)
        $loaded=$setup.Find({param($node)$node -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $node.Member.Value -eq 'Add_Loaded'},$true)
        if(!$removed -or !$loaded){throw 'Expected the actual already-removed state and initial focus handlers.'}
        $context=@{done=$false};$Mode='Uninstall';$ui=@{}
        foreach($name in @('Heading','Introduction','Promise','Primary','Secondary')) {
            $ui[$name]=[pscustomobject]@{Text='old text';Content='old action';Visibility='Visible';IsEnabled=$true;IsDefault=$false;IsCancel=$false;focused=$false}
            $ui[$name]|Add-Member ScriptMethod Focus {$this.focused=$true;return $true}
        }
        $body=$removed.Extent.Text;. ([scriptblock]::Create($body.Substring(1,$body.Length-2)))
        $body=$loaded.Arguments[0].ScriptBlock.Extent.Text;. ([scriptblock]::Create($body.Substring(1,$body.Length-2)))
        if (!(Test-Path -LiteralPath (Join-Path $standaloneRoot '.moon-setup-visible'))) {throw 'Standalone setup did not record that its window opened.'}
        if(!$context.done -or $ui.Promise.Text -ne '' -or $ui.Promise.Visibility -ne 'Collapsed' -or $ui.Secondary.Visibility -ne 'Collapsed' -or $ui.Primary.Content -ne 'Done' -or !$ui.Primary.IsDefault -or !$ui.Primary.IsCancel -or !$ui.Primary.focused -or $ui.Secondary.focused){throw 'Already-removed setup did not present a single focused Done action without a startup promise.'}
        $checks.Add('Actual already-removed state hides the startup promise and duplicate action, with Done focused for Enter and Escape')
    }
    $report=[pscustomobject]@{passed=$true;checks=$checks.ToArray();uac_requested=$false;installation_changed=$false}
    if($ReportPath){[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))}
    $report|ConvertTo-Json -Depth 5
}finally{Remove-Item -LiteralPath $temporary -Recurse -Force}
