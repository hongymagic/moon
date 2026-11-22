[CmdletBinding()]
param([switch]$RestoreGlazeForCapture,[string]$InventoryPath,[string]$RecoveryPath,[string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\glaze-enumeration.json'))
. (Join-Path $PSScriptRoot 'install-common.ps1')
if (!$InventoryPath) {$InventoryPath=Join-Path (Get-MoonBuildDirectory release) 'examples\window_inventory.exe'}
$InventoryPath=(Resolve-Path -LiteralPath $InventoryPath).ProviderPath
if (!$RecoveryPath) {$RecoveryPath=Join-Path (Get-MoonBuildDirectory release) 'examples\recover_windows.exe'}
if ($RestoreGlazeForCapture) {$RecoveryPath=(Resolve-Path -LiteralPath $RecoveryPath).ProviderPath}
$session=[Diagnostics.Process]::GetCurrentProcess().SessionId
if (@(Get-CimInstance Win32_Process | Where-Object {$_.SessionId -eq $session -and $_.Name -in @('moon.exe','testwin.exe')}).Count) {throw 'Wait for the active moon/testwin validation run to finish before the Glaze comparison.'}
$glaze=Join-Path $env:ProgramFiles 'glzr.io\GlazeWM\glazewm.exe'
$cli=Join-Path $env:ProgramFiles 'glzr.io\GlazeWM\cli\glazewm.exe'
function Inventory { (Invoke-MoonNative $InventoryPath).Output | ConvertFrom-Json }
function GlazeWindows { (Invoke-MoonNative $cli @('query','windows')).Output | ConvertFrom-Json }
$before=Inventory
$started=$false; $during=$null; $glazeWindows=$null; $after=$null; $recovered=$null
try {
    if (@(Get-CimInstance Win32_Process | Where-Object {$_.SessionId -eq $session -and $_.Name -eq 'glazewm.exe'}).Count -eq 0) {
        if (!$RestoreGlazeForCapture) {throw 'Glaze is not running; pass -RestoreGlazeForCapture for a controlled temporary start and normal exit.'}
        Start-Process -FilePath $glaze
        $started=$true
    }
    Wait-MoonCondition -Description 'GlazeWM query endpoint' -Condition {$query=GlazeWindows; $query.success}
    Start-Sleep -Seconds 1
    $glazeWindows=GlazeWindows
    $during=Inventory
} finally {
    try {
        if ($started) {
            $result=(Invoke-MoonNative $cli @('command','wm-exit')).Output | ConvertFrom-Json
            if (!$result.success) {throw 'Glaze refused normal exit after the enumeration comparison.'}
            Wait-MoonCondition -Description 'temporary GlazeWM to exit normally' -Condition {@(Get-CimInstance Win32_Process | Where-Object {$_.SessionId -eq $session -and $_.Name -eq 'glazewm.exe'}).Count -eq 0}
            if ($glazeWindows -and $during) {
                $beforeIds=@($before.windows | ForEach-Object {$_.window.id})
                $identities=@($before.windows)+@($during.windows | Where-Object {$beforeIds -notcontains $_.window.id})
                $identityIds=@($identities | ForEach-Object {$_.window.id})
                $handles=@($glazeWindows.data.windows | Where-Object {$identityIds -contains $_.handle} | ForEach-Object {[string]$_.handle})
                if ($handles.Count) {
                    $sourcePath=$ReportPath+'.recovery-source.json'
                    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($sourcePath)))
                    Write-MoonJsonFile $sourcePath ([pscustomobject]@{native=[pscustomobject]@{windows=$identities}})
                    $recovered=(Invoke-MoonNative $RecoveryPath (@($sourcePath)+$handles)).Output | ConvertFrom-Json
                }
            }
            Start-Sleep -Milliseconds 500
        }
    } finally {
        $after=Inventory
        $record=[pscustomobject]@{recorded_at=[DateTime]::UtcNow.ToString('o');temporarily_started_glaze=$started;before=$before;glaze=$glazeWindows;during=$during;after=$after;owned_window_recovery=$recovered}
        [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
        Write-MoonJsonFile $ReportPath $record
    }
}
$beforeTitled=@($before.windows | Where-Object {$_.cloaked -eq 2 -and $_.on_current_virtual_desktop -and $_.window.title})
$recovery=@(foreach($window in $beforeTitled){$current=@($after.windows | Where-Object {$_.window.id -eq $window.window.id -and $_.window.process_id -eq $window.window.process_id});[pscustomobject]@{id=$window.window.id;process=$window.window.process;present=($current.Count -eq 1);cloaked_after=if($current.Count -eq 1){$current[0].cloaked}else{$null}}})
[pscustomobject]@{report=$ReportPath;before_count=@($before.windows).Count;during_count=@($during.windows).Count;after_count=@($after.windows).Count;recovery=$recovery} | ConvertTo-Json -Depth 6
