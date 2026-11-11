[CmdletBinding()]
param([string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon\uiaccess-results.json'))
. (Join-Path $PSScriptRoot 'install-common.ps1')
if ((Test-MoonAdministrator) -or [MoonInstallNative]::Integrity() -ne 8192 -or [MoonInstallNative]::TokenWord(26) -ne 0) {
    throw 'Run this verification from ordinary, medium-integrity PowerShell. It measures whether an unprivileged client can reach the installed daemon.'
}
$daemon=Invoke-MoonRpc query stats
if (!$daemon.token.ui_access) {throw 'The running daemon does not have UIAccess.'}
$target=Join-Path (Get-MoonInstallDirectory) 'moon.exe'
if (![string]::Equals($daemon.executable,$target,[StringComparison]::OrdinalIgnoreCase)) {throw 'Verification requires the signed Program Files installation.'}
$signature=Get-AuthenticodeSignature -LiteralPath $target
if ($signature.Status -ne 'Valid') {throw 'The installed executable signature is not valid.'}
$state=Read-MoonInstallState (Get-MoonInstallDirectory)
$task=Get-ScheduledTask -TaskName $state.task_name
Assert-MoonOwnedTask $task $state $target
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-uiaccess-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
$childOutput=Join-Path $temporary 'ordinary-child.json'
$helper=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'install-common.ps1')).ProviderPath
$childScript=@"
`$ErrorActionPreference='Stop'
. '$($helper.Replace("'","''"))'
`$result=[pscustomobject]@{process_id=`$PID;integrity_rid=[MoonInstallNative]::Integrity();ui_access=([MoonInstallNative]::TokenWord(26) -ne 0);elevated=([MoonInstallNative]::TokenWord(20) -ne 0)}
[IO.File]::WriteAllText('$($childOutput.Replace("'","''")).tmp',(`$result|ConvertTo-Json),[Text.UTF8Encoding]::new(`$false))
[IO.File]::Move('$($childOutput.Replace("'","''")).tmp','$($childOutput.Replace("'","''"))')
"@
$encoded=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($childScript))
$powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
try {
    Invoke-MoonRpc command -Commands @("exec `"$powershell`" -NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -EncodedCommand $encoded") | Out-Null
    Wait-MoonCondition -Description 'the ordinary program launched by moon' -Condition {Test-Path -LiteralPath $childOutput}
    $ordinary=Get-Content -Raw -LiteralPath $childOutput | ConvertFrom-Json
    if ($ordinary.integrity_rid -ne 8192 -or $ordinary.ui_access -or $ordinary.elevated) {throw 'A program launched by moon inherited elevated privileges or UIAccess.'}
    # Resolve identity and compile the helper before lowering integrity. After the
    # change, no writable path is required; the child prints its result to stdout.
    $lowScript=@"
`$ErrorActionPreference='Stop'
. '$($helper.Replace("'","''"))'
`$name=[MoonInstallNative]::PipeName()
[MoonInstallNative]::LowerToLowIntegrity()
`$pipe=[IO.Pipes.NamedPipeClientStream]::new('.',`$name,[IO.Pipes.PipeDirection]::InOut)
try {`$pipe.Connect(1500); Write-Error 'Low-integrity client unexpectedly connected'; exit 1}
catch [UnauthorizedAccessException] { @{low_integrity_refused=`$true;integrity_rid=[MoonInstallNative]::Integrity()}|ConvertTo-Json -Compress; exit 0 }
finally {`$pipe.Dispose()}
"@
    $lowEncoded=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($lowScript))
    $lowResult=Invoke-MoonNative -Path $powershell -Arguments @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-EncodedCommand',$lowEncoded)
    $low=$lowResult.Output | ConvertFrom-Json
    if (!$low.low_integrity_refused -or $low.integrity_rid -ne 4096) {throw 'Low-integrity client refusal was not proved.'}
    $report=[pscustomobject]@{recorded_at=[DateTime]::UtcNow.ToString('o');daemon=$daemon.token;installed_signature='Valid';startup_run_level=$task.Principal.RunLevel.ToString();medium_client_connected=$true;ordinary_child=$ordinary;low_client=$low}
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
    [IO.File]::WriteAllText($ReportPath,($report | ConvertTo-Json -Depth 12),[Text.UTF8Encoding]::new($false))
    $report | ConvertTo-Json -Depth 12
} finally {if (Test-Path -LiteralPath $temporary) {Remove-Item -LiteralPath $temporary -Recurse -Force}}
