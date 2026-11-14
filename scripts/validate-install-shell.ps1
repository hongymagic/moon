[CmdletBinding()]
param([string]$ReportPath)
. (Join-Path $PSScriptRoot 'install-common.ps1')
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-shell-check-'+[guid]::NewGuid().ToString('N'))
$registryPath='Software\moon-shell-check-'+[guid]::NewGuid().ToString('N')
$root=[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($registryPath)
[void][IO.Directory]::CreateDirectory($temporary)
$programs=Join-Path $temporary 'Programs'
[void][IO.Directory]::CreateDirectory($programs)
[IO.File]::WriteAllText((Join-Path $temporary 'moon.exe'),'shortcut target fixture')
$state=[pscustomobject]@{install_id=[guid]::NewGuid().ToString('N');owner_sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value;files=@('moon.exe');shell=$null}
$checks=[Collections.Generic.List[string]]::new()
function Plan([string]$Version='0.1.0') {New-MoonShellPlan -Directory $temporary -State $state -Version $Version -ShortcutArguments '' -UninstallCommand '"C:\Program Files\moon\moon.exe" uninstall' -ProgramsDirectory $programs -RegistryRoot $root}
function Expect-Rejection([scriptblock]$Action,[string]$Description) {
    $rejected=$false;try {& $Action} catch {$rejected=$true}
    if (!$rejected) {throw $Description}
}
try {
    $plan=Plan
    if ($plan.before_registry.exists -or $null -ne $plan.before_shortcut) {throw 'First install unexpectedly captured existing registration.'}
    $state.shell=$plan.ownership
    Publish-MoonShellPlan $plan $state $root
    $snapshot=Read-MoonShellRegistry $root $plan.paths.registry
    $fields=@{};foreach ($value in $snapshot.values) {$fields[$value.name]=$value.value}
    if ($fields.DisplayName -ne 'Moon' -or $fields.DisplayVersion -ne '0.1.0' -or $fields.NoModify -ne 1 -or $fields.NoRepair -ne 1 -or $fields.MoonInstallId -ne $state.install_id) {throw 'Installed Apps fields are incomplete.'}
    $shell=New-Object -ComObject WScript.Shell
    try {
        $link=$shell.CreateShortcut($plan.paths.shortcut)
        try {
            if ($link.TargetPath -ine (Join-Path $temporary 'moon-settings.exe') -or $link.Arguments -ne '' -or $link.IconLocation -ine ((Join-Path $temporary 'moon.exe')+',0')) {throw 'Start menu shortcut target, arguments or icon differ.'}
        } finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($link)}
    } finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)}
    $checks.Add('Actual Shell Link resolves Moon settings and embedded icon; Installed Apps metadata is complete')

    $upgrade=Plan '0.2.0';$state.shell=$upgrade.ownership
    Publish-MoonShellPlan $upgrade $state $root
    Undo-MoonShellPlan $upgrade $state $root
    if ((Read-MoonShellRegistry $root $plan.paths.registry | ConvertTo-Json -Depth 10 -Compress) -cne ($upgrade.before_registry | ConvertTo-Json -Depth 10 -Compress)) {throw 'Upgrade rollback lost previous registration.'}
    if ((Get-FileHash -LiteralPath $plan.paths.shortcut -Algorithm SHA256).Hash -ne $plan.ownership.shortcut_sha256) {throw 'Upgrade rollback changed previous shortcut bytes.'}
    $state.shell=$plan.ownership
    $checks.Add('Failed upgrade restores the exact prior shortcut and application metadata')

    Undo-MoonShellPlan $plan $state $root
    if ((Read-MoonShellRegistry $root $plan.paths.registry).exists -or (Test-Path -LiteralPath $plan.paths.shortcut)) {throw 'Failed first installation left shell entries.'}
    $state.shell=$null
    $checks.Add('Failed first installation removes only its newly created shell entries')

    [IO.File]::WriteAllText($plan.paths.shortcut,'unrelated shortcut fixture')
    Expect-Rejection {Plan | Out-Null} 'Unowned existing shortcut was accepted.'
    if ([IO.File]::ReadAllText($plan.paths.shortcut) -ne 'unrelated shortcut fixture') {throw 'Unowned shortcut was modified.'}
    Remove-Item -LiteralPath $plan.paths.shortcut
    $checks.Add('A preexisting unowned Moon shortcut is refused without modification')

    $key=$root.CreateSubKey($plan.paths.registry)
    try {$key.SetValue('DisplayName','Another application')} finally {$key.Dispose()}
    Expect-Rejection {Plan | Out-Null} 'Unowned existing application registry entry was accepted.'
    $root.DeleteSubKeyTree($plan.paths.registry)
    $checks.Add('An unowned installed-apps key is refused without modification')

    $plan=Plan
    [IO.File]::WriteAllText($plan.paths.shortcut,'newer shortcut fixture')
    Expect-Rejection {Publish-MoonShellPlan $plan $state $root} 'Concurrent shortcut edit was overwritten.'
    if ((Read-MoonShellRegistry $root $plan.paths.registry).exists) {throw 'Failed preflight published application metadata.'}
    Remove-Item -LiteralPath $plan.paths.shortcut
    $checks.Add('Concurrent shortcut creation rejects the stale plan before registry publication')

    $plan=Plan;$state.shell=$plan.ownership
    Publish-MoonShellPlan $plan $state $root
    $upgrade=Plan '0.2.0';$publicationStarted=$false
    $key=$root.OpenSubKey($plan.paths.registry,$true)
    try {$key.SetValue('DisplayName','Newer registered name')} finally {$key.Dispose()}
    try {Publish-MoonShellPlan $upgrade $state $root -Started ([ref]$publicationStarted); throw 'Stale plan was accepted.'}
    catch {if ($publicationStarted -or $_.Exception.Message -notmatch 'changed during setup'){throw}}
    $snapshot=Read-MoonShellRegistry $root $plan.paths.registry
    if (($snapshot.values | Where-Object name -eq 'DisplayName').value -ne 'Newer registered name') {throw 'Stale publication changed the newer registration.'}
    Remove-MoonShellIntegration -State $state -ProgramsDirectory $programs -RegistryRoot $root
    $state.shell=$null
    $checks.Add('A stale registry plan does not enter rollback or overwrite the newer registration')

    $plan=Plan;$state.shell=$plan.ownership;Publish-MoonShellPlan $plan $state $root
    [IO.File]::WriteAllText($plan.paths.shortcut,'edited owned shortcut')
    Remove-MoonShellIntegration -State $state -ProgramsDirectory $programs -RegistryRoot $root -WarningAction SilentlyContinue
    if ([IO.File]::ReadAllText($plan.paths.shortcut) -ne 'edited owned shortcut' -or (Read-MoonShellRegistry $root $plan.paths.registry).exists) {throw 'Uninstall lost the edited shortcut or retained the owned registration.'}
    Remove-Item -LiteralPath $plan.paths.shortcut
    $state.shell=$null
    $checks.Add('Uninstall preserves a later user edit to the shortcut and removes its owned app listing')

    $plan=Plan;$state.shell=$plan.ownership;Publish-MoonShellPlan $plan $state $root
    $key=$root.OpenSubKey($plan.paths.registry,$true)
    try {$key.SetValue('MoonInstallId','unrelated-owner')} finally {$key.Dispose()}
    Expect-Rejection {Remove-MoonShellIntegration -State $state -ProgramsDirectory $programs -RegistryRoot $root} 'Uninstall removed registration whose ownership changed.'
    if (!(Test-Path -LiteralPath $plan.paths.shortcut)) {throw 'Uninstall changed the shortcut before ownership validation.'}
    $key=$root.OpenSubKey($plan.paths.registry,$true)
    try {$key.SetValue('MoonInstallId',$state.install_id)} finally {$key.Dispose()}
    $checks.Add('Changed app-list ownership rejects removal before touching the shortcut')

    Remove-MoonShellIntegration -State $state -ProgramsDirectory $programs -RegistryRoot $root
    if ((Read-MoonShellRegistry $root $plan.paths.registry).exists -or (Test-Path -LiteralPath $plan.paths.shortcut)) {throw 'Owned uninstall left registration.'}
    $checks.Add('Uninstall removes the unchanged owned shortcut and application listing')

    $report=[pscustomobject]@{passed=$true;recorded_at=[DateTime]::UtcNow.ToString('o');checks=$checks.ToArray();scope='Private HKCU test subtree and temporary files only; no elevation, installation or desktop changes'}
    if ($ReportPath) {[IO.File]::WriteAllText($ReportPath,($report | ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))}
    $report | ConvertTo-Json -Depth 5
} finally {$root.Dispose();[Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($registryPath,$false);Remove-Item -LiteralPath $temporary -Recurse -Force}
