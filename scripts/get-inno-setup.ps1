# Fetch the release compiler into a private build directory in portable mode.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Directory)
$ErrorActionPreference = 'Stop'
$version = '7.1.1'
$sha256 = '8a5a6711c3766a6e5fb74f280ba0e3551e719ce94538d901fdbd1f8f9920b3f0'
$directory = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Directory)
if (Test-Path -LiteralPath $directory) { throw "Choose a new compiler directory: $directory" }
$download = Join-Path ([IO.Path]::GetTempPath()) ('moon-inno-' + [guid]::NewGuid().ToString('N') + '.exe')
try {
    $url = "https://github.com/jrsoftware/issrc/releases/download/is-7_1_1/innosetup-$version-x64.exe"
    $client = [Net.WebClient]::new()
    try { $client.DownloadFile($url, $download) } finally { $client.Dispose() }
    if ((Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash -ine $sha256) { throw 'Incorrect Inno Setup compiler download checksum.' }
    $process = Start-Process -FilePath $download -ArgumentList "/PORTABLE=1 /CURRENTUSER /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /DIR=`"$directory`"" -WindowStyle Hidden -Wait -PassThru
    try { if ($process.ExitCode -ne 0) { throw "Inno Setup compiler extraction failed ($($process.ExitCode))." } } finally { $process.Dispose() }
    (Get-Item -LiteralPath (Join-Path $directory 'ISCC.exe')).FullName
} finally {
    [IO.File]::Delete($download)
}
