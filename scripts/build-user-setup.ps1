# Package the ordinary-permission applications. PowerShell runs only on the build host.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Compiler,
    [Parameter(Mandatory)][string]$Binary,
    [Parameter(Mandatory)][string]$SettingsBinary,
    [Parameter(Mandatory)][string]$RustLibraryNotices,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][ValidatePattern('\A(?:[0-9]+\.[0-9]+\.[0-9]+|[0-9]{4}\.[0-9]{2}\.[0-9]{2}-(?:0|[1-9][0-9]*))\z')][string]$Version,
    [Parameter(Mandatory)][string]$Target,
    [switch]$Sign
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'install-common.ps1')
. (Join-Path $PSScriptRoot 'package-target.ps1')
. (Join-Path $PSScriptRoot 'package-image.ps1')
$platform = Get-MoonPackageTarget $Target
$Development = $false
$root = Split-Path -Parent $PSScriptRoot
$compiler = (Get-Item -LiteralPath $Compiler).FullName
$out = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
$name = "moon-$Version-windows-$($platform.arch)-setup"
$destination = Join-Path $out ($name + '.exe')
$checksum = $destination + '.sha256'
if ([IO.File]::Exists($destination) -or [IO.File]::Exists($checksum)) { throw "Setup output already exists in $out." }
[void][IO.Directory]::CreateDirectory($out)
$work = [IO.Directory]::CreateDirectory((Join-Path $out ('.moon-user-setup-' + [guid]::NewGuid().ToString('N'))))
try {
    $payload = Join-Path $work.FullName 'payload'
    foreach ($app in @(@($Binary,'Moon','moon.exe'), @($SettingsBinary,'Moon Settings','moon-settings.exe'))) {
        $path = Join-Path $payload $app[2]
        Copy-PackageFile (Get-Item -LiteralPath $app[0]).FullName $path
        $imports = @(Assert-PackagePe $path)
        if ($imports -match '^(vcruntime|msvcp|msvcr|concrt|vccorlib)[0-9].*\.dll$|^ucrtbased\.dll$') { throw "Build $($app[2]) with the static C runtime." }
        [void](Read-PackageIdentity $path $app[1] $app[2] $false)
    }
    Copy-PackageFile (Join-Path $root 'LICENSE') (Join-Path $payload 'LICENSE')
    Copy-PackageFile (Join-Path $root 'crates/moon-win/assets/icons/moon.ico') (Join-Path $payload 'moon.ico')
    foreach ($notice in 'Symbols-LICENSE.txt','Symbols-NOTICES.txt') {
        Copy-PackageFile (Join-Path $root "crates/moon-win/assets/$notice") (Join-Path $payload "licenses/$notice")
    }
    Copy-PackageFile (Get-Item -LiteralPath $RustLibraryNotices).FullName (Join-Path $payload 'licenses/Rust-library.html')
    $parts = @($Version -split '[.-]' | ForEach-Object { [uint16]$_ })
    if ($parts.Count -eq 3) { $parts += 0 }
    $architecture = if ($platform.arch -eq 'arm64') { 'arm64' } else { 'x64compatible' }
    $signing = @()
    if ($Sign) {
        $pwsh = (Get-Command pwsh.exe -CommandType Application).Source
        $signer = Join-Path $PSScriptRoot 'sign-release-file.ps1'
        $signing = @('/DPublishSigned', ('/Smoon=$q' + $pwsh + '$q -NoProfile -File $q' + $signer + '$q $f'))
    }
    & $compiler /Qp @signing "/DVersion=$Version" "/DNumericVersion=$($parts -join '.')" "/DArchitecture=$architecture" "/DOutputName=$name" "/DPayload=$payload" "/O$($work.FullName)" (Join-Path $PSScriptRoot 'user-setup.iss') | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup compilation failed ($LASTEXITCODE)." }
    $exe = Join-Path $work.FullName ($name + '.exe')
    $hash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $sum = Join-Path $work.FullName 'sha256'
    [IO.File]::WriteAllText($sum, ($hash + '  ' + $name + ".exe" + [char]10), [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($exe,$destination)
    try { [IO.File]::Move($sum,$checksum) } catch { [IO.File]::Delete($destination); throw }
    [pscustomobject]@{installer=$destination;checksum=$checksum;sha256=$hash;version=$Version;target=$Target}
} finally {
    $work.Delete($true)
}
