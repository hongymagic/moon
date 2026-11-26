[CmdletBinding()]
param([Parameter(Mandatory)][string]$BuildDirectory,[Parameter(Mandatory)][string]$ReportPath,[switch]$AllowBackgroundDesktop)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'install-common.ps1')
. (Join-Path $PSScriptRoot 'test-window-foreground.ps1')
$moon=Join-Path $BuildDirectory 'moon.exe'
$testwin=Join-Path $BuildDirectory 'testwin.exe'
$session=[Diagnostics.Process]::GetCurrentProcess().SessionId
if (@(Get-Process moon,testwin,glazewm,komorebi -ErrorAction SilentlyContinue | Where-Object SessionId -eq $session).Count) {
    throw 'Game integration needs an unused Windows desktop.'
}
if (Test-Path (Join-Path $env:LOCALAPPDATA "moon\journal-v1-session$session.json")) {
    throw 'Restore the previous recovery journal before starting game integration.'
}
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
public static class MoonGameFixture {
    [StructLayout(LayoutKind.Sequential)] struct Keys {public ushort key,scan; public uint flags,time; public UIntPtr extra;}
    [StructLayout(LayoutKind.Sequential)] struct Mouse {public int x,y; public uint data,flags,time; public UIntPtr extra;}
    [StructLayout(LayoutKind.Explicit)] struct Value {[FieldOffset(0)] public Keys keys; [FieldOffset(0)] public Mouse mouse;}
    [StructLayout(LayoutKind.Sequential)] struct Input {public uint type; public Value value;}
    [StructLayout(LayoutKind.Sequential)] struct Rect {public int left,top,right,bottom;}
    [StructLayout(LayoutKind.Sequential)] struct Monitor {public int size;public Rect bounds,work;public uint flags;}
    delegate bool Visit(IntPtr hwnd,IntPtr data);
    [DllImport("user32.dll",SetLastError=true)] static extern uint SendInput(uint count,Input[] input,int size);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern IntPtr FindWindow(string cls,string title);
    [DllImport("user32.dll")] static extern bool EnumWindows(Visit visit,IntPtr data);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,StringBuilder name,int count);
    [DllImport("user32.dll",EntryPoint="GetWindowLongW")] static extern int GetStyle(IntPtr h,int index);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr MonitorFromWindow(IntPtr h,uint flags);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern bool GetMonitorInfo(IntPtr h,ref Monitor monitor);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out Rect rect);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern IntPtr SendMessageTimeout(IntPtr h,uint msg,UIntPtr w,IntPtr l,uint flags,uint timeout,out UIntPtr result);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll",SetLastError=true)] static extern bool RegisterHotKey(IntPtr window,int id,uint modifiers,uint key);
    [DllImport("user32.dll",SetLastError=true)] static extern bool UnregisterHotKey(IntPtr window,int id);
    public static bool EmergencyAvailable() {
        const int id=0x4751;
        if(!RegisterHotKey(IntPtr.Zero,id,7,0x7b)) {
            int error=Marshal.GetLastWin32Error();
            if(error!=1409)throw new Win32Exception(error);
            return false;
        }
        if(!UnregisterHotKey(IntPtr.Zero,id))throw new Win32Exception();
        return true;
    }
    static uint Owner(IntPtr h) {uint pid; GetWindowThreadProcessId(h,out pid);return pid;}
    public static long Window(string title,uint pid) {
        var h=FindWindow("MoonTestWindow",title);
        return Owner(h)==pid?h.ToInt64():0;
    }
    public static string Frame(long window) {
        Rect r;if(!GetWindowRect(new IntPtr(window),out r))throw new Win32Exception();
        return String.Join(",",r.left,r.top,r.right,r.bottom);
    }
    public static int WorkTop(long window) {
        var m=new Monitor();m.size=Marshal.SizeOf(typeof(Monitor));
        if(!GetMonitorInfo(MonitorFromWindow(new IntPtr(window),2),ref m))throw new Win32Exception();
        return m.work.top;
    }
    public static int Bars(long window,uint pid) {
        var monitor=MonitorFromWindow(new IntPtr(window),2);int count=0;
        EnumWindows((h,p)=>{
            if(Owner(h)==pid && IsWindowVisible(h) && MonitorFromWindow(h,0)==monitor && (GetStyle(h,-20)&8)!=0) {
                var name=new StringBuilder(256);GetClassName(h,name,name.Capacity);
                if(name.ToString()=="moon.surface")count++;
            }
            return true;
        },IntPtr.Zero);
        return count;
    }
    public static ulong KeyCounts(long window) {
        UIntPtr counts;
        if(SendMessageTimeout(new IntPtr(window),0x805d,UIntPtr.Zero,IntPtr.Zero,3,3000,out counts)==IntPtr.Zero)throw new Win32Exception();
        return counts.ToUInt64();
    }
    static Input Key(ushort key,bool up) {
        var value=new Value();value.keys=new Keys{key=key,flags=up?2u:0u};
        return new Input{type=1,value=value};
    }
    public static void Chord(long expected,uint pid,ushort key,bool shift) {
        var h=GetForegroundWindow();
        if(h.ToInt64()!=expected || Owner(h)!=pid)throw new InvalidOperationException("Foreground left the fixture before input");
        var input=shift?new[]{Key(0x12,false),Key(0x10,false),Key(key,false),Key(key,true),Key(0x10,true),Key(0x12,true)}
                       :new[]{Key(0x12,false),Key(key,false),Key(key,true),Key(0x12,true)};
        if(SendInput((uint)input.Length,input,Marshal.SizeOf(typeof(Input)))!=input.Length)throw new Win32Exception();
    }
}
'@
[void][MoonGameFixture]::SetThreadDpiAwarenessContext([IntPtr](-4))
function Query-GameFixture([string]$Name) {Invoke-MoonRpc -Method query -Query $Name -DevelopmentPath $moon}
function Wait-GameFocus([long]$Expected,[bool]$Playing) {
    Wait-MoonCondition -Description "foreground $Expected with game input $Playing" -Condition {
        $g=Query-GameFixture games
        [MoonTestForeground]::GetForegroundWindow().ToInt64() -eq $Expected -and $g.observed_foreground -eq $Expected -and
            $(if($Playing) {$g.foreground -eq $game} else {$null -eq $g.foreground})
    }
}
$folder=Join-Path ([IO.Path]::GetTempPath()) ('moon-game-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($folder)
$config=Join-Path $folder 'config.toml'
[IO.File]::WriteAllText($config,@'
config-version=1
cursor-jump=false
workspace-animation=false
[gaming]
automatic=false
pass-every-key=true
[bindings]
"alt+minus"=["pause"]
[[rules]]
game-mode="always"
matches=[{process={equals="testwin"},title={equals="moon test window 1"}}]
'@,[Text.UTF8Encoding]::new($false))
$helper=$null
$owned=0
$checks=[Collections.Generic.List[string]]::new()
$skip=$null
$observed=$null
$errorMessage=$null
function Passed([string]$Name) {$checks.Add($Name);Write-Host "PASS $Name"}
try {
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=$testwin
    $start.Arguments='2'
    $start.UseShellExecute=$false
    $helper=[Diagnostics.Process]::Start($start)
    Wait-MoonCondition -Description 'two owned windows' -Condition {
        [MoonGameFixture]::Window('moon test window 1',$helper.Id) -and [MoonGameFixture]::Window('moon test window 2',$helper.Id)
    }
    $game=[MoonGameFixture]::Window('moon test window 1',$helper.Id)
    $desktop=[MoonGameFixture]::Window('moon test window 2',$helper.Id)
    $frame=[MoonGameFixture]::Frame($game)
    $originalTop=[MoonGameFixture]::WorkTop($game)
    $ready=(Invoke-MoonNative $moon @('start','--background','--only','testwin','--hook','--surfaces','--config',$config)).Output | ConvertFrom-Json
    $owned=[int]$ready.process_id
    if (!(Try-Set-MoonTestForeground $desktop $helper.Id)) {
        if (!$AllowBackgroundDesktop) {throw 'Windows did not grant native foreground to the desktop fixture.'}
        $skip='This runner has no interactive foreground. Native input assertions run on the interactive Windows job.'
        Write-Host "SKIP $skip"
        return
    }
    Grant-MoonTestForeground $helper.Id $owned
    Wait-GameFocus $desktop $false
    Wait-MoonCondition -Description 'desktop bar and workspace' -Condition {
        [MoonGameFixture]::Bars($desktop,$owned) -eq 1 -and
        [MoonGameFixture]::WorkTop($desktop) -gt $originalTop -and
        @(Query-GameFixture windows | Where-Object id -eq $desktop).Count -eq 1
    }
    if (!(Try-Set-MoonTestForeground $game $helper.Id)) {throw 'Windows did not activate the game fixture.'}
    Wait-GameFocus $game $true
    Wait-MoonCondition -Description 'released AppBar reservation and placement' -Condition {
        [MoonGameFixture]::Bars($game,$owned) -eq 0 -and [MoonGameFixture]::WorkTop($game) -eq $originalTop -and
            @(Query-GameFixture windows | Where-Object {$_}).Count -eq 0
    }
    if ([MoonGameFixture]::Frame($game) -ne $frame) {throw 'Moon moved the explicitly matched windowed game.'}
    Wait-MoonCondition -Description 'emergency chord released during play' -Condition {[MoonGameFixture]::EmergencyAvailable()}
    Passed 'Game keeps its original window size and owns its monitor, including the reserved top strip'
    $before=[MoonGameFixture]::KeyCounts($game)
    [MoonGameFixture]::Chord($game,$helper.Id,0xbd,$false)
    Wait-MoonCondition -Description 'both halves of bound Alt+minus reached the game' -Condition {
        [MoonGameFixture]::KeyCounts($game) -eq ($before + [uint64]4294967297)
    }
    if ((Query-GameFixture world).paused) {throw 'Moon consumed the binding during play.'}
    Passed 'Bound key down and up reach the game'
    [MoonGameFixture]::Chord($game,$helper.Id,9,$false)
    Wait-GameFocus $desktop $false
    Wait-MoonCondition -Description 'bar restored over desktop' -Condition {[MoonGameFixture]::Bars($desktop,$owned) -eq 1}
    Wait-MoonCondition -Description 'emergency chord reserved again on the desktop' -Condition {![MoonGameFixture]::EmergencyAvailable()}
    [MoonGameFixture]::Chord($desktop,$helper.Id,9,$false)
    Wait-GameFocus $game $true
    Passed 'Pass every key releases the emergency hotkey during play and restores it on the desktop'
    Passed 'Two separate native Alt+Tab presses take focus from game to desktop and back'
    [MoonGameFixture]::Chord($game,$helper.Id,9,$false)
    # The first desktop chord can arrive before the daemon receives the
    # switcher's foreground event. Wait only for Windows to grant focus.
    Wait-MoonCondition -Description 'desktop receives native foreground' -Condition {
        [MoonTestForeground]::GetForegroundWindow().ToInt64() -eq $desktop
    }
    [MoonGameFixture]::Chord($desktop,$helper.Id,0xbd,$false)
    Wait-MoonCondition -Description 'ordinary Moon shortcut works on the desktop' -Condition {(Query-GameFixture world).paused}
    $null=Invoke-MoonRpc -Method command -Commands @('pause') -DevelopmentPath $moon
    Passed 'The first desktop binding works after the native switch'
    [MoonGameFixture]::Chord($desktop,$helper.Id,9,$false)
    Wait-GameFocus $game $true
    $null=Invoke-MoonRpc -Method command -Commands @('game-mode end') -DevelopmentPath $moon
    Wait-GameFocus $game $false
    Wait-MoonCondition -Description 'one-off end overrides the executable rule' -Condition {
        @(Query-GameFixture windows | Where-Object id -eq $game).Count -eq 1
    }
    $null=Invoke-MoonRpc -Method command -Commands @('restart') -DevelopmentPath $moon
    Wait-MoonCondition -Description 'restarted daemon owns this test configuration' -Condition {
        $current=Query-GameFixture stats
        $current.process_id -ne $owned -and $current.config_path -eq $config -and $current.ready_us -gt 0
    }
    $owned=[int](Query-GameFixture stats).process_id
    Wait-GameFocus $game $false
    if ((Query-GameFixture games).sessions.PSObject.Properties[[string]$game]) {throw 'Restart lost the per-instance override.'}
    $null=Invoke-MoonRpc -Method command -Commands @('game-mode start') -DevelopmentPath $moon
    Wait-GameFocus $game $true
    if ((Query-GameFixture games).sessions.([string]$game).source -ne 'manual') {throw 'The manual session did not target the focused game.'}
    Passed 'Ending and restarting game mode overrides an Always rule for this instance'
    if (!(Try-Set-MoonTestForeground $desktop $helper.Id)) {throw 'The foreground game fixture could not activate its own desktop window.'}
    Wait-GameFocus $desktop $false
    [MoonGameFixture]::Chord($desktop,$helper.Id,9,$true)
    Wait-MoonCondition -Description 'Windows reverse switcher moved focus' -Condition {
        $current=[MoonTestForeground]::GetForegroundWindow().ToInt64()
        $current -ne 0 -and $current -ne $desktop
    }
    Passed 'Alt+Shift+Tab still reaches the Windows reverse switcher from the desktop'
    # Windows may select a third app when switching in reverse. Close only our
    # verified game HWND without trying to take foreground back from that app.
    if (![MoonTestForeground]::Request($game,$helper.Id,0x10,0)) {
        # WM_CLOSE normally returns zero even on success.
        Wait-MoonCondition -Description 'game window closed' -Condition {![MoonGameFixture]::Window('moon test window 1',$helper.Id)}
    }
    Wait-MoonCondition -Description 'game closed and tiling and bar recovered' -Condition {
        $g=Query-GameFixture games
        @($g.sessions.PSObject.Properties).Count -eq 0 -and [MoonGameFixture]::Bars($desktop,$owned) -eq 1 -and
            @(Query-GameFixture windows | Where-Object id -eq $desktop).Count -eq 1
    }
    Passed 'Closing the game restores the desktop workspace, bar and shortcuts'
} catch {
    $errorMessage=$_.Exception.Message
    if ($owned) {
        try {
            $stats=Query-GameFixture stats
            $observed=[ordered]@{
                game=$game;desktop=$desktop;foreground=[MoonTestForeground]::GetForegroundWindow().ToInt64()
                tracked=(Query-GameFixture games);paused=$stats.paused;last_error=$stats.last_error
                bindings=@((Query-GameFixture shortcuts).bindings | Where-Object chord -eq 'alt+minus')
                key_counts=[MoonGameFixture]::KeyCounts($desktop)
                recent_transactions=@($stats.recent_transactions | Select-Object -Last 12)
                commands_dispatched=@($stats.key_to_dispatch_us).Count
            }
            Write-Host ($observed | ConvertTo-Json -Depth 6 -Compress)
        } catch { Write-Host ('Could not sample the failed fixture: '+$_.Exception.Message) }
    }
    throw
} finally {
    try {
        if ($owned) {
            $verified=Query-GameFixture stats
            if ($verified.config_path -ne $config) {throw 'The active daemon changed during game integration.'}
            $owned=[int]$verified.process_id
            $null=Invoke-MoonRpc -Method lifecycle -LifecycleAction exit -DevelopmentPath $moon
            Wait-MoonCondition -Description 'owned daemon exit' -Condition {!(Get-Process -Id $owned -ErrorAction SilentlyContinue)}
        }
    } finally {
        if ($helper) {if (!$helper.HasExited) {$helper.Kill();$helper.WaitForExit()};$helper.Dispose()}
        if ([IO.File]::Exists($config)) {[IO.File]::Delete($config)}
        [IO.Directory]::Delete($folder)
        [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath))) | Out-Null
        [IO.File]::WriteAllText($ReportPath,([ordered]@{passed=@($checks.ToArray());skipped=$skip;error=$errorMessage;observed=$observed} | ConvertTo-Json -Depth 7))
    }
}
