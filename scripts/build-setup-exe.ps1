# Wrap the distribution produced by package.ps1 in a single Windows installer.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ArchivePath,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][ValidatePattern('\A(?:[0-9]+\.[0-9]+\.[0-9]+|[0-9]{4}\.[0-9]{2}\.[0-9]{2}-(?:0|[1-9][0-9]*))\z')][string]$Version,
    [string]$Target='x86_64-pc-windows-msvc'
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'package-target.ps1')
$platform=Get-MoonPackageTarget $Target
Add-Type -AssemblyName System.IO.Compression,System.IO.Compression.FileSystem

$source=(Resolve-Path -LiteralPath $ArchivePath).ProviderPath
$output=$ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
[void][IO.Directory]::CreateDirectory($output)
$name="moon-$Version-windows-$($platform.arch)-setup.exe"
$destination=Join-Path $output $name
$checksum=$destination+'.sha256'
if([IO.File]::Exists($destination) -or [IO.File]::Exists($checksum)){
    throw "Setup output already exists: $destination or $checksum"
}

# Match the native applications' numeric Win32 version, retaining the original
# release tag as the display version in Explorer.
$parts=@($Version -split '[.-]' | ForEach-Object { [uint16]$_ })
if($parts.Count -eq 3){$parts+=0}
$numericVersion=$parts -join '.'
$temporary=Join-Path $output ('.moon-setup-build-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
$utf8=[Text.UTF8Encoding]::new($false)
try {
    # Compile against an immutable input in our own build area. Both the
    # SHA-256 below and the embedded resource describe the very same bytes.
    $payloadPath=Join-Path $temporary 'package.zip'
    [IO.File]::Copy($source,$payloadPath)
    $archive=[IO.Compression.ZipFile]::OpenRead($payloadPath)
    try {
        $manifest=$archive.GetEntry('package-manifest.json')
        if(!$manifest -or !$archive.GetEntry('setup.ps1')){throw 'The ZIP must contain Moon setup and its package manifest.'}
        $reader=[IO.StreamReader]::new($manifest.Open())
        try {$package=$reader.ReadToEnd() | ConvertFrom-Json}finally{$reader.Dispose()}
        if($package.schema_version -ne 1 -or $package.product -cne 'Moon' -or $package.kind -cne 'release' -or
            $package.target -cne $Target -or $package.version -cne $Version){
            throw 'The package identity, release kind, target, or version is incorrect.'
        }
        $expected=@{}
        foreach($file in $package.files){
            if($expected.ContainsKey($file.path) -or $file.path -ieq 'package-manifest.json'){throw 'Duplicate manifest path.'}
            $expected.Add($file.path,$file)
        }
        $seen=@{}
        $directories=@{}
        foreach($entry in $archive.Entries){
            $path=$entry.FullName
            $parent=''
            foreach($segment in $path.Split('/')){
                if($segment -notmatch '^[a-zA-Z0-9_-][a-zA-Z0-9_. -]*$' -or $segment -match '[. ]$|^(?:CON|PRN|AUX|NUL|(?:COM|LPT)[1-9])(?:\.|$)'){
                    throw "Unsafe Windows package path: $path"
                }
                if($parent){
                    if($seen.ContainsKey($parent)){throw "Archive file is used as a directory: $parent"}
                    $directories[$parent]=$true
                    $parent+='/'
                }
                $parent+=$segment
            }
            if($seen.ContainsKey($path) -or $directories.ContainsKey($path)){throw "Conflicting archive path: $path"}
            $seen.Add($path,$true)
            if($path -ceq 'package-manifest.json'){continue}
            $file=$expected[$path]
            if(!$file -or $file.path -cne $path -or $file.bytes -ne $entry.Length){throw "Unexpected or mismatched archive entry: $path"}
            $stream=$entry.Open()
            try {$hash=(Get-FileHash -InputStream $stream -Algorithm SHA256).Hash}finally{$stream.Dispose()}
            if($hash -ine $file.sha256){throw "Archive checksum mismatch: $path"}
        }
        if($seen.Count -ne $expected.Count+1){throw 'The archive is missing manifest entries.'}
    } finally {$archive.Dispose()}

    $sha256=(Get-FileHash -LiteralPath $payloadPath -Algorithm SHA256).Hash
    $sourceRoot=Split-Path -Parent $PSScriptRoot
    $manifestPath=Join-Path $temporary 'setup.manifest'
    $template=[IO.File]::ReadAllText((Join-Path $sourceRoot 'crates/moon/app.manifest'))
    [IO.File]::WriteAllText($manifestPath,$template.Replace('@ASSEMBLY_VERSION@',$numericVersion).Replace('@PROCESSOR_ARCHITECTURE@',$platform.setupManifest).Replace('@UIACCESS@','false').Replace('name="moon"','name="Moon.Setup"').Replace('moon window manager','Moon Setup'),$utf8)
    $metadataPath=Join-Path $temporary 'Package.cs'
    $metadata=@"
using System.Reflection;
[assembly: AssemblyTitle("Moon Setup")]
[assembly: AssemblyProduct("Moon")]
[assembly: AssemblyFileVersion("$numericVersion")]
[assembly: AssemblyInformationalVersion("$Version")]
namespace Moon.Setup { internal static class Package { internal const string Sha256 = "$sha256"; } }
"@
    [IO.File]::WriteAllText($metadataPath,$metadata,$utf8)

    $compiler=Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
    $executable=Join-Path $temporary $name
    [IO.File]::Copy((Join-Path $sourceRoot 'crates/moon-win/assets/icons/moon.ico'),(Join-Path $temporary 'moon.ico'))
    [IO.File]::Copy((Join-Path $PSScriptRoot 'setup-exe/Program.cs'),(Join-Path $temporary 'Program.cs'))
    # Simple relative names also work with spaces, commas or Unicode in the checkout.
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=$compiler
    $start.Arguments=@('/nologo','/target:winexe',"/platform:$($platform.setupPlatform)",'/optimize+','/warnaserror+',"/out:$name",'/win32icon:moon.ico',
        '/win32manifest:setup.manifest','/resource:package.zip,Moon.Package.zip','/reference:System.IO.Compression.dll',
        '/reference:System.IO.Compression.FileSystem.dll','/reference:System.Windows.Forms.dll','Program.cs','Package.cs') -join ' '
    $start.WorkingDirectory=$temporary
    $start.UseShellExecute=$false
    $process=[Diagnostics.Process]::Start($start)
    try {
        $process.WaitForExit()
        if($process.ExitCode -ne 0){throw "Windows setup compiler failed with code $($process.ExitCode)."}
    } finally {$process.Dispose()}
    $sha256=(Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    $checksumPath=Join-Path $temporary 'checksum.txt'
    [IO.File]::WriteAllText($checksumPath,($sha256+'  '+$name+"`n"),$utf8)
    [IO.File]::Move($executable,$destination)
    try {[IO.File]::Move($checksumPath,$checksum)}catch{[IO.File]::Delete($destination);throw}
    [pscustomobject]@{installer=$destination;checksum=$checksum;sha256=$sha256}
} finally {
    [IO.Directory]::Delete($temporary,$true)
}
