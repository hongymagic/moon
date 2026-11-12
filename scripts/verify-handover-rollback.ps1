[CmdletBinding()]
param(
    [string]$StatePath=(Join-Path $env:LOCALAPPDATA 'moon\handover.json'),
    [string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\handover-rollback-results.json')
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
$saved=Get-Content -Raw -LiteralPath $StatePath | ConvertFrom-Json
if ($saved.status -ne 'rolled_back') {throw 'The handover record does not report a completed rollback.'}
$links=@($saved.startup_links | ForEach-Object {
    $actual=(Get-FileHash -LiteralPath $_.original -Algorithm SHA256).Hash
    [pscustomobject]@{path=$_.original;matches_backup=($actual -eq $_.sha256);sha256=$actual}
})
$configs=@($saved.config_backups | ForEach-Object {
    $actual=(Get-FileHash -LiteralPath $_.original -Algorithm SHA256).Hash
    [pscustomobject]@{path=$_.original;matches_backup=($actual -eq $_.sha256);sha256=$actual}
})
$expected=@(@($saved.processes | ForEach-Object {$_.name})+@($saved.startup_links | ForEach-Object {$_.program}) | Select-Object -Unique)
$processes=@(Get-CimInstance Win32_Process | Where-Object {$_.SessionId -eq $saved.session -and $_.Name -in @('glazewm.exe','zebar.exe','tacky-borders.exe')} | Select-Object Name,ProcessId,ExecutablePath)
$missing=@($expected | Where-Object {$processes.Name -notcontains $_})
$certificates=@(foreach($store in @('LocalMachine\My','LocalMachine\Root','LocalMachine\TrustedPublisher')) {
    Get-ChildItem -LiteralPath ('Cert:\'+$store) | Where-Object {$_.Subject -like 'CN=moon code signing *'} | ForEach-Object {[pscustomobject]@{store=$store;thumbprint=$_.Thumbprint;subject=$_.Subject}}
})
$tasks=@(Get-ScheduledTask | Where-Object {$_.TaskName -like 'moon-*'})
$installationExists=Test-Path -LiteralPath (Get-MoonInstallDirectory)
$environment=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
try {$path=if($environment){$environment.GetValue('Path','',[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)}else{''}} finally {if($environment){$environment.Dispose()}}
$pathContainsMoon=$false
if ($path) {$pathContainsMoon=@($path -split ';' | Where-Object {[string]::Equals($_.Trim('"').TrimEnd('\'),(Get-MoonInstallDirectory),[StringComparison]::OrdinalIgnoreCase)}).Count -gt 0}
$moonProcesses=@(Get-Process -Name moon -ErrorAction SilentlyContinue)
$passed=@($links | Where-Object {!$_.matches_backup}).Count -eq 0 -and @($configs | Where-Object {!$_.matches_backup}).Count -eq 0 -and $missing.Count -eq 0 -and $certificates.Count -eq 0 -and $tasks.Count -eq 0 -and !$installationExists -and !$pathContainsMoon -and $moonProcesses.Count -eq 0
$report=[pscustomobject]@{recorded_at=[DateTime]::UtcNow.ToString('o');passed=$passed;handover_state=$StatePath;handover_status=$saved.status;rollback_finished_at=$saved.finished_at;startup_links=$links;config_files=$configs;restored_processes=$processes;missing_previous_process_names=$missing;moon_certificate_entries=$certificates;moon_task_count=$tasks.Count;moon_install_directory_exists=$installationExists;moon_user_path_entry_exists=[bool]$pathContainsMoon;moon_process_count=$moonProcesses.Count;trust_evidence='No moon signing certificate or trust entries exist in LocalMachine My, Root or TrustedPublisher.'}
[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 10),[Text.UTF8Encoding]::new($false))
$report | ConvertTo-Json -Depth 10
if (!$passed) {throw 'Handover rollback validation found a mismatch.'}
