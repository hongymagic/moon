[CmdletBinding()]
param()
. (Join-Path $PSScriptRoot 'install-common.ps1')
$checks=[Collections.Generic.List[string]]::new()
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-handover-check-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
function Assert-Rejected([scriptblock]$Action,[string]$Description) {
    $rejected=$false
    try {& $Action | Out-Null} catch {$rejected=$true}
    if (!$rejected) {throw $Description}
}
function New-TestShortcut([string]$File,[string]$Target,[string]$Arguments,[string]$WorkingDirectory) {
    $shell=New-Object -ComObject WScript.Shell
    try {
        $shortcut=$shell.CreateShortcut($File)
        try {$shortcut.TargetPath=$Target;$shortcut.Arguments=$Arguments;$shortcut.WorkingDirectory=$WorkingDirectory;$shortcut.Save()}
        finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shortcut)}
    } finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)}
}
try {
    foreach ($file in @('install-common.ps1','handover.ps1','validate-handover.ps1')) {
        $tokens=$null;$errors=$null
        [void][Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $file),[ref]$tokens,[ref]$errors)
        if ($errors.Count) {throw ($errors | Out-String)}
    }
    $checks.Add('PowerShell 5.1 syntax')
    $identity=[MoonInstallNative]::ProcessIdentity([uint32]$PID)
    $current=[Diagnostics.Process]::GetCurrentProcess()
    try {
        if ($identity.Birth -ne $current.StartTime.ToUniversalTime().ToFileTimeUtc() -or ![string]::Equals($identity.Path,$current.MainModule.FileName,[StringComparison]::OrdinalIgnoreCase)) {throw 'Native process identity disagrees with the current process.'}
    } finally {$current.Dispose()}
    $checks.Add('Native limited-query process path and creation time')
    Assert-Rejected {[MoonInstallNative]::ProcessIdentity(0)} 'An unqueryable process identity was accepted.'
    $checks.Add('Unqueryable process identity fails closed')
    $program=[IO.Path]::GetFileName($identity.Path)
    $process=[pscustomobject]@{Name=$program;ProcessId=$PID;ExecutablePath=$null;CommandLine=$null}
    $shortcutFile=Join-Path $temporary 'matching.lnk'
    $arguments='--config "C:\configuration with spaces\settings.yaml" --literal "two words"'
    New-TestShortcut $shortcutFile $identity.Path $arguments $temporary
    $links=@(Get-MoonStartupLinks -Programs @($program) -Directory $temporary)
    $launch=Get-MoonProcessLaunch $process $links
    if ($launch.path -cne $identity.Path -or $launch.process_birth -ne $identity.Birth -or $launch.arguments -cne $arguments -or $launch.working_directory -cne $temporary -or $launch.launch_source -ne 'startup_shortcut') {throw 'Exact matching shortcut launch information was not preserved.'}
    Assert-MoonProcessIdentity $launch
    $checks.Add('Null WMI path and command line use one exact matched shortcut with unchanged arguments and working directory')
    $process.ExecutablePath=$identity.Path
    $process.CommandLine='"'+$identity.Path+'" --from-process "original arg"'
    $direct=Get-MoonProcessLaunch $process @()
    if ($direct.arguments -cne '--from-process "original arg"' -or $direct.launch_source -ne 'process_command_line') {throw 'Available process arguments were not preserved.'}
    $checks.Add('Available process command line remains authoritative')
    $process.ExecutablePath=Join-Path $temporary $program
    Assert-Rejected {Get-MoonProcessLaunch $process $links} 'A conflicting WMI executable path was accepted.'
    $process.ExecutablePath=$null;$process.CommandLine=$null
    $checks.Add('Conflicting native and WMI executable identities are rejected')
    Assert-Rejected {Get-MoonProcessLaunch $process @()} 'A process without launch provenance was accepted.'
    $checks.Add('Missing launch provenance fails before desktop changes')
    $foreignFile=Join-Path $temporary 'foreign.lnk'
    New-TestShortcut $foreignFile (Join-Path $temporary $program) '--foreign' $temporary
    $foreign=@(Get-MoonStartupLinks -Programs @($program) -Directory $temporary | Where-Object original -eq $foreignFile)
    Assert-Rejected {Get-MoonProcessLaunch $process $foreign} 'A same-name executable at a different path was accepted.'
    $checks.Add('A shortcut with the same executable name at a different path is rejected')
    $duplicateFile=Join-Path $temporary 'duplicate.lnk'
    New-TestShortcut $duplicateFile $identity.Path '--other-arguments' $temporary
    $ambiguous=@(Get-MoonStartupLinks -Programs @($program) -Directory $temporary)
    Assert-Rejected {Get-MoonProcessLaunch $process $ambiguous} 'Ambiguous exact-path shortcuts were accepted.'
    $checks.Add('Multiple exact-path shortcuts fail closed')
    $bytes=[IO.File]::ReadAllBytes($shortcutFile)
    [IO.File]::WriteAllBytes($shortcutFile,[byte[]]($bytes+@([byte]0)))
    Assert-Rejected {Get-MoonProcessLaunch $process $links} 'A changed shortcut was accepted after preflight.'
    $checks.Add('A shortcut changed after capture is rejected')
    $launch.process_birth++
    Assert-Rejected {Assert-MoonProcessIdentity $launch} 'A stale process creation time was accepted.'
    $checks.Add('Process creation time is rechecked before shutdown')
    [pscustomobject]@{passed=$checks.Count;checks=$checks.ToArray();startup_changed=$false;applications_started=$false;uac_requested=$false} | ConvertTo-Json -Depth 5
} finally {
    if (Test-Path -LiteralPath $temporary) {Remove-Item -LiteralPath $temporary -Recurse -Force}
}
