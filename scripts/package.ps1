# Build a validated distribution from explicit, already-built Windows executables.
# The build runs the Windows compiler. It never starts Moon or requests elevation.
[CmdletBinding(DefaultParameterSetName='Binaries')]
param(
    [Parameter(Mandatory,ParameterSetName='Binaries')][string]$Binary,
    [Parameter(Mandatory,ParameterSetName='Binaries')][string]$SettingsBinary,
    [Parameter(Mandatory,ParameterSetName='Binaries')][string]$InventoryBinary,
    [Parameter(Mandatory,ParameterSetName='Binaries')][string]$RecoveryBinary,
    [Parameter(ParameterSetName='Binaries')][string]$RustLibraryNotices,
    [Parameter(Mandatory,ParameterSetName='Staged')][string]$StageDirectory,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][ValidatePattern('\A(?:[0-9]+\.[0-9]+\.[0-9]+|[0-9]{4}\.[0-9]{2}\.[0-9]{2}-(?:0|[1-9][0-9]*))\z')][string]$Version,
    [string]$Target='x86_64-pc-windows-msvc',
    [switch]$Development
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'install-common.ps1')
. (Join-Path $PSScriptRoot 'package-target.ps1')
$platform=Get-MoonPackageTarget $Target
Add-Type -AssemblyName System.IO.Compression,System.IO.Compression.FileSystem
. (Join-Path $PSScriptRoot 'package-image.ps1')
$repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$utf8=[Text.UTF8Encoding]::new($false)
$mode=if($Development){'development'}else{'release'}
$name="moon-$Version-windows-$($platform.arch)"+$(if($Development){'-development'}else{''})

# Explicit payload: setup chooses handover on first install, whose verification
# and recovery dependencies must travel with it. No arbitrary repository files.
$sources=[ordered]@{}
$stagedHashes=@{}
foreach($file in @('Install Moon.cmd','setup.ps1','install.ps1','uninstall.ps1','install-common.ps1','handover.ps1','verify-uiaccess.ps1')) {
    $sources[$file]=Join-Path $PSScriptRoot $file
}
foreach($file in @('examples/workspace-state.ps1','examples/README.md')) {
    $sources[$file]=Join-Path $repository $file
}
foreach($document in Get-ChildItem -LiteralPath (Join-Path $repository 'docs') -File) {
    if($document.Extension -in @('.md','.json')){$sources['docs/'+$document.Name]=$document.FullName}
}
$sources['LICENSE']=Join-Path $repository 'LICENSE'
$sources['licenses/Symbols-NOTICES.txt']=Join-Path $repository 'crates/moon-win/assets/Symbols-NOTICES.txt'
$sources['licenses/Symbols-LICENSE.txt']=Join-Path $repository 'crates/moon-win/assets/Symbols-LICENSE.txt'
$rustLicenseFile='licenses/Rust-library.html'
if($PSCmdlet.ParameterSetName -eq 'Staged' -and (Test-Path -LiteralPath (Join-Path $StageDirectory $rustLicenseFile))) {
    $RustLibraryNotices=Join-Path $StageDirectory $rustLicenseFile
}
if($RustLibraryNotices){$sources[$rustLicenseFile]=$RustLibraryNotices}
$sources['moon.exe']=$Binary
$sources['moon-settings.exe']=$SettingsBinary
$sources['window_inventory.exe']=$InventoryBinary
$sources['recover_windows.exe']=$RecoveryBinary

function Get-PackageTree([string]$Root) {
    $pending=[Collections.Generic.Stack[string]]::new();$pending.Push($Root)
    while($pending.Count) {
        $directory=$pending.Pop()
        if((Get-Item -Force -LiteralPath $directory).Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Staged directories cannot be links: $directory"}
        foreach($item in Get-ChildItem -Force -LiteralPath $directory) {
            if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Staged entries cannot be links: $($item.FullName)"}
            if($item.PSIsContainer){$pending.Push($item.FullName)}else{$item.FullName.Substring($Root.Length+1).Replace('\','/')}
        }
    }
}
if($PSCmdlet.ParameterSetName -eq 'Staged') {
    $StageDirectory=(Resolve-Path -LiteralPath $StageDirectory).ProviderPath.TrimEnd('\','/')
    $allowed=@($sources.Keys)+@('README.txt','package-manifest.json')
    foreach($file in @(Get-PackageTree $StageDirectory)) {
        if($allowed -notcontains $file){throw "Unexpected file in staged package: $file"}
    }
    $previousManifest=Join-Path $StageDirectory 'package-manifest.json'
    if(Test-Path -LiteralPath $previousManifest) {
        Assert-PackageFile $previousManifest
        if((Get-Item -LiteralPath $previousManifest).Length -gt 1048576){throw 'Staged manifest is too large.'}
        $previous=Get-Content -Raw -LiteralPath $previousManifest | ConvertFrom-Json
        if($previous.schema_version -ne 1 -or $previous.product -ne 'Moon' -or $previous.version -ne $Version -or $previous.target -cne $Target -or $previous.kind -ne $mode){throw 'Staged manifest identity does not match the requested package.'}
        if(@($previous.files).Count -ne $sources.Count+1){throw 'Staged manifest does not describe the complete payload.'}
        $seen=@{}
        foreach($entry in $previous.files) {
            if(($sources.Keys -notcontains $entry.path -and $entry.path -ne 'README.txt') -or $seen.ContainsKey($entry.path)){throw "Unexpected or duplicate staged manifest entry: $($entry.path)"}
            $seen[$entry.path]=$true
            $path=Join-Path $StageDirectory $entry.path
            Assert-PackageFile $path
            if($entry.sha256 -notmatch '^[a-fA-F0-9]{64}$' -or (Get-Item -LiteralPath $path).Length -ne $entry.bytes -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $entry.sha256){throw "Staged manifest integrity check failed: $($entry.path)"}
            $stagedHashes[$entry.path]=$entry.sha256
        }
    }
    foreach($file in @($sources.Keys)){$sources[$file]=Join-Path $StageDirectory $file}
}
foreach($source in $sources.Values){Assert-PackageFile $source}
$OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
[void][IO.Directory]::CreateDirectory($OutputDirectory)
Assert-MoonNoReparse $OutputDirectory
$finalStage=Join-Path $OutputDirectory $name
$finalZip=Join-Path $OutputDirectory ($name+'.zip')
$finalManifest=Join-Path $OutputDirectory ($name+'.manifest.json')
$finalChecksum=Join-Path $OutputDirectory ($name+'.zip.sha256')
$finalInstaller=Join-Path $OutputDirectory ($name+'-uiaccess.exe')
$outputs=@($finalStage,$finalZip,$finalManifest,$finalChecksum)
if(!$Development){$outputs+=@($finalInstaller,($finalInstaller+'.sha256'))}
foreach($path in $outputs) {
    if(Test-Path -LiteralPath $path){throw "Package output already exists; choose another output directory: $path"}
}
$temporary=Join-Path $OutputDirectory ('.moon-package-'+[guid]::NewGuid().ToString('N'))
$payload=Join-Path $temporary 'payload'
$published=[Collections.Generic.List[string]]::new()
$primaryError=$null
$cleanupLeftovers=[Collections.Generic.List[string]]::new()
$report=$null
try {
    [void][IO.Directory]::CreateDirectory($payload)
    foreach($file in $sources.Keys){Copy-PackageFile $sources[$file] (Join-Path $payload $file)}
    # A staged file might change after its first check and before its read lock.
    # Compare the immutable copies too, against the manifest already read.
    foreach($file in $sources.Keys) {
        if($stagedHashes.ContainsKey($file) -and (Get-FileHash -LiteralPath (Join-Path $payload $file) -Algorithm SHA256).Hash -ne $stagedHashes[$file]){throw "Staged input changed while packaging: $file"}
    }
    $identities=@{};$imports=@{}
    foreach($file in @('moon.exe','moon-settings.exe','window_inventory.exe','recover_windows.exe')){$imports[$file]=@(Assert-PackagePe (Join-Path $payload $file))}
    $identities['moon.exe']=Read-PackageIdentity (Join-Path $payload 'moon.exe') 'Moon' 'moon.exe' $true
    $identities['moon-settings.exe']=Read-PackageIdentity (Join-Path $payload 'moon-settings.exe') 'Moon Settings' 'moon-settings.exe' $false
    if(!$Development) {
        foreach($file in $imports.Keys) {
            $external=@($imports[$file] | Where-Object {$_ -match '^(vcruntime|msvcp|msvcr|concrt|vccorlib)[0-9].*\.dll$|^ucrtbased\.dll$'})
            if($external.Count){throw "The $file image imports an external Visual C++ runtime: $($external -join ', '). Rebuild with +crt-static."}
        }
    }
    foreach($file in $sources.Keys) {
        if($file.EndsWith('.ps1',[StringComparison]::OrdinalIgnoreCase)) {
            $tokens=$null;$errors=$null
            [void][Management.Automation.Language.Parser]::ParseFile((Join-Path $payload $file),[ref]$tokens,[ref]$errors)
            if($errors.Count){throw "The package contains invalid PowerShell: $file - $($errors[0].Message)"}
        }
    }
    $readme=@"
Moon $Version for Windows 11 ($($platform.arch))

Extract the entire ZIP to one folder before opening Install Moon.cmd.
Setup requests Windows approval when needed and preserves your personal settings.
Moon Settings uses the Microsoft Edge WebView2 Evergreen Runtime. Setup explains
how to obtain it from Microsoft if it is missing; this package does not install it.

Read docs/installation.md for installation, removal, and recovery instructions.
Read docs/acceptance-status.md for the measured validation status and limits.
examples/workspace-state.ps1 is a standalone public-IPC extension example.
Packaged setup scripts live beside this README; documentation paths beginning
scripts/ refer to their location in the source checkout.

package-manifest.json lists every payload file's SHA-256 and executable identity.
The accompanying .zip.sha256 file covers the complete ZIP, including that manifest.
The installer signs the UIAccess executable locally during installation; package
creation itself does not install, sign, start, or trust any application.
"@
    if($Development){$readme="DEVELOPMENT PACKAGE - NOT AN INSTALLATION CANDIDATE.`n`n"+$readme}
    [IO.File]::WriteAllText((Join-Path $payload 'README.txt'),($readme.Replace("`r`n","`n")+"`n"),$utf8)
    [string[]]$paths=@($sources.Keys)+@('README.txt')
    [Array]::Sort($paths,[StringComparer]::Ordinal)
    $files=@(foreach($file in $paths) {
        $path=Join-Path $payload $file
        [ordered]@{path=$file;bytes=(Get-Item -LiteralPath $path).Length;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant();identity=$(if($identities.ContainsKey($file)){$identities[$file]}else{$null});imports=$(if($imports.ContainsKey($file)){@($imports[$file])}else{$null})}
    })
    $manifest=[ordered]@{schema_version=1;product='Moon';version=$Version;target=$Target;kind=$mode;files=$files}
    $manifestPath=Join-Path $payload 'package-manifest.json'
    [IO.File]::WriteAllText($manifestPath,(($manifest|ConvertTo-Json -Depth 8).Replace("`r`n","`n")+"`n"),$utf8)
    $expected=@{}
    foreach($file in $files){$expected[$file.path]=$file.sha256}
    $expected['package-manifest.json']=(Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
    [string[]]$entries=@($expected.Keys);[Array]::Sort($entries,[StringComparer]::Ordinal)
    $zipPath=Join-Path $temporary ($name+'.zip')
    $zip=[IO.Compression.ZipFile]::Open($zipPath,[IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach($file in $entries) {
            $entry=$zip.CreateEntry($file,[IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime=[DateTimeOffset]::new(1980,1,1,0,0,0,[TimeSpan]::Zero)
            $entryStream=$entry.Open()
            try {
                $source=[IO.File]::OpenRead((Join-Path $payload $file))
                try {$source.CopyTo($entryStream)}finally{$source.Dispose()}
            } finally {$entryStream.Dispose()}
        }
    } finally {$zip.Dispose()}
    # Verify the actual archive contents rather than trusting the input copies.
    $zip=[IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        if($zip.Entries.Count -ne $entries.Length){throw 'Archive entry count differs from its manifest.'}
        $seen=@{}
        foreach($entry in $zip.Entries) {
            if(!$expected.ContainsKey($entry.FullName) -or $seen.ContainsKey($entry.FullName)){throw "Unexpected or duplicate archive entry: $($entry.FullName)"}
            $seen[$entry.FullName]=$true
            $stream=$entry.Open()
            try {$hash=(Get-FileHash -InputStream $stream -Algorithm SHA256).Hash.ToLowerInvariant()}finally{$stream.Dispose()}
            if($hash -ne $expected[$entry.FullName]){throw "Archive integrity check failed: $($entry.FullName)"}
        }
    } finally {$zip.Dispose()}
    $zipHash=(Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $checksumPath=Join-Path $temporary 'package.sha256'
    [IO.File]::WriteAllText($checksumPath,($zipHash+'  '+[IO.Path]::GetFileName($finalZip)+"`n"),$utf8)
    $outsideManifest=Join-Path $temporary 'manifest.json';[IO.File]::Copy($manifestPath,$outsideManifest)
    $filesToPublish=@(@($zipPath,$finalZip),@($outsideManifest,$finalManifest),@($checksumPath,$finalChecksum))
    if(!$Development) {
        $setup=& (Join-Path $PSScriptRoot 'build-setup-exe.ps1') -ArchivePath $zipPath -Version $Version -Target $Target -OutputDirectory $temporary
        $filesToPublish+=@(@($setup.installer,$finalInstaller),@($setup.checksum,($finalInstaller+'.sha256')))
    }
    [IO.Directory]::Move($payload,$finalStage);$published.Add($finalStage)
    foreach($pair in $filesToPublish) {
        [IO.File]::Move($pair[0],$pair[1]);$published.Add($pair[1])
    }
    $report=[pscustomobject]@{kind=$mode;version=$Version;target=$Target;directory=$finalStage;archive=$finalZip;manifest=$finalManifest;checksum=$finalChecksum;sha256=$zipHash;payload_files=$files.Count;archive_verified=$true;cleanup_leftovers=@()}
    if(!$Development){$report | Add-Member -NotePropertyMembers @{installer=$finalInstaller;installer_checksum=($finalInstaller+'.sha256');installer_sha256=$setup.sha256}}
} catch {
    $primaryError=$_
    foreach($path in $published) {
        try {Remove-Item -LiteralPath $path -Force -Recurse}
        catch {$cleanupLeftovers.Add($path);Write-Warning "Package cleanup could not remove '$path': $($_.Exception.Message)" -WarningAction Continue}
    }
} finally {
    try {if(Test-Path -LiteralPath $temporary){Remove-Item -LiteralPath $temporary -Force -Recurse}}
    catch {$cleanupLeftovers.Add($temporary);Write-Warning "Package cleanup could not remove '$temporary': $($_.Exception.Message)" -WarningAction Continue}
}
if($primaryError){throw $primaryError}
$report.cleanup_leftovers=$cleanupLeftovers.ToArray()
$report
