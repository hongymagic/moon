[CmdletBinding()]
param(
    [string]$MoonPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\moon.exe'),
    [string]$TestwinPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\testwin.exe'),
    [string]$SettingsPath=(Join-Path $env:ProgramFiles 'PowerToys\WinUI3Apps\PowerToys.Settings.exe'),
    [string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\application-integration-results.json'),
    [string]$OwnedSettingsRecordPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\owned-settings-recovery.json'),
    [switch]$SignedInstallation,
    [switch]$ElevatedCoordinator,
    [string]$OwnerSid,
    [long]$PreviousForeground=0,
    [switch]$CompileOnly
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
# Reuse the checked INPUT layout and guarded Alt-chord injector used by S8.
& (Join-Path $PSScriptRoot 'verify-elevated.ps1') -CompileOnly | Out-Null
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class MoonApplicationNative {
    public sealed class Window { public long Id; public uint ProcessId; public string Class; public string Title; }
    delegate bool EnumProc(IntPtr window,IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback,IntPtr data);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window,StringBuilder value,int size);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window,StringBuilder value,int size);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr window,uint attribute,out uint value,uint size);
    [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,uint process);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool QueryFullProcessImageName(IntPtr process,uint flags,StringBuilder name,ref uint size);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetProcessTimes(IntPtr process,out long created,out long exited,out long kernel,out long user);
    public static Window[] Windows() {
        var result=new List<Window>();
        EnumWindows((h,p)=>{if(!IsWindowVisible(h))return true;uint id;GetWindowThreadProcessId(h,out id);var name=new StringBuilder(256);var title=new StringBuilder(2048);GetClassName(h,name,256);GetWindowText(h,title,2048);result.Add(new Window{Id=h.ToInt64(),ProcessId=id,Class=name.ToString(),Title=title.ToString()});return true;},IntPtr.Zero);
        return result.ToArray();
    }
    public static uint ProcessId(long window) {uint process;GetWindowThreadProcessId(new IntPtr(window),out process);return process;}
    public static uint Cloaked(long window) {uint value;Marshal.ThrowExceptionForHR(DwmGetWindowAttribute(new IntPtr(window),14,out value,4));return value;}
    public static string ImagePath(uint process) {var handle=OpenProcess(0x1000,false,process);if(handle==IntPtr.Zero)throw new System.ComponentModel.Win32Exception();try{var name=new StringBuilder(32768);uint size=(uint)name.Capacity;if(!QueryFullProcessImageName(handle,0,name,ref size))throw new System.ComponentModel.Win32Exception();return name.ToString();}finally{CloseHandle(handle);}}
    public static long Birth(uint process) {var handle=OpenProcess(0x1000,false,process);if(handle==IntPtr.Zero)throw new System.ComponentModel.Win32Exception();try{long created,exited,kernel,user;if(!GetProcessTimes(handle,out created,out exited,out kernel,out user))throw new System.ComponentModel.Win32Exception();return created;}finally{CloseHandle(handle);}}
}
'@
if ($CompileOnly) {
    [pscustomobject]@{native_helpers_compiled=$true;input_size=[MoonElevatedNative]::InputSize();desktop_changed=$false} | ConvertTo-Json
    return
}
$MoonPath=(Resolve-Path -LiteralPath $MoonPath).ProviderPath
$TestwinPath=(Resolve-Path -LiteralPath $TestwinPath).ProviderPath
$SettingsPath=(Resolve-Path -LiteralPath $SettingsPath).ProviderPath
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
try {$currentSid=$identity.User.Value} finally {$identity.Dispose()}
if (!$OwnerSid) {$OwnerSid=$currentSid}
if ($OwnerSid -ne $currentSid) {throw 'The test coordinator must use the same Windows account.'}
if ($SignedInstallation) {
    $target=Join-Path (Get-MoonInstallDirectory) 'moon.exe'
    if (![string]::Equals($MoonPath,$target,[StringComparison]::OrdinalIgnoreCase) -or (Get-AuthenticodeSignature -LiteralPath $target).Status -ne 'Valid') {throw 'Signed application testing requires the installed, validly signed moon.exe.'}
    if (!$PreviousForeground) {$PreviousForeground=[MoonElevatedNative]::GetForegroundWindow().ToInt64()}
    if (!(Test-MoonAdministrator)) {
        if ($ElevatedCoordinator) {throw 'Windows did not grant the requested elevated test coordinator.'}
        $arguments=@('-NoProfile','-ExecutionPolicy','Bypass','-File',$PSCommandPath,'-MoonPath',$MoonPath,'-TestwinPath',$TestwinPath,'-SettingsPath',$SettingsPath,'-ReportPath',$ReportPath,'-OwnedSettingsRecordPath',$OwnedSettingsRecordPath,'-OwnerSid',$OwnerSid,'-PreviousForeground',([string]$PreviousForeground),'-SignedInstallation','-ElevatedCoordinator')
        $process=Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -Verb RunAs -WindowStyle Hidden -ArgumentList (($arguments | ForEach-Object {ConvertTo-MoonArgument $_}) -join ' ') -PassThru -Wait
        try {if($process.ExitCode -ne 0){throw 'The elevated application test failed; inspect its report.'}} finally {$process.Dispose()}
        Get-Content -Raw -LiteralPath $ReportPath
        return
    }
    if ([MoonInstallNative]::Integrity() -lt 12288) {throw 'The signed application test needs a real high-integrity coordinator for input over elevated Settings.'}
} elseif ((Test-MoonAdministrator) -or [MoonInstallNative]::Integrity() -ne 8192) {throw 'Run development application tests with an ordinary medium-integrity Windows token.'}
if (!$SignedInstallation) {
    foreach ($runner in @(Get-Process -Name PowerToys -ErrorAction SilentlyContinue)) {
        if ([MoonElevatedNative]::Integrity([uint32]$runner.Id) -gt 8192) {throw 'PowerToys runs elevated; use the signed installation scenario before opening its Settings window.'}
    }
}
if (@(Get-Process -Name moon,testwin,winver,glazewm -ErrorAction SilentlyContinue).Count) {throw 'Stop and restore the installed daemon before this isolated test; no moon, testwin, winver or GlazeWM process may be active.'}
$existingSettings=@(Get-Process -Name PowerToys.Settings -ErrorAction SilentlyContinue)
if ($existingSettings.Count) {
    if (!$SignedInstallation -or !(Test-Path -LiteralPath $OwnedSettingsRecordPath)) {throw 'An existing PowerToys Settings process is not owned by this test.'}
    $savedSettings=Get-Content -Raw -LiteralPath $OwnedSettingsRecordPath | ConvertFrom-Json
    if ($existingSettings.Count -ne 1 -or $existingSettings[0].Id -ne $savedSettings.process_id -or [MoonApplicationNative]::Birth([uint32]$existingSettings[0].Id) -ne $savedSettings.process_birth -or ![string]::Equals([MoonApplicationNative]::ImagePath([uint32]$existingSettings[0].Id),$SettingsPath,[StringComparison]::OrdinalIgnoreCase)) {throw 'The prior test Settings process identity changed.'}
}
$journal=Join-Path $env:LOCALAPPDATA ('moon\journal-v1-session'+[Diagnostics.Process]::GetCurrentProcess().SessionId+'.json')
if (Test-Path -LiteralPath $journal) {throw 'A recovery journal already exists; it belongs to an earlier run.'}
$runnerBefore=@(Get-Process -Name PowerToys -ErrorAction SilentlyContinue | ForEach-Object {[pscustomobject]@{id=$_.Id;started=$_.StartTime.ToUniversalTime().ToString('o')}})
if (!$PreviousForeground) {$PreviousForeground=[MoonElevatedNative]::GetForegroundWindow().ToInt64()}
[void][MoonElevatedNative]::SetThreadDpiAwarenessContext([IntPtr](-4))
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-applications-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
$config=Join-Path $temporary 'three applications.toml'
$source=@'
config-version = 1
cursor-jump = false
focus-follows-cursor = false
[bar]
enabled = false
[borders]
enabled = false
[[rules]]
commands = ["ignore"]
matches = [{process={not-regex='(?i)^(testwin|winver|PowerToys\.Settings)$'}}]
[[rules]]
commands = ["move-to-workspace 1"]
matches = [{process={equals="testwin"}}]
[[rules]]
commands = ["set-floating", "move-to-workspace 2"]
matches = [{process={equals="winver"}}]
[[rules]]
commands = ["set-tiling", "move-to-workspace 3"]
matches = [{process={equals="PowerToys.Settings"}}]
'@
[IO.File]::WriteAllText($config,$source,[Text.UTF8Encoding]::new($false))
$owned=[Collections.Generic.List[object]]::new()
$processes=[Collections.Generic.List[object]]::new()
$checks=[Collections.Generic.List[object]]::new()
$transitions=[Collections.Generic.List[object]]::new()
$daemonId=$null; $failure=$null; $restored=$false; $hookStats=$null
function Start-OwnedProcess([string]$Path,[string]$Arguments='') {
    $start=[Diagnostics.ProcessStartInfo]::new(); $start.FileName=$Path; $start.Arguments=$Arguments; $start.UseShellExecute=$false; $start.CreateNoWindow=$true
    $process=[Diagnostics.Process]::Start($start); $processes.Add($process); return $process
}
function Invoke-SignedMoon([string[]]$Words,[int]$Seconds=20) {
    $launcher=Start-Process -FilePath $MoonPath -ArgumentList (($Words | ForEach-Object {ConvertTo-MoonArgument $_}) -join ' ') -WindowStyle Hidden -PassThru
    try {
        if (!$launcher.WaitForExit($Seconds*1000)) {$launcher.Kill();$launcher.WaitForExit();throw 'The signed moon operation timed out.'}
        if ($launcher.ExitCode -ne 0) {throw ('The signed moon operation failed: '+($Words -join ' '))}
    } finally {$launcher.Dispose()}
}
function Add-OwnedWindow([string]$Name,[int]$Workspace,[string]$ExpectedPath) {
    $script:found=$null
    Wait-MoonCondition -Description ('new '+$Name+' window') -Seconds 20 -Condition {
        $matching=@([MoonApplicationNative]::Windows() | Where-Object {
            $process=Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue
            if ($process -and $process.ProcessName -eq $Name -and [string]::Equals([MoonApplicationNative]::ImagePath([uint32]$process.Id),$ExpectedPath,[StringComparison]::OrdinalIgnoreCase) -and @($processes | Where-Object Id -eq $process.Id).Count -eq 0) {
                # All three application names were absent before this run. Keep
                # the actual delegated Settings process for failure cleanup too.
                $processes.Add($process)
            }
            $process -and $process.ProcessName -eq $Name -and $_.Title.Length -gt 0
        })
        if ($matching.Count -ne 1) {return $false}
        $script:found=$matching[0]; return $true
    }
    $process=Get-Process -Id $script:found.ProcessId
    $image=[MoonApplicationNative]::ImagePath([uint32]$process.Id)
    if (![string]::Equals($image,$ExpectedPath,[StringComparison]::OrdinalIgnoreCase)) {throw 'The newly opened application is running from an unexpected path.'}
    $birth=[MoonApplicationNative]::Birth([uint32]$process.Id)
    $item=[pscustomobject]@{name=$Name;workspace=$Workspace;id=$script:found.Id;process_id=$process.Id;birth=$birth;started=[DateTime]::FromFileTimeUtc($birth).ToString('o');class=$script:found.Class;title=$script:found.Title;original=[MoonElevatedNative]::Frame($script:found.Id);path=$image;version=[Diagnostics.FileVersionInfo]::GetVersionInfo($image).FileVersion;integrity_rid=[MoonElevatedNative]::Integrity([uint32]$process.Id)}
    if ([MoonApplicationNative]::Cloaked($item.id) -ne 0) {throw 'A newly opened test window is unexpectedly cloaked.'}
    $owned.Add($item)
}
function Test-OwnedIdentity($Item) {
    if (![MoonElevatedNative]::IsWindow([IntPtr][long]$Item.id) -or [MoonApplicationNative]::ProcessId([long]$Item.id) -ne $Item.process_id) {return $false}
    $process=Get-Process -Id $Item.process_id -ErrorAction SilentlyContinue
    return $process -and [MoonApplicationNative]::Birth([uint32]$process.Id) -eq $Item.birth
}
function Test-Frame($Item,$Rectangle) {
    $frame=[MoonElevatedNative]::Frame([long]$Item.id)
    return [Math]::Abs($frame.Left-$Rectangle.x) -le 1 -and [Math]::Abs($frame.Top-$Rectangle.y) -le 1 -and [Math]::Abs(($frame.Right-$frame.Left)-$Rectangle.width) -le 1 -and [Math]::Abs(($frame.Bottom-$frame.Top)-$Rectangle.height) -le 1
}
function Check-WinUI([bool]$Floating) {
    $item=$owned[2]
    Wait-MoonCondition -Description 'WinUI 3 committed state and native frame agree' -Condition {
        $placements=@(Invoke-MoonRpc query windows)
        $placement=@($placements | Where-Object {$_.id -eq $item.id})
        return $placement.Count -eq 1 -and $placement[0].visible -and $placement[0].floating -eq $Floating -and [MoonApplicationNative]::Cloaked($item.id) -eq 0 -and (Test-Frame $item $placement[0].rect)
    }
}
function Switch-OwnedWorkspace($From,$To,[int]$Trial=-1) {
    if (!(Test-OwnedIdentity $From) -or !(Test-OwnedIdentity $To)) {throw 'A test window identity changed; input was not sent.'}
    $clock=[Diagnostics.Stopwatch]::StartNew()
    [MoonElevatedNative]::Chord([long]$From.id,[ushort](48+$To.workspace))
    Wait-MoonCondition -Description 'hook-driven foreground matches the requested application' -Seconds 3 -Condition {
        [MoonElevatedNative]::GetForegroundWindow().ToInt64() -eq $To.id -and (Invoke-MoonRpc query world).focused -eq $To.id
    }
    $clock.Stop()
    if ($Trial -ge 0) {$transitions.Add([pscustomobject]@{trial=$Trial;from=$From.name;to=$To.name;from_pid=$From.process_id;to_pid=$To.process_id;foreground=$To.id;elapsed_ms=$clock.Elapsed.TotalMilliseconds;passed=$true})}
}
try {
    if ($SignedInstallation) {Invoke-SignedMoon @('config','validate','--config',$config)}
    else {Invoke-MoonNative $MoonPath @('config','validate','--config',$config) | Out-Null}
    Start-OwnedProcess $TestwinPath '1' | Out-Null
    Add-OwnedWindow 'testwin' 1 $TestwinPath
    $winver=Join-Path $env:SystemRoot 'System32\winver.exe'
    Start-OwnedProcess $winver | Out-Null
    Add-OwnedWindow 'winver' 2 $winver
    # PowerToys Settings forwards an argument-free launch to its existing runner.
    # The newly enumerated window, not this short-lived launcher's PID, owns the UI.
    Start-OwnedProcess $SettingsPath | Out-Null
    Add-OwnedWindow 'PowerToys.Settings' 3 $SettingsPath
    if (@($owned.process_id | Select-Object -Unique).Count -ne 3) {throw 'The focus test did not obtain three distinct application processes.'}
    try {
        if ($SignedInstallation) {
            Invoke-SignedMoon @('start','--background','--no-surfaces','--config',$config)
            Wait-MoonCondition -Description 'the signed isolated UIAccess daemon' -Condition {
                $running=Get-MoonDaemon
                if ($running -and $running.token.ui_access -and [string]::Equals($running.executable,$MoonPath,[StringComparison]::OrdinalIgnoreCase)) {$script:daemonId=[int]$running.process_id;return $true}
                return $false
            }
        } else {
            $start=(Invoke-MoonNative $MoonPath @('start','--background','--no-surfaces','--config',$config) -TimeoutSeconds 20).Output | ConvertFrom-Json
            $daemonId=[int]$start.process_id
        }
    } finally {
        if (!$daemonId) {
            $newDaemon=@(Get-CimInstance Win32_Process -Filter "Name='moon.exe'" | Where-Object {
                $_.ExecutablePath -and [string]::Equals($_.ExecutablePath,$MoonPath,[StringComparison]::OrdinalIgnoreCase) -and $_.CommandLine.Contains($config)
            })
            if ($newDaemon.Count -eq 1) {$daemonId=[int]$newDaemon[0].ProcessId}
        }
    }
    Wait-MoonCondition -Description 'only the three owned applications are managed' -Condition {
        $world=Invoke-MoonRpc query world
        $ids=@($world.windows.PSObject.Properties | ForEach-Object {[long]$_.Name})
        return $ids.Count -eq 3 -and @($ids | Where-Object {$owned.id -notcontains $_}).Count -eq 0
    }
    Invoke-MoonRpc command -Commands @('workspace 3') | Out-Null
    Wait-MoonCondition -Description 'WinUI application foreground' -Condition {[MoonElevatedNative]::GetForegroundWindow().ToInt64() -eq $owned[2].id}
    Check-WinUI $false
    $checks.Add([pscustomobject]@{check='WinUI 3 exact tiling';passed=$true})
    Invoke-MoonRpc command -Commands @('toggle-floating') | Out-Null
    Check-WinUI $true
    $checks.Add([pscustomobject]@{check='WinUI 3 floating and exact frame';passed=$true})
    Switch-OwnedWorkspace $owned[2] $owned[0]
    Wait-MoonCondition -Description 'WinUI hidden on its inactive workspace' -Condition {
        $frame=[MoonElevatedNative]::Frame($owned[2].id)
        [MoonApplicationNative]::Cloaked($owned[2].id) -ne 0 -or $frame.Left -le -30000
    }
    Switch-OwnedWorkspace $owned[0] $owned[2]
    Check-WinUI $true
    $checks.Add([pscustomobject]@{check='WinUI 3 hide and floating restoration';passed=$true})
    Invoke-MoonRpc command -Commands @('toggle-floating') | Out-Null
    Check-WinUI $false
    Switch-OwnedWorkspace $owned[2] $owned[0]
    $from=$owned[0]
    for ($trial=0;$trial -lt 100;$trial++) {
        $to=$owned[($trial+1)%3]
        Switch-OwnedWorkspace $from $to $trial
        $from=$to
    }
    $hookStats=(Invoke-MoonRpc query stats).hooks
    $checks.Add([pscustomobject]@{check='S4 hook-driven focus across three applications';passed=$true;successes=$transitions.Count;trials=100})
} catch {$failure=$_}
finally {
    if ($daemonId) {
        try {
            $running=Get-MoonDaemon
            if ($running -and $running.process_id -eq $daemonId) {Invoke-MoonRpc command -Commands @('exit') | Out-Null}
            try {Wait-MoonCondition -Description 'owned daemon and watchdog restoration' -Seconds 5 -Condition {!(Get-Process -Id $daemonId -ErrorAction SilentlyContinue) -and !(Test-Path -LiteralPath $journal)}}
            catch {
                $stuck=Get-CimInstance Win32_Process -Filter "ProcessId=$daemonId"
                if ($stuck -and $stuck.CommandLine.Contains($config)) {Stop-Process -Id $daemonId -Force}
                Wait-MoonCondition -Description 'owned watchdog restores windows after forced test cleanup' -Condition {!(Get-Process -Id $daemonId -ErrorAction SilentlyContinue) -and !(Test-Path -LiteralPath $journal)}
                throw 'The test daemon required forced cleanup instead of a clean exit.'
            }
            foreach ($item in $owned) {
                if (!(Test-OwnedIdentity $item)) {throw 'An application closed before its original frame could be verified.'}
                $original=$item.original
                $rectangle=[pscustomobject]@{x=$original.Left;y=$original.Top;width=$original.Right-$original.Left;height=$original.Bottom-$original.Top}
                if ([MoonApplicationNative]::Cloaked($item.id) -ne 0 -or !(Test-Frame $item $rectangle)) {throw ('Original frame restoration failed for '+$item.name)}
            }
            $restored=$true
            $checks.Add([pscustomobject]@{check='all three original native frames restored before closing';passed=$true})
        } catch {if (!$failure) {$failure=$_}}
    }
    foreach ($item in $owned) {
        if (Test-OwnedIdentity $item) {[void][MoonElevatedNative]::PostMessage([IntPtr][long]$item.id,0x10,[IntPtr]::Zero,[IntPtr]::Zero)}
    }
    foreach ($item in $owned) {
        $process=Get-Process -Id $item.process_id -ErrorAction SilentlyContinue
        if ($process -and [MoonApplicationNative]::Birth([uint32]$process.Id) -eq $item.birth) {
            try {if (!$process.WaitForExit(3000)) {$process.Kill();$process.WaitForExit()}} catch {if(!$failure){$failure=$_}} finally {$process.Dispose()}
        }
    }
    foreach ($process in $processes) {
        try {if (!$process.HasExited -and !$process.WaitForExit(3000)) {$process.Kill();$process.WaitForExit()}} catch {if(!$failure){$failure=$_}} finally {$process.Dispose()}
    }
    foreach ($runner in $runnerBefore) {
        $current=Get-Process -Id $runner.id -ErrorAction SilentlyContinue
        if ((!$current -or $current.StartTime.ToUniversalTime().ToString('o') -ne $runner.started) -and !$failure) {$failure='The existing PowerToys runner did not survive the test.'}
    }
    if ([MoonElevatedNative]::IsWindow([IntPtr]$PreviousForeground)) {
        [void][MoonElevatedNative]::SetForegroundWindow([IntPtr]$PreviousForeground)
        try {Wait-MoonCondition -Description 'the previous user foreground window' -Seconds 3 -Condition {[MoonElevatedNative]::GetForegroundWindow().ToInt64() -eq $PreviousForeground}}
        catch {if(!$failure){$failure=$_}}
    }
    $report=[pscustomobject]@{recorded_at=[DateTime]::UtcNow.ToString('o');passed=(!$failure);error=if($failure){[string]$failure}else{$null};moon_sha256=(Get-FileHash -LiteralPath $MoonPath -Algorithm SHA256).Hash;applications=$owned.ToArray();checks=$checks.ToArray();focus_transitions=$transitions.ToArray();hook_stats=$hookStats;original_frames_restored=$restored;existing_powertoys_runners=$runnerBefore;input_scope='Alt workspace chords only while a proven newly owned test HWND is foreground';evidence_directory=$temporary}
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
    # Evidence may live on a WSL share, which does not support Windows ACL APIs.
    # This report carries no installation authority and needs no protected ACL.
    [IO.File]::WriteAllText($ReportPath,($report | ConvertTo-Json -Depth 12),[Text.UTF8Encoding]::new($false))
}
if ($failure) {throw $failure}
$report | ConvertTo-Json -Depth 12
