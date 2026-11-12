# Harmless Windows desktop-shell regression. Never invokes installation or UAC.
[CmdletBinding()]
param([string]$ReportPath)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$Repository=Split-Path -Parent $PSScriptRoot
if (!$ReportPath) {$ReportPath=Join-Path $Repository 'target\explorer-launch-validation.json'}
$ReportPath=[IO.Path]::GetFullPath($ReportPath)
[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($ReportPath))
. (Join-Path $Repository 'scripts\install-common.ps1')
if (!('MoonLaunchPathCheck' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Text;
using System.Runtime.InteropServices;
public static class MoonLaunchPathCheck {
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]static extern uint GetFinalPathNameByHandle(IntPtr file,StringBuilder path,uint length,uint flags);
    public static string Read(string path){using(var file=new FileStream(path,FileMode.Open,FileAccess.Read,FileShare.ReadWrite|FileShare.Delete)){var result=new StringBuilder(32768);if(GetFinalPathNameByHandle(file.SafeFileHandle.DangerousGetHandle(),result,(uint)result.Capacity,0)==0)throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());return result.ToString();}}
}
'@
}
$source=Join-Path $Repository 'scripts\launch-from-explorer.ps1'
$tokens=$null;$errors=$null;$ast=[Management.Automation.Language.Parser]::ParseFile($source,[ref]$tokens,[ref]$errors)
if($errors.Count){throw ($errors|Out-String)}
Assert-MoonNoReparse ([Environment]::GetFolderPath('UserProfile'))
$fixture=Join-Path ([Environment]::GetFolderPath('UserProfile')) ('.moon-launch-test-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($fixture)
$checks=[Collections.Generic.List[object]]::new();$ownedDirectories=[Collections.Generic.List[string]]::new();$failure=$null;$cleanupErrors=[Collections.Generic.List[string]]::new()
$powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$runner=Join-Path $fixture 'runner.ps1';$dummy=Join-Path $fixture 'dummy.ps1';$utf8=[Text.UTF8Encoding]::new($false)
$runnerSource=@'
param([string]$Source,[string]$Request)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$scripts=Split-Path -Parent $Source
. (Join-Path $scripts 'install-common.ps1')
$tokens=$null;$errors=$null;$ast=[Management.Automation.Language.Parser]::ParseFile($Source,[ref]$tokens,[ref]$errors)
foreach($definition in $ast.FindAll({param($node)$node -is [Management.Automation.Language.FunctionDefinitionAst]},$false)) {
    $quoted=[string][char]39+$scripts.Replace([string][char]39,([string][char]39+[char]39))+[char]39
    . ([scriptblock]::Create($definition.Extent.Text.Replace('$PSScriptRoot',$quoted)))
}
$spec=[IO.File]::ReadAllText($Request)|ConvertFrom-Json
exit (Invoke-MoonExplorerScript $spec.script ([string[]]$spec.arguments))
'@
$dummySource=@'
param([int]$Code,[string]$First,[string]$Second,[string]$Third,[string]$Fourth,[string]$Fifth)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'install-common.ps1')
[IO.File]::AppendAllText((Join-Path $PSScriptRoot 'invocations.log'),([string]$Code+':'+$PID+[Environment]::NewLine))
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$identity=[MoonInstallNative]::ProcessIdentity([uint32]$PID)
[Console]::Out.Write((@{values=@($First,$Second,$Third,$Fourth,$Fifth);pid=$PID;birth=$identity.Birth;owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value;integrity=[MoonInstallNative]::Integrity();uiaccess=[MoonInstallNative]::TokenWord(26);elevated=[MoonInstallNative]::TokenWord(20);session=[Diagnostics.Process]::GetCurrentProcess().SessionId}|ConvertTo-Json -Compress))
[Console]::Error.Write('dummy stderr '+$Code)
exit $Code
'@
try {
[IO.File]::WriteAllText($runner,$runnerSource,$utf8);[IO.File]::WriteAllText($dummy,$dummySource,$utf8)
Copy-Item -LiteralPath (Join-Path $Repository 'scripts\install-common.ps1') -Destination (Join-Path $fixture 'install-common.ps1')
$values=@(('a '+[char]34+'quoted'+[char]34+' value'),'','C:\a b\','$() & % ^ ;',('line1'+[char]10+[char]0x96ea))
$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$session=[Diagnostics.Process]::GetCurrentProcess().SessionId
    $checks.Add(@{name='PowerShell 5.1 production parser';passed=$true})
    foreach($code in @(0,42,1223)) {
        $request=Join-Path $fixture ('case-'+$code+'.json')
        $arguments=@('-Code',[string]$code,'-First',$values[0],'-Second',$values[1],'-Third',$values[2],'-Fourth',$values[3],'-Fifth',$values[4])
        [IO.File]::WriteAllText($request,(@{script=$dummy;arguments=$arguments}|ConvertTo-Json -Depth 4),$utf8)
        $result=Invoke-MoonNative $powershell @('-NoProfile','-ExecutionPolicy','Bypass','-File',$runner,'-Source',$source,'-Request',$request) -TimeoutSeconds 55 -AllowFailure
        $match=[regex]::Match($result.Error,'Diagnostics: ([^\r\n]+)')
        if(!$match.Success){throw ('No diagnostic path: '+$result.Error)}
        $directory=$match.Groups[1].Value
        # A malformed result must never turn test cleanup into deletion of an
        # unrelated path. Retained requests must point at this exact dummy.
        if((Split-Path -Parent $directory) -ne [Environment]::GetFolderPath('UserProfile') -or (Split-Path -Leaf $directory) -notmatch '^\.moon-launch-[a-f0-9]{32}$'){throw 'Unexpected launch diagnostic directory.'}
        Assert-MoonNoReparse $directory
        if(Test-Path -LiteralPath $directory) {
            $ownedRequest=Get-Content -Raw -LiteralPath (Join-Path $directory 'request.json')|ConvertFrom-Json
            if($ownedRequest.script -cne $dummy -or (Split-Path -Leaf $directory) -cne ('.moon-launch-'+$ownedRequest.nonce)){throw 'Launch diagnostics do not belong to this fixture.'}
        }
        $ownedDirectories.Add($directory)
        if($result.ExitCode -ne $code){throw ('Wrong Windows exit code: '+$result.ExitCode+' expected '+$code+' '+$result.Error)}
        $output=$result.Output|ConvertFrom-Json
        if(($output.values|ConvertTo-Json -Compress) -cne ($values|ConvertTo-Json -Compress)){throw ('Arguments changed: '+$result.Output)}
        if($output.owner -ne $owner -or $output.session -ne $session -or $output.integrity -ne 8192 -or $output.uiaccess -ne 0 -or $output.elevated -ne 0){throw 'Dummy script did not run in the ordinary owner context.'}
        if(!$result.Error.Contains('dummy stderr '+$code)){throw 'stderr was not forwarded.'}
        if($directory.StartsWith($env:LOCALAPPDATA,[StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Parent $directory) -ne [Environment]::GetFolderPath('UserProfile')){throw 'Launch state was staged under AppData.'}
        $physical=$null
        if($code -eq 0){if(Test-Path -LiteralPath $directory){throw 'Successful launch files were not removed.'}}
        else {
            $physical=[MoonLaunchPathCheck]::Read((Join-Path $directory 'result.json'))
            if($physical -cne ('\\?\'+(Join-Path $directory 'result.json'))){throw ('Result was redirected: '+$physical)}
            $completion=Get-Content -Raw -LiteralPath (Join-Path $directory 'result.json')|ConvertFrom-Json
            if($completion.exit_code -ne $code){throw 'Retained result lost the exact native code.'}
            $ready=Get-Content -Raw -LiteralPath (Join-Path $directory 'ready.json')|ConvertFrom-Json
            if(Get-Process -Id ([int]$ready.process_id) -ErrorAction SilentlyContinue){throw 'A launch worker remains running.'}
        }
        if(Get-Process -Id ([int]$output.pid) -ErrorAction SilentlyContinue){throw 'A dummy script remains running.'}
        $checks.Add(@{name=('Explorer launch '+$code+' preserves arguments, streams, context and cleanup');passed=$true;exit_code=$result.ExitCode;directory=$directory;physical_result=$physical;token=$output})
    }
    foreach($mode in @('expired-no-go','foreign-owner')) {
        $nonce=[guid]::NewGuid().ToString('N');$directory=Join-Path ([Environment]::GetFolderPath('UserProfile')) ('.moon-launch-'+$nonce)
        [void][IO.Directory]::CreateDirectory($directory);$ownedDirectories.Add($directory)
        Copy-Item -LiteralPath $source -Destination (Join-Path $directory 'worker.ps1')
        Copy-Item -LiteralPath (Join-Path $Repository 'scripts\install-common.ps1') -Destination (Join-Path $directory 'install-common.ps1')
        $sid=if($mode -eq 'foreign-owner'){'S-1-5-18'}else{$owner}
        $deadline=if($mode -eq 'foreign-owner'){[DateTime]::UtcNow.AddSeconds(30)}else{[DateTime]::UtcNow.AddSeconds(-1)}
        $request=@{version=1;nonce=$nonce;owner_sid=$sid;session=[Diagnostics.Process]::GetCurrentProcess().SessionId;script=$dummy;arguments=@('-Code','0');start_deadline=$deadline.ToString('o')}
        [IO.File]::WriteAllText((Join-Path $directory 'request.json'),($request|ConvertTo-Json -Depth 4),$utf8)
        if($mode -eq 'foreign-owner') {
            # Isolate ownership: neither a missing authorization nor an expired
            # deadline may accidentally make this regression pass.
            $go=@{nonce=$nonce;approved_at=[DateTime]::UtcNow.ToString('o')}
            [IO.File]::WriteAllText((Join-Path $directory 'go.json'),($go|ConvertTo-Json),$utf8)
        }
        $invocations=(Get-FileHash -LiteralPath (Join-Path $fixture 'invocations.log') -Algorithm SHA256).Hash
        $result=Invoke-MoonNative $powershell @('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $directory 'worker.ps1'),'-WorkerDirectory',$directory) -TimeoutSeconds 15 -AllowFailure
        if($result.ExitCode -ne 1 -or (Test-Path -LiteralPath (Join-Path $directory 'result.json')) -or (Test-Path -LiteralPath (Join-Path $directory 'stdout.txt'))){throw ($mode+' incorrectly started an action.')}
        if($mode -eq 'foreign-owner' -and (Test-Path -LiteralPath (Join-Path $directory 'ready.json'))){throw 'Foreign-owner request passed the ownership guard.'}
        if((Get-FileHash -LiteralPath (Join-Path $fixture 'invocations.log') -Algorithm SHA256).Hash -ne $invocations){throw ($mode+' reached the dummy script.')}
        $checks.Add(@{name=$mode;passed=$true})
    }
} catch {$failure=$_} finally {
    foreach($directory in @($ownedDirectories)+@($fixture)) {
        try {
            if(!(Test-Path -LiteralPath $directory)){continue}
            Assert-MoonNoReparse $directory
            $entries=@(Get-ChildItem -LiteralPath $directory -Force)
            if(@($entries|Where-Object {$_.PSIsContainer -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)}).Count){throw 'Unexpected nested directory in the owned fixture; retained for inspection.'}
            $readyPath=Join-Path $directory 'ready.json'
            if(Test-Path -LiteralPath $readyPath) {
                $ready=Get-Content -Raw -LiteralPath $readyPath|ConvertFrom-Json
                $process=Get-Process -Id ([int]$ready.process_id) -ErrorAction SilentlyContinue
                if($process) {
                    try {if([MoonInstallNative]::ProcessIdentity([uint32]$process.Id).Birth -eq $ready.birth){throw 'An owned worker is still running; its files were retained.'}}
                    finally {$process.Dispose()}
                }
            }
            Remove-Item -LiteralPath $directory -Recurse -Force
            if(Test-Path -LiteralPath $directory){throw 'Owned test files remain after cleanup.'}
        } catch {$cleanupErrors.Add($directory+': '+$_.Exception.Message)}
    }
}
if(!$failure -and !$cleanupErrors.Count){$checks.Add(@{name='All owned worker processes exited and fixture files were removed';passed=$true})}
$report=@{passed=(!$failure -and !$cleanupErrors.Count);observed_at=[DateTime]::UtcNow.ToString('o');checks=$checks;fixture=$fixture;source_sha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash;test_sha256=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash;no_installation_or_uac=$true;cleanup_errors=@($cleanupErrors);error=$(if($failure){$failure|Out-String}else{$null})}
[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 10),$utf8)
if($failure){throw $failure}
if($cleanupErrors.Count){throw ($cleanupErrors -join [Environment]::NewLine)}
$report|ConvertTo-Json -Depth 10
