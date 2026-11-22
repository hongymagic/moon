[CmdletBinding()]
param([string]$InventoryPath,[string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\window-inventory.json'))
. (Join-Path $PSScriptRoot 'install-common.ps1')
if (!$InventoryPath) {$InventoryPath=Join-Path (Get-MoonBuildDirectory release) 'examples\window_inventory.exe'}
$native=(Invoke-MoonNative (Resolve-Path -LiteralPath $InventoryPath).ProviderPath).Output | ConvertFrom-Json
$glazePath=Join-Path $env:ProgramFiles 'glzr.io\GlazeWM\cli\glazewm.exe'
$glaze=$null; $glazeError=$null
if (Test-Path -LiteralPath $glazePath) {
    try {
        $query=Invoke-MoonNative $glazePath @('query','windows')
        $glaze=$query.Output | ConvertFrom-Json
    } catch {$glazeError=$_.Exception.Message}
}
$record=[pscustomobject]@{recorded_at=[DateTime]::UtcNow.ToString('o');native=$native;glaze=$glaze;glaze_error=$glazeError}
[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
Write-MoonJsonFile $ReportPath $record
[pscustomobject]@{report=$ReportPath;manageable_count=@($native.windows).Count;default_ignored_count=@($native.windows | Where-Object ignored_by_defaults).Count;cloaked=@($native.windows | Where-Object {$_.cloaked -ne 0} | ForEach-Object {[pscustomobject]@{id=$_.window.id;process=$_.window.process;class=$_.window.class;cloaked=$_.cloaked;on_current_virtual_desktop=$_.on_current_virtual_desktop;ignored=$_.ignored_by_defaults}});glaze_error=$glazeError} | ConvertTo-Json -Depth 6
