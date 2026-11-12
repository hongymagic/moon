# Isolated packaging regression checks. Input executables are only read, never run.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Binary,
    [Parameter(Mandatory)][string]$SettingsBinary,
    [Parameter(Mandatory)][string]$InventoryBinary,
    [Parameter(Mandatory)][string]$RecoveryBinary,
    [string]$Version='0.1.0',
    [string]$ReportPath
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$package=Join-Path $PSScriptRoot 'package.ps1'
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-package-tests-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
$checks=[Collections.Generic.List[string]]::new()
$inputs=@{Binary=$Binary;SettingsBinary=$SettingsBinary;InventoryBinary=$InventoryBinary;RecoveryBinary=$RecoveryBinary;Version=$Version;Development=$true}
$before=@{};foreach($path in @($Binary,$SettingsBinary,$InventoryBinary,$RecoveryBinary)){$before[$path]=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash}
function Assert-Rejected([string]$Name,[hashtable]$Arguments,[string]$Pattern) {
    $errorRecord=$null
    try {& $package @Arguments | Out-Null}catch{$errorRecord=$_}
    if(!$errorRecord){throw "Expected packaging rejection: $Name"}
    if($errorRecord.ToString() -notmatch $Pattern){throw "Unexpected rejection for $Name : $errorRecord"}
    if((Test-Path -LiteralPath $Arguments.OutputDirectory) -and @(Get-ChildItem -Force -LiteralPath $Arguments.OutputDirectory).Count){throw "Failed packaging published or leaked files: $Name"}
    $checks.Add($Name)
}
function Replace-PackageTestBytes([string]$Path,[string]$From,[string]$To) {
    $needle=[Text.Encoding]::UTF8.GetBytes($From);$replacement=[Text.Encoding]::UTF8.GetBytes($To)
    if($needle.Length -ne $replacement.Length){throw 'Test mutation must preserve PE offsets.'}
    $bytes=[IO.File]::ReadAllBytes($Path)
    for($index=0;$index -le $bytes.Length-$needle.Length;$index++) {
        $match=$true
        for($offset=0;$offset -lt $needle.Length;$offset++){if($bytes[$index+$offset] -ne $needle[$offset]){$match=$false;break}}
        if($match){[Array]::Copy($replacement,0,$bytes,$index,$replacement.Length);[IO.File]::WriteAllBytes($Path,$bytes);return}
    }
    throw 'Permission declaration was not found in the owned test copy.'
}
try {
    $first=& $package @inputs -OutputDirectory (Join-Path $temporary 'first')
    if(!$first.archive_verified -or $first.kind -ne 'development'){throw 'Development package was not labeled or verified.'}
    $checks.Add('Complete development bundle validates and every ZIP entry passes SHA-256 readback')
    $second=& $package -StageDirectory $first.directory -OutputDirectory (Join-Path $temporary 'second') -Version $Version -Development
    if($first.sha256 -ne $second.sha256 -or (Get-FileHash $first.manifest).Hash -ne (Get-FileHash $second.manifest).Hash){throw 'Identical staged payload did not produce identical ZIP and manifest bytes.'}
    $checks.Add('Repackaging identical staged inputs produces identical manifest and ZIP hashes')
    $existsError=$null
    try {& $package @inputs -OutputDirectory (Join-Path $temporary 'first') | Out-Null}catch{$existsError=$_}
    if(!$existsError -or $existsError.ToString() -notmatch 'already exists' -or (Get-FileHash $first.archive).Hash -ne $first.sha256){throw 'Existing outputs were not preserved.'}
    $checks.Add('Existing output artifacts are rejected and preserved byte-for-byte')

    $missing=$inputs.Clone();$missing.RecoveryBinary=Join-Path $temporary 'missing.exe';$missing.OutputDirectory=Join-Path $temporary 'missing-output'
    Assert-Rejected 'Missing recovery helper rejects the package before publication' $missing 'missing'
    $wrongVersion=$inputs.Clone();$wrongVersion.Version='999.0.0';$wrongVersion.OutputDirectory=Join-Path $temporary 'version-output'
    Assert-Rejected 'Mismatched executable and requested package versions are rejected' $wrongVersion 'identity/version'
    $invalid=Join-Path $temporary 'invalid.exe';[IO.File]::WriteAllText($invalid,'Not an executable.')
    $invalidInputs=$inputs.Clone();$invalidInputs.InventoryBinary=$invalid;$invalidInputs.OutputDirectory=Join-Path $temporary 'invalid-output'
    Assert-Rejected 'A non-PE recovery/inventory executable is rejected' $invalidInputs 'Not a Windows executable'

    $stage=$second.directory
    $staged=@{StageDirectory=$stage;Version=$Version;Development=$true;OutputDirectory=(Join-Path $temporary 'tampered-output')}
    $document=Join-Path $stage 'examples/README.md';$saved=[IO.File]::ReadAllBytes($document)
    try {
        [IO.File]::AppendAllText($document,'Changed after staging.')
        Assert-Rejected 'An existing staged manifest detects changed payload bytes' $staged 'integrity check failed'
    } finally {[IO.File]::WriteAllBytes($document,$saved)}
    $extra=Join-Path $stage 'private-unexpected.txt';[IO.File]::WriteAllText($extra,'Owned fixture only.')
    try {
        $staged.OutputDirectory=Join-Path $temporary 'extra-output'
        Assert-Rejected 'Unlisted staged files cannot enter the distribution' $staged 'Unexpected file'
    } finally {Remove-Item -LiteralPath $extra}
    $helper=Join-Path $stage 'recover_windows.exe';$held=Join-Path $temporary 'held-helper.exe';[IO.File]::Move($helper,$held)
    try {
        $staged.OutputDirectory=Join-Path $temporary 'incomplete-stage-output'
        Assert-Rejected 'An incomplete staged package is rejected' $staged 'missing'
    } finally {[IO.File]::Move($held,$helper)}

    $privileged=Join-Path $temporary 'privileged-settings.exe';[IO.File]::Copy($SettingsBinary,$privileged)
    Replace-PackageTestBytes $privileged 'uiAccess="false"' 'uiAccess="true" '
    $privilegedInputs=$inputs.Clone();$privilegedInputs.SettingsBinary=$privileged;$privilegedInputs.OutputDirectory=Join-Path $temporary 'privileged-output'
    Assert-Rejected 'Settings with UIAccess is rejected even for development packaging' $privilegedInputs 'must declare uiAccess=false'
    $daemon=Join-Path $temporary 'invalid-daemon.exe';[IO.File]::Copy($Binary,$daemon)
    # Both declarations are invalid in strict mode. Preserve byte lengths/resources.
    try {Replace-PackageTestBytes $daemon 'uiAccess="true"' 'uiAccess="none"'}catch {
        if($_.ToString() -notmatch 'Permission declaration was not found'){throw}
    }
    $daemonInputs=$inputs.Clone();$daemonInputs.Remove('Development');$daemonInputs.Binary=$daemon;$daemonInputs.OutputDirectory=Join-Path $temporary 'daemon-output'
    Assert-Rejected 'Strict packaging requires the daemon UIAccess declaration' $daemonInputs 'must declare uiAccess=true'

    $record=Get-Content -Raw -LiteralPath $first.manifest | ConvertFrom-Json
    $external=@($record.files | ForEach-Object {$_.imports} | Where-Object {$_ -match '^(vcruntime|msvcp|msvcr|concrt|vccorlib)[0-9].*\.dll$|^ucrtbased\.dll$'})
    if($external.Count) {
        $runtimeDaemon=Join-Path $temporary 'runtime-daemon.exe';[IO.File]::Copy($Binary,$runtimeDaemon)
        try {Replace-PackageTestBytes $runtimeDaemon 'uiAccess="false"' 'uiAccess="true" '}catch {
            if($_.ToString() -notmatch 'Permission declaration was not found'){throw}
        }
        $runtimeInputs=$inputs.Clone();$runtimeInputs.Remove('Development');$runtimeInputs.Binary=$runtimeDaemon;$runtimeInputs.OutputDirectory=Join-Path $temporary 'runtime-output'
        Assert-Rejected 'Strict packaging rejects an external Visual C++ runtime dependency' $runtimeInputs 'external Visual C\+\+ runtime'
    }

    # Execute the real publication/cleanup body with a failure injected after
    # its first file publication. Lock one owned file to exercise cleanup failure.
    $instrumentedDirectory=Join-Path $temporary 'instrumented';[void][IO.Directory]::CreateDirectory($instrumentedDirectory)
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'install-common.ps1') -Destination $instrumentedDirectory
    $source=[IO.File]::ReadAllText($package)
    $rootLine='$repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot ''..''))'
    $publishLine='[IO.File]::Move($pair[0],$pair[1]);$published.Add($pair[1])'
    if(!$source.Contains($rootLine) -or !$source.Contains($publishLine)){throw 'Packaging fault-injection boundaries changed; update the regression fixture.'}
    $repositoryPath=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).Replace("'","''")
    $source=$source.Replace($rootLine,('$repository='''+$repositoryPath+''''))
    $injection=@'

        if($pair[1] -eq $finalZip) {
            $global:moonPackageTestLock=[IO.File]::Open((Join-Path $finalStage 'README.txt'),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::None)
            throw 'Owned injected publication failure'
        }
'@
    $source=$source.Replace($publishLine,($publishLine+$injection))
    $instrumented=Join-Path $instrumentedDirectory 'package.ps1';[IO.File]::WriteAllText($instrumented,$source,[Text.UTF8Encoding]::new($false))
    $partial=Join-Path $temporary 'partial-output';$publicationError=$null;$cleanupWarnings=@()
    try {
        try {& $instrumented -StageDirectory $first.directory -OutputDirectory $partial -Version $Version -Development -WarningVariable cleanupWarnings -WarningAction SilentlyContinue | Out-Null}catch{$publicationError=$_}
        if(!$publicationError -or $publicationError.ToString() -notmatch 'Owned injected publication failure' -or !$cleanupWarnings.Count){throw 'Cleanup failure masked the original publication error or did not report the locked file.'}
        if(@(Get-ChildItem -LiteralPath $partial -File).Count){throw 'A locked directory prevented cleanup of other published artifacts.'}
        $checks.Add('Publication failure retains its primary error and cleans other artifacts despite a locked file')
    } finally {
        if(Get-Variable -Name moonPackageTestLock -Scope Global -ErrorAction SilentlyContinue){$global:moonPackageTestLock.Dispose();Remove-Variable -Name moonPackageTestLock -Scope Global}
    }

    foreach($path in $before.Keys){if((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $before[$path]){throw "Packaging changed an original executable: $path"}}
    $checks.Add('All original build inputs remain byte-for-byte unchanged')
    $report=[pscustomobject]@{passed=$true;mode='development packaging validation only';checks=$checks.ToArray();development_archive_sha256=$first.sha256;executables_started=$false;installation_changed=$false}
    if($ReportPath){[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)));[IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))}
    $report | ConvertTo-Json -Depth 5
} finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
