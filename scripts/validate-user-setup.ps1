# Exercise the shipping installer on a clean Windows account. Silent setup skips
# launching the window manager. An existing Moon installation is never replaced.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Installer,
    [Parameter(Mandatory)][string]$Binary,
    [Parameter(Mandatory)][string]$SettingsBinary,
    [switch]$Signed
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'install-common.ps1')
$installer = (Get-Item -LiteralPath $Installer).FullName
$version = (Get-Item -LiteralPath $Binary).VersionInfo.ProductVersion
$manifest = [xml][MoonInstallNative]::Manifest($installer)
$level = $manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
if (!$level -or $level.level -ne 'asInvoker' -or $level.GetAttribute('uiAccess') -eq 'true' -or
    [Diagnostics.FileVersionInfo]::GetVersionInfo($installer).ProductVersion.TrimEnd() -cne $version) {
    throw 'Unexpected per-user installer permissions or release version.'
}
$sum = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
if ([IO.File]::ReadAllText($installer + '.sha256').TrimEnd() -cne ($sum + '  ' + [IO.Path]::GetFileName($installer))) { throw 'Incorrect setup checksum.' }
$appKey = 'HKCU:\Software\MoonWM\UserInstall'
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\MoonWM.User_is1'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$runName = 'Moon Window Manager'
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Moon.lnk'
function Get-SignInCommand {
    $key = Get-Item -LiteralPath $runKey -ErrorAction SilentlyContinue
    if ($key) { try { $key.GetValue($runName) } finally { $key.Dispose() } }
}
if ((Test-Path -LiteralPath $appKey) -or (Test-Path -LiteralPath $uninstallKey) -or (Test-Path -LiteralPath $shortcut) -or
    $null -ne (Get-SignInCommand) -or
    (Test-Path -LiteralPath (Join-Path (Get-MoonInstallDirectory) 'installation.json'))) { throw 'Installer smoke tests require a clean Windows account.' }
$work = [IO.Directory]::CreateDirectory((Join-Path ([IO.Path]::GetTempPath()) ('Moon Setup QA ' + [guid]::NewGuid().ToString('N'))))
$install = Join-Path $work.FullName ('Moon ' + [char]0x03A9)
$uninstaller = Join-Path $install 'unins000.exe'
function Read-UserPath {
    $environment = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
    if ($environment) {
        try { $environment.GetValue('Path',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
        finally { $environment.Dispose() }
    }
}
$originalPath = Read-UserPath
$arguments = @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-')
function Invoke-Setup([string]$Exe,[string[]]$Extra) {
    $line = (@($arguments + $Extra) | ForEach-Object { ConvertTo-MoonArgument $_ }) -join ' '
    $process = Start-Process -FilePath $Exe -ArgumentList $line -WindowStyle Hidden -Wait -PassThru
    try { if ($process.ExitCode -ne 0) { throw "$Exe exited with $($process.ExitCode)." } } finally { $process.Dispose() }
}
try {
    Invoke-Setup $installer @("/DIR=$install", "/LOG=$($work.FullName)\install.log")
    if ((Get-ItemPropertyValue -LiteralPath $appKey -Name Directory) -ne $install -or
        !(Test-Path -LiteralPath $uninstallKey) -or !(Test-Path -LiteralPath $shortcut) -or
        (Get-SignInCommand) -cne ('"' + $install + '\moon-settings.exe" --start-moon')) {
        throw 'Setup did not register Moon in the current account.'
    }
    $unexpected = @(Get-ChildItem -LiteralPath $install -File -Recurse | Where-Object Extension -in '.ps1','.cmd','.bat')
    if ($unexpected.Count -ne 0) { throw 'The per-user installer shipped shell scripts.' }
    foreach ($inputPath in @($Binary,$SettingsBinary)) {
        $path = Join-Path $install ([IO.Path]::GetFileName($inputPath))
        if (!$Signed -and (Get-FileHash -LiteralPath $path).Hash -cne (Get-FileHash -LiteralPath $inputPath).Hash) { throw "Wrong installed executable: $path" }
        $manifest = [xml][MoonInstallNative]::Manifest($path)
        $level = $manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
        if ($level.level -ne 'asInvoker' -or $level.uiAccess -ne 'false') { throw "Wrong installed permissions: $path" }
    }
    if ($Signed) {
        foreach ($path in @($installer,$uninstaller,(Join-Path $install 'moon.exe'),(Join-Path $install 'moon-settings.exe'))) {
            $signature = Get-AuthenticodeSignature -LiteralPath $path
            if ($signature.Status -ne 'Valid' -or !$signature.TimeStamperCertificate) { throw "Missing valid publisher signature or timestamp: $path" }
        }
    }
    if ((& (Join-Path $install 'moon.exe') --version) -cne "moon $version" -or $LASTEXITCODE) { throw 'The installed CLI did not report the release version.' }
    $other = Join-Path $install 'user-note.txt'
    [IO.File]::WriteAllText($other,'retain')
    Remove-ItemProperty -LiteralPath $runKey -Name $runName
    Invoke-Setup $installer @("/DIR=$install", "/LOG=$($work.FullName)\upgrade.log")
    if ($null -ne (Get-SignInCommand) -or
        @((Read-UserPath) -split ';' | Where-Object {$_ -eq $install}).Count -ne 1) { throw 'Upgrade reset sign-in or duplicated the user PATH entry.' }
    Invoke-Setup $uninstaller @("/LOG=$($work.FullName)\uninstall.log")
    if ((Test-Path -LiteralPath $appKey) -or (Test-Path -LiteralPath $uninstallKey) -or (Test-Path -LiteralPath $shortcut) -or
        (Test-Path -LiteralPath (Join-Path $install 'moon.exe')) -or (Test-Path -LiteralPath (Join-Path $install 'moon-settings.exe')) -or
        [string](Read-UserPath) -cne [string]$originalPath -or [IO.File]::ReadAllText($other) -cne 'retain') { throw 'Uninstall did not preserve the user files and PATH or remove the owned installation.' }
    Write-Output "Verified per-user install, upgrade, uninstall and CLI for Moon $version."
} finally {
    if (Test-Path -LiteralPath $uninstaller) { Invoke-Setup $uninstaller @() }
    $work.Delete($true)
}
