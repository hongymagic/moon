[CmdletBinding()]
param([string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon\cli-launch-results.json'))
. (Join-Path $PSScriptRoot 'install-common.ps1')
if ((Test-MoonAdministrator) -or [MoonInstallNative]::Integrity() -ne 8192 -or [MoonInstallNative]::TokenWord(26) -ne 0) {throw 'Run CLI launch verification from an ordinary medium-integrity process.'}
$target=Join-Path (Get-MoonInstallDirectory) 'moon.exe'
$signature=Get-AuthenticodeSignature -LiteralPath $target
if ($signature.Status -ne 'Valid') {throw 'CLI launch verification requires the signed installed executable.'}
$checks=[Collections.Generic.List[object]]::new()
$direct=$false
try {
    $version=Invoke-MoonNative $target @('version')
    if ($version.Output.Trim() -notmatch '^moon \d+\.\d+\.\d+') {throw 'Unexpected version stdout.'}
    $direct=$true
    $checks.Add([pscustomobject]@{check='CreateProcess from medium, redirected stdout and exit status';passed=$true;output=$version.Output.Trim()})
} catch {
    $launchError=$_.Exception.GetBaseException()
    $checks.Add([pscustomobject]@{check='CreateProcess from medium, redirected stdout and exit status';passed=$false;native_error=if($launchError -is [ComponentModel.Win32Exception]){$launchError.NativeErrorCode}else{$null};error=$launchError.Message})
}
if ($direct) {
    try {
        $stats=(Invoke-MoonNative $target @('query','stats')).Output | ConvertFrom-Json
        if (!$stats.token.ui_access -or ![string]::Equals($stats.executable,$target,[StringComparison]::OrdinalIgnoreCase)) {throw 'CLI did not reach the installed UIAccess daemon.'}
        $checks.Add([pscustomobject]@{check='ordinary CLI redirects valid query JSON';passed=$true;daemon_id=$stats.process_id})
        $start=[Diagnostics.ProcessStartInfo]::new(); $start.FileName=$target; $start.Arguments='subscribe'; $start.UseShellExecute=$false; $start.CreateNoWindow=$true; $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
        $subscription=[Diagnostics.Process]::new(); $subscription.StartInfo=$start
        $subscriptionStarted=$false
        try {
            [void]$subscription.Start()
            $subscriptionStarted=$true
            $line=$subscription.StandardOutput.ReadLineAsync()
            if (!$line.Wait(5000)) {throw 'No redirected subscription acknowledgement arrived within five seconds.'}
            $reply=$line.Result | ConvertFrom-Json
            if (!$reply.ok -or $reply.version -ne 1) {throw 'Subscription acknowledgement is invalid.'}
            $checks.Add([pscustomobject]@{check='ordinary CLI streams subscription NDJSON';passed=$true;version=$reply.version})
        } finally {if($subscriptionStarted -and !$subscription.HasExited){$subscription.Kill();$subscription.WaitForExit()};$subscription.Dispose()}
    } catch {$checks.Add([pscustomobject]@{check='ordinary redirected CLI query and subscription';passed=$false;error=$_.Exception.GetBaseException().Message})}
}
$report=[pscustomobject]@{recorded_at=[DateTime]::UtcNow.ToString('o');caller_integrity_rid=[MoonInstallNative]::Integrity();caller_uiaccess=$false;path=$target;signature='Valid';direct_launch=$direct;passed=(@($checks | Where-Object {!$_.passed}).Count -eq 0);checks=$checks.ToArray();desktop_changed=$false}
[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
[IO.File]::WriteAllText($ReportPath,($report | ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
$report | ConvertTo-Json -Depth 8
