# Exercise the shipping verifier, then test installation dispatch with a harmless
# setup.ps1 in a second EXE. No test clicks setup or installs Moon on this desktop.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$InstallerPath,
    [Parameter(Mandatory)][string]$ArchivePath,
    [Parameter(Mandatory)][string]$Version,
    [string]$Target='x86_64-pc-windows-msvc',
    [string]$ReportPath
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'install-common.ps1')
. (Join-Path $PSScriptRoot 'package-target.ps1')
$platform=Get-MoonPackageTarget $Target
Add-Type -AssemblyName System.IO.Compression,System.IO.Compression.FileSystem

$installer = (Get-Item -LiteralPath $InstallerPath).FullName
$archive = (Get-Item -LiteralPath $ArchivePath).FullName
$utf8 = [Text.UTF8Encoding]::new($false)
$checks = [Collections.Generic.List[string]]::new()
$work = Join-Path ([IO.Path]::GetTempPath()) ('Moon setup QA ' + [char]0x03a9 + ' & ' + [guid]::NewGuid().ToString('N'))

function Start-Probe([string]$Executable, [string]$Arguments, [hashtable]$ChildEnvironment) {
    $start = [Diagnostics.ProcessStartInfo]::new($Executable, $Arguments)
    $start.UseShellExecute = $false
    $start.WorkingDirectory = $work
    foreach ($key in $ChildEnvironment.Keys) { $start.EnvironmentVariables[$key] = $ChildEnvironment[$key] }
    [Diagnostics.Process]::Start($start)
}

function Get-ProbeExitCode([Diagnostics.Process]$Process) {
    try {
        if (!$Process.WaitForExit(60000)) {
            $Process.Kill()
            $Process.WaitForExit()
            throw 'Setup probe did not exit within 60 seconds.'
        }
        $Process.ExitCode
    } finally { $Process.Dispose() }
}

try {
    [void][IO.Directory]::CreateDirectory($work)
    $info = [Diagnostics.FileVersionInfo]::GetVersionInfo($installer)
    if ($info.ProductName -cne 'Moon' -or $info.FileDescription -cne 'Moon Setup' -or $info.ProductVersion -cne $Version) {
        throw "Unexpected setup product, description or version: $($info.ProductName), $($info.FileDescription), $($info.ProductVersion)"
    }
    [xml]$manifest = [MoonInstallNative]::Manifest($installer)
    $levels = $manifest.SelectNodes("//*[local-name()='requestedExecutionLevel']")
    if ($levels.Count -ne 1 -or $levels[0].level -ne 'asInvoker' -or $levels[0].uiAccess -ne 'false') {
        throw 'The installer must open as the current user, without UIAccess.'
    }
    $identity = $manifest.SelectSingleNode("//*[local-name()='assemblyIdentity']")
    if (!$identity -or $identity.processorArchitecture -cne $platform.setupManifest) {
        throw 'The setup executable has the wrong manifest architecture.'
    }
    $assembly = [Reflection.Assembly]::ReflectionOnlyLoad([IO.File]::ReadAllBytes($installer))
    if ($assembly.GetName().ProcessorArchitecture -ne $platform.setupAssembly) { throw "Expected a $($platform.setupPlatform) setup executable." }
    $stream = $assembly.GetManifestResourceStream('Moon.Package.zip')
    if (!$stream) { throw 'Setup does not contain Moon.Package.zip.' }
    try { $embeddedHash = (Get-FileHash -InputStream $stream -Algorithm SHA256).Hash } finally { $stream.Dispose() }
    if ($embeddedHash -ne (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash) {
        throw 'The setup payload differs from the ZIP supplied for release.'
    }
    $setupHash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    if ([IO.File]::ReadAllText($installer + '.sha256').TrimEnd() -cne ($setupHash + '  ' + [IO.Path]::GetFileName($installer))) {
        throw 'The setup checksum does not match its executable.'
    }
    $checks.Add('Release identity, launcher architecture, original-user manifest, EXE checksum and embedded ZIP match')

    # Rebuild the same launcher around a private script. It waits for permission
    # from this test to finish, so a fire-and-forget or early cleanup is visible.
    $stage = Join-Path $work 'fixture'
    $docs = Join-Path $stage 'docs'
    [void][IO.Directory]::CreateDirectory($docs)
    [IO.File]::WriteAllText((Join-Path $docs 'nested note.txt'), 'Moon setup fixture', $utf8)
    $probe = @'
param()
$ErrorActionPreference = 'Stop'
if ($env:MOON_SETUP_EXE_DIRECTORY -ine $PSScriptRoot -or $args.Count) { throw 'Setup must identify its workspace in the child environment.' }
[IO.File]::WriteAllText((Join-Path $PSScriptRoot '.moon-setup-visible'), '')
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
try {
    $receipt = @{
        directory = $PSScriptRoot
        working_directory = [IO.Directory]::GetCurrentDirectory()
        sid = $identity.User.Value
        apartment = [Threading.Thread]::CurrentThread.GetApartmentState().ToString()
        is_64_bit = [Environment]::Is64BitProcess
    } | ConvertTo-Json
} finally { $identity.Dispose() }
if ($env:MOON_FIXTURE_WORKER_MARKER) {
    [IO.File]::WriteAllText((Join-Path $PSScriptRoot '.moon-setup-worker-active'), $env:MOON_FIXTURE_WORKER_MARKER)
}
[IO.File]::WriteAllText($env:MOON_FIXTURE_RECEIPT + '.writing', $receipt)
[IO.File]::Move($env:MOON_FIXTURE_RECEIPT + '.writing', $env:MOON_FIXTURE_RECEIPT)
$timer = [Diagnostics.Stopwatch]::StartNew()
while (!(Test-Path -LiteralPath $env:MOON_FIXTURE_GATE) -and $timer.Elapsed.TotalSeconds -lt 20) { Start-Sleep -Milliseconds 50 }
if (!(Test-Path -LiteralPath $env:MOON_FIXTURE_GATE)) { exit 97 }
if ([IO.File]::ReadAllText((Join-Path $PSScriptRoot 'docs\nested note.txt')) -ne 'Moon setup fixture') { exit 98 }
exit 42
'@
    [IO.File]::WriteAllText((Join-Path $stage 'setup.ps1'), $probe, $utf8)
    $files = foreach ($name in 'setup.ps1','docs/nested note.txt') {
        $file = Get-Item -LiteralPath (Join-Path $stage $name)
        [ordered]@{path=$name; bytes=$file.Length; sha256=(Get-FileHash -LiteralPath $file.FullName).Hash.ToLowerInvariant()}
    }
    $metadata = [ordered]@{schema_version=1; product='Moon'; target=$Target; kind='release'; version=$Version; files=@($files)}
    [IO.File]::WriteAllText((Join-Path $stage 'package-manifest.json'), ($metadata | ConvertTo-Json -Depth 6), $utf8)
    $baseName = "moon-$Version-windows-$($platform.arch)"
    $fixtureZip = Join-Path $work ($baseName + '.zip')
    $zip = [IO.Compression.ZipFile]::Open($fixtureZip, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($name in @($files.path) + 'package-manifest.json') {
            [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, (Join-Path $stage $name), $name)
        }
    } finally { $zip.Dispose() }
    $out = Join-Path $work 'output'
    & (Join-Path $PSScriptRoot 'build-setup-exe.ps1') -ArchivePath $fixtureZip -OutputDirectory $out -Version $Version -Target $Target | Out-Null
    $fixture = Join-Path $out ($baseName + '-uiaccess.exe')

    $temporary = Join-Path $work 'temporary files'
    [void][IO.Directory]::CreateDirectory($temporary)
    $receiptPath = Join-Path $work 'child.json'
    $gate = Join-Path $work 'continue'
    $environment = @{TEMP=$temporary; TMP=$temporary; MOON_FIXTURE_RECEIPT=$receiptPath; MOON_FIXTURE_GATE=$gate}
    if ((Get-ProbeExitCode (Start-Probe $fixture '/VERIFYONLY' $environment)) -ne 0 -or (Test-Path -LiteralPath $receiptPath)) {
        throw 'Verification must succeed without running the setup script.'
    }
    $process = Start-Probe $fixture '' $environment
    try {
        $timer = [Diagnostics.Stopwatch]::StartNew()
        while (!(Test-Path -LiteralPath $receiptPath) -and !$process.HasExited -and $timer.Elapsed.TotalSeconds -lt 60) {
            Start-Sleep -Milliseconds 50
        }
        if ($process.HasExited -or !(Test-Path -LiteralPath $receiptPath)) { throw 'Setup exited or failed to start the fixture.' }
        $expectedMachine = if ($platform.setupPlatform -eq 'anycpu') { [MoonInstallNative]::NativeMachine() } else { $platform.machine }
        if ([MoonInstallNative]::ProcessMachine($process.Handle) -ne $expectedMachine) {
            throw 'The setup launcher ran under the wrong processor architecture.'
        }
        $receipt = [IO.File]::ReadAllText($receiptPath) | ConvertFrom-Json
        $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
        try { $sid = $identity.User.Value } finally { $identity.Dispose() }
        if ($receipt.sid -ne $sid -or $receipt.apartment -ne 'STA' -or !$receipt.is_64_bit) {
            throw 'The guided setup child did not run under the original account in 64-bit STA mode.'
        }
        if (![IO.Directory]::Exists($receipt.directory)) { throw 'Setup removed the payload while its child was running.' }
        $profilePath = [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)
        if ([IO.Path]::GetDirectoryName($receipt.directory) -ine $profilePath -or $receipt.working_directory -ine $profilePath) {
            throw 'Setup must stage the payload below the user profile and launch its child from the profile itself.'
        }
    } finally {
        [IO.File]::WriteAllText($gate, 'finish')
        $childExit = Get-ProbeExitCode $process
    }
    if ($childExit -ne 42 -or [IO.Directory]::Exists($receipt.directory)) {
        throw "Setup must preserve the child exit code and remove its extracted payload (exit $childExit)."
    }
    $checks.Add('Normal launch keeps the fixture available until the original-user STA child exits, returns its code and removes the payload')

    # An independent install worker can outlive the window after a crash. A
    # token in its marker identifies the fixture directory this test may remove.
    $workerToken = [guid]::NewGuid().ToString('N')
    $environment.MOON_FIXTURE_WORKER_MARKER = $workerToken
    [IO.File]::Delete($receiptPath)
    if ((Get-ProbeExitCode (Start-Probe $fixture '' $environment)) -ne 42) { throw 'Unexpected exit for the active worker fixture.' }
    $retained = ([IO.File]::ReadAllText($receiptPath) | ConvertFrom-Json).directory
    $marker = Join-Path $retained '.moon-setup-worker-active'
    if ([IO.Path]::GetDirectoryName($retained) -ine $profilePath -or
        [IO.Path]::GetFileName($retained) -notmatch '^Moon-Setup-[0-9a-f]{32}$' -or
        ![IO.File]::Exists($marker) -or [IO.File]::ReadAllText($marker) -cne $workerToken) {
        throw 'Setup did not retain the extraction directory for its active worker.'
    }
    [IO.Directory]::Delete($retained, $true)
    $checks.Add('An active install worker retains its payload after the guided window exits')

    # Flip one byte in the private EXE's resource. The launcher remains valid.
    $singleByte = [Text.Encoding]::GetEncoding(28591)
    $bytes = [IO.File]::ReadAllBytes($fixture)
    $embedded = [IO.File]::ReadAllBytes($fixtureZip)
    $position = $singleByte.GetString($bytes).IndexOf($singleByte.GetString($embedded), [StringComparison]::Ordinal)
    if ($position -lt 0) { throw 'Could not locate the fixture ZIP for the integrity check.' }
    $bytes[$position + 10] = $bytes[$position + 10] -bxor 1
    $corrupt = Join-Path $work 'damaged setup.exe'
    [IO.File]::WriteAllBytes($corrupt, $bytes)
    if ((Get-ProbeExitCode (Start-Probe $corrupt '/VERIFYONLY' $environment)) -eq 0) { throw 'Setup accepted a damaged embedded ZIP.' }
    if ((Get-ProbeExitCode (Start-Probe $installer '/VERIFYONLY' $environment)) -ne 0) { throw 'The release installer failed native verification.' }
    $checks.Add('Native verification accepts the release installer and rejects altered fixture bytes')

    $result = [ordered]@{passed=$true; version=$Version; target=$Target; exe_sha256=$setupHash; checks=$checks.ToArray(); installation_performed=$false}
    $json = $result | ConvertTo-Json
    if ($ReportPath) { [IO.File]::WriteAllText([IO.Path]::GetFullPath($ReportPath), $json, $utf8) }
    $json
} finally {
    if ([IO.Directory]::Exists($work)) { Remove-Item -LiteralPath $work -Recurse -Force }
}
