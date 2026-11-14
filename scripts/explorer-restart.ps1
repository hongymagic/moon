[CmdletBinding()]
param(
    [string]$MoonPath=(Join-Path $env:LOCALAPPDATA 'moon-final-validation\moon.exe'),
    [string]$TestwinPath=(Join-Path $env:LOCALAPPDATA 'moon-final-validation\testwin.exe'),
    [string]$InventoryPath=(Join-Path $env:LOCALAPPDATA 'moon-install-candidate\window_inventory.exe'),
    [string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon-dev\explorer-restart-results.json'),
    [switch]$CompileOnly
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
& (Join-Path $PSScriptRoot 'application-integration.ps1') -CompileOnly | Out-Null
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class MoonExplorerNative {
    [StructLayout(LayoutKind.Sequential)] public struct Rect {public int Left,Top,Right,Bottom;}
    [StructLayout(LayoutKind.Sequential)] struct IconId {public uint Size;public IntPtr Window;public uint Id;public Guid Guid;}
    delegate bool EnumProc(IntPtr window,IntPtr data);
    [DllImport("user32.dll")] public static extern IntPtr GetShellWindow();
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string name,string title);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback,IntPtr data);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window,StringBuilder value,int size);
    [DllImport("shell32.dll")] static extern int Shell_NotifyIconGetRect(ref IconId identifier,out Rect rectangle);
    [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,uint id);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool QueryFullProcessImageName(IntPtr process,uint flags,StringBuilder name,ref uint length);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetProcessTimes(IntPtr process,out long birth,out long exit,out long kernel,out long user);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool TerminateProcess(IntPtr process,uint code);
    [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr handle,uint milliseconds);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    static Exception Error(){return new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());}
    public static Rect TrayRect(uint process) {
        var matches=new List<IntPtr>();
        EnumWindows((h,p)=>{uint id;GetWindowThreadProcessId(h,out id);var name=new StringBuilder(256);GetClassName(h,name,256);if(id==process&&name.ToString()=="moon.notification")matches.Add(h);return true;},IntPtr.Zero);
        if(matches.Count!=1)throw new InvalidOperationException("Expected one moon tray owner window");
        var icon=new IconId{Size=(uint)Marshal.SizeOf(typeof(IconId)),Window=matches[0],Id=1,Guid=Guid.Empty};
        Rect rectangle;int status=Shell_NotifyIconGetRect(ref icon,out rectangle);
        if(status!=0)throw new InvalidOperationException("Tray icon lookup failed: "+status);
        return rectangle;
    }
    public static void StopVerifiedDesktopShell(uint process,long expectedBirth,string expectedPath) {
        var handle=OpenProcess(0x101001,false,process);if(handle==IntPtr.Zero)throw Error();
        try {
            uint desktop,taskbar;GetWindowThreadProcessId(GetShellWindow(),out desktop);GetWindowThreadProcessId(FindWindow("Shell_TrayWnd",null),out taskbar);
            var path=new StringBuilder(32768);uint size=(uint)path.Capacity;long birth,exit,kernel,user;
            if(!QueryFullProcessImageName(handle,0,path,ref size)||!GetProcessTimes(handle,out birth,out exit,out kernel,out user))throw Error();
            if(desktop!=process||taskbar!=process||birth!=expectedBirth||!String.Equals(path.ToString(),expectedPath,StringComparison.OrdinalIgnoreCase)||WaitForSingleObject(handle,0)!=258)throw new InvalidOperationException("Desktop shell identity changed; nothing was terminated");
            if(!TerminateProcess(handle,0))throw Error();
        }finally{CloseHandle(handle);}
    }
}
'@
if ($CompileOnly) {@{native_helpers_compiled=$true;desktop_changed=$false} | ConvertTo-Json; return}
if ((Test-MoonAdministrator) -or [MoonInstallNative]::Integrity() -ne 8192) {throw 'Run the Explorer scenario with an ordinary medium-integrity token.'}
foreach ($variable in @('MoonPath','TestwinPath','InventoryPath')) {Set-Variable -Name $variable -Value ((Resolve-Path -LiteralPath (Get-Variable -Name $variable -ValueOnly)).ProviderPath)}
if (@(Get-Process -Name moon,testwin,glazewm,zebar,tacky-borders -ErrorAction SilentlyContinue).Count) {throw 'Another desktop probe or previous manager is active; nothing was changed.'}
$journal=Join-Path $env:LOCALAPPDATA ('moon\journal-v1-session'+[Diagnostics.Process]::GetCurrentProcess().SessionId+'.json')
if (Test-Path -LiteralPath $journal) {throw 'A prior recovery journal exists; nothing was changed.'}
[void][MoonElevatedNative]::SetThreadDpiAwarenessContext([IntPtr](-4))
function Get-DesktopShell {
    $desktop=[MoonExplorerNative]::GetShellWindow().ToInt64()
    $taskbar=[MoonExplorerNative]::FindWindow('Shell_TrayWnd',$null).ToInt64()
    if (!$desktop -or !$taskbar) {throw 'The desktop shell or taskbar is unavailable.'}
    $owner=[MoonApplicationNative]::ProcessId($desktop)
    if ([MoonApplicationNative]::ProcessId($taskbar) -ne $owner) {throw 'Desktop and taskbar ownership differ.'}
    $identity=[MoonInstallNative]::ProcessIdentity($owner)
    if (![string]::Equals($identity.Path,(Join-Path $env:SystemRoot 'explorer.exe'),[StringComparison]::OrdinalIgnoreCase)) {throw 'Unexpected desktop shell executable.'}
    [pscustomobject]@{process_id=$owner;birth=$identity.Birth;path=$identity.Path;desktop=$desktop;taskbar=$taskbar}
}
function Assert-ShellHasNoUserWindows($Desktop) {
    $shell=New-Object -ComObject Shell.Application
    $registered=@()
    try {
        $collection=$shell.Windows()
        try {
            for ($index=0;$index -lt $collection.Count;$index++) {
                $window=$collection.Item($index)
                try {
                    $handle=[long]$window.HWND
                    $registered+=[pscustomobject]@{id=$handle;process_id=[MoonApplicationNative]::ProcessId($handle);location=[string]$window.LocationURL;name=[string]$window.LocationName}
                } finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($window)}
            }
        } finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($collection)}
    } finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)}
    if (@($registered | Where-Object process_id -eq $Desktop.process_id).Count) {throw 'The desktop shell process owns a registered user folder/control-panel window; no restart was attempted.'}
    $titled=@([MoonApplicationNative]::Windows() | Where-Object {$_.ProcessId -eq $Desktop.process_id -and $_.Title.Length -gt 0 -and $_.Class -notin @('Progman','WorkerW','Shell_TrayWnd','Shell_SecondaryTrayWnd')})
    if ($titled.Count) {throw 'The desktop shell process owns a titled application window; no restart was attempted.'}
    return $registered
}
function Get-Inventory {(Invoke-MoonNative $InventoryPath).Output | ConvertFrom-Json}
function Get-WorkAreas($Inventory) {
    @($Inventory.monitors | Sort-Object id | ForEach-Object {[ordered]@{id=$_.id;bounds=$_.bounds;work_area=$_.work_area;scale=$_.scale}}) | ConvertTo-Json -Depth 5 -Compress
}
function Test-NativePlacements {
    $placements=@(Invoke-MoonRpc query windows -DevelopmentPath $MoonPath)
    if ($placements.Count -ne 4) {return $false}
    foreach ($placement in $placements) {
        $frame=[MoonElevatedNative]::Frame([long]$placement.id);$cloak=[MoonApplicationNative]::Cloaked([long]$placement.id)
        if ($placement.visible) {
            if ($cloak -ne 0 -or [Math]::Abs($frame.Left-$placement.rect.x) -gt 1 -or [Math]::Abs($frame.Top-$placement.rect.y) -gt 1 -or [Math]::Abs($frame.Right-$frame.Left-$placement.rect.width) -gt 1 -or [Math]::Abs($frame.Bottom-$frame.Top-$placement.rect.height) -gt 1) {return $false}
        } elseif ($cloak -eq 0 -and $frame.Left -gt -30000) {return $false}
    }
    return $true
}
function Assert-PreservedApplications {
    foreach ($item in $preserved) {
        if (![MoonElevatedNative]::IsWindow([IntPtr][long]$item.window.id) -or [MoonApplicationNative]::ProcessId([long]$item.window.id) -ne $item.window.process_id -or [MoonApplicationNative]::Birth([uint32]$item.window.process_id) -ne $item.process_birth) {throw ('An existing application identity changed: '+$item.window.id)}
    }
}
function Start-ExplorerIfAbsent {
    if ([MoonExplorerNative]::GetShellWindow() -eq [IntPtr]::Zero) {
        $start=[Diagnostics.ProcessStartInfo]::new();$start.FileName=Join-Path $env:SystemRoot 'explorer.exe';$start.UseShellExecute=$false
        $launched=[Diagnostics.Process]::Start($start);$launched.Dispose()
    }
}
$directory=Join-Path ([IO.Path]::GetTempPath()) ('moon-explorer-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($directory)
$config=Join-Path $directory 'config.toml'
[IO.File]::WriteAllText($config,"config-version = 1`ncursor-jump = false`nfocus-follows-cursor = false`n",[Text.UTF8Encoding]::new($false))
$desktopBefore=Get-DesktopShell
$shellWindows=@(Assert-ShellHasNoUserWindows $desktopBefore)
$inventoryBefore=Get-Inventory
$preserved=@($inventoryBefore.windows | Where-Object {$_.window.process_id -ne $desktopBefore.process_id -and $_.cloaked -eq 0})
$backgroundFrames=@($inventoryBefore.windows | Where-Object {$_.cloaked -ne 0})
if (@($preserved | Where-Object {$null -eq $_.process_birth}).Count) {throw 'A user application identity cannot be verified; no restart was attempted.'}
$workBefore=Get-WorkAreas $inventoryBefore
$previousForeground=[MoonElevatedNative]::GetForegroundWindow().ToInt64()
$helper=$null;$daemonId=0;$daemonBirth=0;$failure=$null;$shellStopped=$false;$originals=@{};$checks=[Collections.Generic.List[string]]::new()
$stateBefore=$null;$stateAfter=$null;$desktopAfter=$null;$workWithMoon=$null;$trayBefore=$null;$trayAfter=$null;$restored=$false;$failureSnapshot=$null
try {
    $start=[Diagnostics.ProcessStartInfo]::new();$start.FileName=$TestwinPath;$start.Arguments='4';$start.UseShellExecute=$false;$start.CreateNoWindow=$true
    $helper=[Diagnostics.Process]::Start($start)
    Wait-MoonCondition -Description 'four owned test windows' -Condition {@([MoonElevatedNative]::Windows([uint32]$helper.Id)).Count -eq 4}
    foreach ($id in [MoonElevatedNative]::Windows([uint32]$helper.Id)) {$originals[[string]$id]=[MoonElevatedNative]::Frame($id)}
    $started=(Invoke-MoonNative $MoonPath @('start','--background','--only','testwin','--surfaces','--config',$config) -TimeoutSeconds 20).Output | ConvertFrom-Json
    $daemonId=[int]$started.process_id;$daemonBirth=[MoonInstallNative]::ProcessIdentity([uint32]$daemonId).Birth
    Wait-MoonCondition -Description 'four owned windows adopted' -Condition {(Invoke-MoonRpc query stats -DevelopmentPath $MoonPath).managed_windows -eq 4 -and (Test-NativePlacements)}
    Invoke-MoonRpc command -DevelopmentPath $MoonPath -Commands @('move-to-workspace 2','workspace 1') | Out-Null
    Wait-MoonCondition -Description 'three visible and one hidden owned window' -Condition {(Test-NativePlacements) -and @((Invoke-MoonRpc query windows -DevelopmentPath $MoonPath) | Where-Object {!$_.visible}).Count -eq 1}
    Wait-MoonCondition -Description 'moon tray icon registered' -Condition {$null -ne [MoonExplorerNative]::TrayRect([uint32]$daemonId)}
    $trayBefore=[MoonExplorerNative]::TrayRect([uint32]$daemonId)
    $stateBefore=Get-MoonLayoutFingerprint -DevelopmentPath $MoonPath
    $workWithMoon=Get-WorkAreas (Get-Inventory)
    if ($workWithMoon -ceq $workBefore) {throw 'The enabled AppBar did not reserve work area before the restart.'}
    Assert-PreservedApplications
    $checks.Add('Four helpers, hidden membership, exact placements, AppBar reservation and tray exist before restart')
    [IO.File]::WriteAllText((Join-Path $directory 'before.json'),(@{shell=$desktopBefore;shell_windows=$shellWindows;user_inventory=$inventoryBefore;world=(Invoke-MoonRpc query world -DevelopmentPath $MoonPath);layout=$stateBefore;work_area=$workWithMoon}|ConvertTo-Json -Depth 100),[Text.UTF8Encoding]::new($false))
    $null=Assert-ShellHasNoUserWindows $desktopBefore
    [MoonExplorerNative]::StopVerifiedDesktopShell([uint32]$desktopBefore.process_id,[long]$desktopBefore.birth,$desktopBefore.path)
    $shellStopped=$true
    Start-Sleep -Seconds 2
    Start-ExplorerIfAbsent
    Wait-MoonCondition -Description 'a replacement desktop and taskbar owner' -Seconds 30 -Condition {
        $current=Get-DesktopShell
        $current.birth -ne $desktopBefore.birth -or $current.process_id -ne $desktopBefore.process_id
    }
    $desktopAfter=Get-DesktopShell
    Wait-MoonCondition -Description 'moon layout, native placements and AppBar after Explorer restart' -Seconds 30 -Condition {
        (Invoke-MoonRpc query stats -DevelopmentPath $MoonPath).process_id -eq $daemonId -and (Get-MoonLayoutFingerprint -DevelopmentPath $MoonPath) -ceq $stateBefore -and (Test-NativePlacements) -and (Get-WorkAreas (Get-Inventory)) -ceq $workWithMoon
    }
    Wait-MoonCondition -Description 'moon tray icon re-added after Explorer restart' -Condition {$null -ne [MoonExplorerNative]::TrayRect([uint32]$daemonId)}
    $trayAfter=[MoonExplorerNative]::TrayRect([uint32]$daemonId)
    $stateAfter=Get-MoonLayoutFingerprint -DevelopmentPath $MoonPath
    Assert-PreservedApplications
    if ([MoonInstallNative]::ProcessIdentity([uint32]$daemonId).Birth -ne $daemonBirth) {throw 'The daemon restarted instead of surviving Explorer.'}
    $checks.Add('New Explorer shell identity; original daemon, full semantic layout, exact frames, work area and tray restored')
    Invoke-MoonRpc command -DevelopmentPath $MoonPath -Commands @('workspace 2') | Out-Null
    Wait-MoonCondition -Description 'the previously hidden workspace usable after shell restart' -Condition {(Test-NativePlacements) -and [MoonElevatedNative]::GetForegroundWindow().ToInt64() -eq (Invoke-MoonRpc query world -DevelopmentPath $MoonPath).focused}
    Invoke-MoonRpc command -DevelopmentPath $MoonPath -Commands @('workspace 1') | Out-Null
    Wait-MoonCondition -Description 'the original workspace and semantic layout restored' -Condition {(Get-MoonLayoutFingerprint -DevelopmentPath $MoonPath) -ceq $stateBefore -and (Test-NativePlacements)}
    $checks.Add('Workspace switching and actual foreground remain usable after restart')
} catch {
    $failure=$_
    # Capture every failed predicate before cleanup changes the evidence.
    $failureSnapshot=[ordered]@{recorded_at=[DateTime]::UtcNow.ToString('o')}
    if ($daemonId) {
        try {$failureSnapshot.stats=Invoke-MoonRpc query stats -DevelopmentPath $MoonPath} catch {$failureSnapshot.stats_error=[string]$_}
        try {$failureSnapshot.world=Invoke-MoonRpc query world -DevelopmentPath $MoonPath} catch {$failureSnapshot.world_error=[string]$_}
        try {
            $failureSnapshot.placements=@(Invoke-MoonRpc query windows -DevelopmentPath $MoonPath)
            $failureSnapshot.native=@($failureSnapshot.placements | ForEach-Object {[ordered]@{id=$_.id;frame=[MoonElevatedNative]::Frame([long]$_.id);cloaked=[MoonApplicationNative]::Cloaked([long]$_.id)}})
            $stateAfter=Get-MoonLayoutFingerprint -World $failureSnapshot.world -Placements $failureSnapshot.placements
            $failureSnapshot.layout=$stateAfter
        } catch {$failureSnapshot.placement_error=[string]$_}
        try {$failureSnapshot.inventory=Get-Inventory;$failureSnapshot.work_area=Get-WorkAreas $failureSnapshot.inventory} catch {$failureSnapshot.inventory_error=[string]$_}
        try {$failureSnapshot.tray=[MoonExplorerNative]::TrayRect([uint32]$daemonId)} catch {$failureSnapshot.tray_error=[string]$_}
    }
    [IO.File]::WriteAllText((Join-Path $directory 'failure-before-cleanup.json'),($failureSnapshot|ConvertTo-Json -Depth 100),[Text.UTF8Encoding]::new($false))
}
finally {
    if ($shellStopped) {
        try {Start-ExplorerIfAbsent;Wait-MoonCondition -Description 'a usable desktop shell during cleanup' -Condition {$null -ne (Get-DesktopShell)}} catch {if(!$failure){$failure=$_}}
    }
    if ($daemonId) {
        try {
            $running=Get-MoonDaemon -DevelopmentPath $MoonPath
            if ($running -and $running.process_id -eq $daemonId) {Invoke-MoonRpc command -DevelopmentPath $MoonPath -Commands @('exit') | Out-Null}
            try {Wait-MoonCondition -Description 'owned daemon and journal cleanup' -Seconds 8 -Condition {!(Get-Process -Id $daemonId -ErrorAction SilentlyContinue) -and !(Test-Path -LiteralPath $journal)}}
            catch {
                if ((Get-Process -Id $daemonId -ErrorAction SilentlyContinue) -and [MoonInstallNative]::ProcessIdentity([uint32]$daemonId).Birth -eq $daemonBirth) {Stop-Process -Id $daemonId -Force}
                Wait-MoonCondition -Description 'owned watchdog recovery' -Condition {!(Get-Process -Id $daemonId -ErrorAction SilentlyContinue) -and !(Test-Path -LiteralPath $journal)}
                throw 'Owned daemon needed forced cleanup.'
            }
            Wait-MoonCondition -Description 'original work area restored' -Condition {(Get-WorkAreas (Get-Inventory)) -ceq $workBefore}
            foreach ($id in $originals.Keys) {
                $actual=[MoonElevatedNative]::Frame([long]$id);$original=$originals[$id]
                if ([MoonApplicationNative]::Cloaked([long]$id) -ne 0) {throw 'An owned helper remains cloaked after recovery.'}
                foreach ($edge in @('Left','Top','Right','Bottom')) {if([Math]::Abs($actual.$edge-$original.$edge) -gt 1){throw 'An owned helper did not regain its original frame.'}}
            }
            $restored=$true;$checks.Add('Owned original frames and original desktop work area restored before helper cleanup')
        } catch {if(!$failure){$failure=$_}}
    }
    if ($helper) {
        try {
            foreach ($id in [MoonElevatedNative]::Windows([uint32]$helper.Id)) {[void][MoonElevatedNative]::PostMessage([IntPtr]$id,0x10,[IntPtr]::Zero,[IntPtr]::Zero)}
            if (!$helper.HasExited -and !$helper.WaitForExit(3000)) {$helper.Kill();$helper.WaitForExit()}
        } catch {if(!$failure){$failure=$_}} finally {$helper.Dispose()}
    }
    try {Assert-PreservedApplications;$checks.Add('All pre-existing application HWND/PID/birth identities survived, including separate Explorer windows')} catch {if(!$failure){$failure=$_}}
    if ([MoonElevatedNative]::IsWindow([IntPtr]$previousForeground)) {[void][MoonElevatedNative]::SetForegroundWindow([IntPtr]$previousForeground)}
    $backgroundLifecycle=@($backgroundFrames | ForEach-Object {@{id=$_.window.id;process_id=$_.window.process_id;original_cloak=$_.cloaked;survived=([MoonElevatedNative]::IsWindow([IntPtr][long]$_.window.id) -and [MoonApplicationNative]::ProcessId([long]$_.window.id) -eq $_.window.process_id)}})
    $report=[pscustomobject]@{recorded_at=[DateTime]::UtcNow.ToString('o');passed=(!$failure);error=if($failure){[string]$failure}else{$null};moon_sha256=(Get-FileHash -LiteralPath $MoonPath -Algorithm SHA256).Hash;checks=$checks.ToArray();shell_before=$desktopBefore;shell_after=$desktopAfter;registered_shell_windows=$shellWindows;preserved_application_windows=@($preserved | ForEach-Object {@{id=$_.window.id;process_id=$_.window.process_id;process_birth=$_.process_birth}});cloaked_background_frame_lifecycle=$backgroundLifecycle;daemon_id=$daemonId;daemon_birth=$daemonBirth;layout_before=$stateBefore;layout_after=$stateAfter;work_area_before=$workBefore;work_area_with_moon=$workWithMoon;tray_before=$trayBefore;tray_after=$trayAfter;failure_snapshot=$failureSnapshot;originals_restored=$restored;explorer_restart_attempted=$shellStopped;evidence_directory=$directory;uac_requested=$false}
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
    [IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 100),[Text.UTF8Encoding]::new($false))
}
if ($failure) {throw $failure}
$report | ConvertTo-Json -Depth 100
