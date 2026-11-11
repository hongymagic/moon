[CmdletBinding()]
param(
    [string]$Directory = (Join-Path $env:LOCALAPPDATA 'moon\soak'),
    [string]$Incident
)
. (Join-Path $PSScriptRoot 'install-common.ps1')

# Each invocation appends one observation. Elapsed calendar time alone does not
# certify the seven-day acceptance condition or conceal gaps between samples.
[void][IO.Directory]::CreateDirectory($Directory)
$session = [Diagnostics.Process]::GetCurrentProcess().SessionId
$record = [ordered]@{
    version = 1
    observed_at = [DateTime]::UtcNow.ToString('o')
    session_id = $session
    incident = $Incident
    daemon = $null
    executable_file_sha256 = $null
    process_started_at = $null
    competing_processes = @()
    error = $null
}
try {
    $record.daemon = Invoke-MoonRpc query stats
    $process = [Diagnostics.Process]::GetProcessById([int]$record.daemon.process_id)
    try {
        if ($process.SessionId -ne $session) {throw 'The daemon belongs to another session.'}
        $record.process_started_at = $process.StartTime.ToUniversalTime().ToString('o')
    } finally {$process.Dispose()}
    $record.executable_file_sha256 = (Get-FileHash -LiteralPath $record.daemon.executable -Algorithm SHA256).Hash
    $record.competing_processes = @(Get-CimInstance Win32_Process | Where-Object {
        $_.SessionId -eq $session -and $_.Name -in @('glazewm.exe','komorebi.exe','whim.exe','zebar.exe','tacky-borders.exe')
    } | Select-Object ProcessId,Name,CreationDate)
    if ($record.competing_processes.Count -ne 0) {throw 'A component of another desktop stack is running.'}
    if (!$record.daemon.token.ui_access) {throw 'This is not the installed UIAccess daily-driver build.'}
    if ($record.daemon.last_error) {throw ('The daemon reports: '+$record.daemon.last_error)}
} catch {$record.error = $_.Exception.Message}

$path = Join-Path $Directory ('session-'+$session+'.jsonl')
$encoding = [Text.UTF8Encoding]::new($false)
$stream = [IO.FileStream]::new($path,[IO.FileMode]::Append,[IO.FileAccess]::Write,[IO.FileShare]::Read)
try {
    $bytes = $encoding.GetBytes(($record | ConvertTo-Json -Depth 50 -Compress)+"`n")
    $stream.Write($bytes,0,$bytes.Length)
    $stream.Flush($true)
} finally {$stream.Dispose()}
[pscustomobject]@{record=$path;observed_at=$record.observed_at;healthy=(!$record.error);error=$record.error} | ConvertTo-Json
if ($record.error) {exit 1}
