[CmdletBinding()]
param(
    [string]$TestwinPath,
    [string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon\elevated-results.json'),
    [switch]$CompileOnly,
    [switch]$Elevated,
    [string]$OwnerSid,
    [long]$PreviousForeground=0
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class MoonElevatedNative {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
    [StructLayout(LayoutKind.Sequential)] struct Mouse { public int X,Y; public uint Data,Flags,Time; public UIntPtr Extra; }
    [StructLayout(LayoutKind.Sequential)] struct Keyboard { public ushort Vk,Scan; public uint Flags,Time; public UIntPtr Extra; }
    [StructLayout(LayoutKind.Explicit)] struct InputUnion { [FieldOffset(0)] public Mouse Mouse; [FieldOffset(0)] public Keyboard Key; }
    [StructLayout(LayoutKind.Sequential)] struct Input { public uint Type; public InputUnion Value; }
    delegate bool EnumProc(IntPtr window,IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback,IntPtr data);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window,StringBuilder value,int size);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] static extern short GetAsyncKeyState(int key);
    [DllImport("user32.dll",SetLastError=true)] static extern uint SendInput(uint count,Input[] input,int size);
    [DllImport("user32.dll",SetLastError=true)] public static extern bool PostMessage(IntPtr window,uint message,IntPtr w,IntPtr l);
    [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr window,uint attribute,out Rect value,uint size);
    [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,uint process);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr process,uint access,out IntPtr token);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr token,int kind,IntPtr value,uint size,out uint returned);
    [DllImport("advapi32.dll")] static extern IntPtr GetSidSubAuthorityCount(IntPtr sid);
    [DllImport("advapi32.dll")] static extern IntPtr GetSidSubAuthority(IntPtr sid,uint index);
    public static int InputSize() {return Marshal.SizeOf(typeof(Input));}
    public static long[] Windows(uint process) {var result=new List<long>();EnumWindows((h,p)=>{uint id;GetWindowThreadProcessId(h,out id);var name=new StringBuilder(256);GetClassName(h,name,256);if(id==process&&IsWindowVisible(h)&&name.ToString()=="MoonTestWindow")result.Add(h.ToInt64());return true;},IntPtr.Zero);return result.ToArray();}
    public static Rect Frame(long window) {Rect result;Marshal.ThrowExceptionForHR(DwmGetWindowAttribute(new IntPtr(window),9,out result,16));return result;}
    public static uint Integrity(uint process) {var handle=OpenProcess(0x1000,false,process);if(handle==IntPtr.Zero)throw new System.ComponentModel.Win32Exception();try{IntPtr token;if(!OpenProcessToken(handle,8,out token))throw new System.ComponentModel.Win32Exception();try{uint size;GetTokenInformation(token,25,IntPtr.Zero,0,out size);var memory=Marshal.AllocHGlobal((int)size);try{if(!GetTokenInformation(token,25,memory,size,out size))throw new System.ComponentModel.Win32Exception();var sid=Marshal.ReadIntPtr(memory);return unchecked((uint)Marshal.ReadInt32(GetSidSubAuthority(sid,(uint)(Marshal.ReadByte(GetSidSubAuthorityCount(sid))-1))));}finally{Marshal.FreeHGlobal(memory);}}finally{CloseHandle(token);}}finally{CloseHandle(handle);}}
    public static void Chord(long expected,ushort key) {
        if(GetForegroundWindow().ToInt64()!=expected)throw new InvalidOperationException("Foreground is outside the owned test window; input was not sent");
        foreach(int modifier in new int[]{16,17,18,91,92})if((GetAsyncKeyState(modifier)&0x8000)!=0)throw new InvalidOperationException("A modifier is already pressed; input was not sent");
        var input=new Input[4];var keys=new ushort[]{18,key,key,18};for(int i=0;i<4;i++)input[i]=new Input{Type=1,Value=new InputUnion{Key=new Keyboard{Vk=keys[i],Flags=i>=2?2u:0u}}};
        if(SendInput(4,input,Marshal.SizeOf(typeof(Input)))!=4){var release=new Input[]{input[2],input[3]};SendInput(2,release,Marshal.SizeOf(typeof(Input)));throw new System.ComponentModel.Win32Exception();}
    }
}
'@
if ($CompileOnly) {
    if ([MoonElevatedNative]::InputSize() -ne 40) {throw 'Unexpected x64 INPUT structure size.'}
    [pscustomobject]@{native_probe_compiled=$true;input_size=[MoonElevatedNative]::InputSize();integrity_rid=[MoonElevatedNative]::Integrity([uint32]$PID);elevation_requested=$false} | ConvertTo-Json
    return
}
if (!$TestwinPath) {throw 'Provide the testwin.exe built with this release.'}
$TestwinPath=(Resolve-Path -LiteralPath $TestwinPath).ProviderPath
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
if (!$OwnerSid) {$OwnerSid=$sid}
if ($OwnerSid -ne $sid) {throw 'The elevated probe must run under the same Windows account.'}
if (!$PreviousForeground) {$PreviousForeground=[MoonElevatedNative]::GetForegroundWindow().ToInt64()}
if (!(Test-MoonAdministrator)) {
    if ($Elevated) {throw 'Windows did not grant the requested elevated test token.'}
    $words=@('-NoProfile','-ExecutionPolicy','Bypass','-File',$PSCommandPath,'-TestwinPath',$TestwinPath,'-ReportPath',$ReportPath,'-OwnerSid',$OwnerSid,'-PreviousForeground',([string]$PreviousForeground),'-Elevated')
    $child=Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -Verb RunAs -WindowStyle Hidden -ArgumentList (($words | ForEach-Object {ConvertTo-MoonArgument $_}) -join ' ') -PassThru -Wait
    try {if($child.ExitCode -ne 0){throw 'Elevated validation failed; inspect its report or Windows error.'}} finally {$child.Dispose()}
    Get-Content -Raw -LiteralPath $ReportPath
    return
}
if ([MoonInstallNative]::Integrity() -lt 12288) {throw 'This probe needs an actual high-integrity coordinator.'}
$stats=Invoke-MoonRpc query stats
if (!$stats.token.ui_access) {throw 'The daemon does not have UIAccess.'}
$world=Invoke-MoonRpc query world
if ($world.paused -or $world.mode -ne 'default') {throw 'The daemon must be active and in its default keybinding mode.'}
foreach ($binding in @(@('alt+h','focus left'),@('alt+l','focus right'),@('alt+k','focus up'),@('alt+j','focus down'),@('alt+f','toggle-fullscreen'))) {
    $configured=$world.config.bindings.PSObject.Properties[$binding[0]]
    if (!$configured -or @($configured.Value).Count -ne 1 -or $configured.Value[0] -ne $binding[1]) {throw "This probe requires the default $($binding[0]) binding."}
}
$originalTrees=@($world.workspaces | ForEach-Object {$_.tree}) | ConvertTo-Json -Depth 64 -Compress
$helper=$null; $handles=@(); $integrity=$null; $results=[Collections.Generic.List[object]]::new(); $failure=$null
[void][MoonElevatedNative]::SetThreadDpiAwarenessContext([IntPtr](-4))
function OwnedPlacements { @(Invoke-MoonRpc query windows | Where-Object {$handles -contains [long]$_.id}) }
function Test-OwnedFrames {
    $placements=OwnedPlacements
    if ($placements.Count -ne 2) {return $false}
    foreach ($placement in $placements) {
        if (!$placement.visible) {return $false}
        $frame=[MoonElevatedNative]::Frame([long]$placement.id)
        if ([Math]::Abs($frame.Left-$placement.rect.x) -gt 1 -or [Math]::Abs($frame.Top-$placement.rect.y) -gt 1 -or [Math]::Abs(($frame.Right-$frame.Left)-$placement.rect.width) -gt 1 -or [Math]::Abs(($frame.Bottom-$frame.Top)-$placement.rect.height) -gt 1) {return $false}
    }
    return $true
}
try {
    $start=[Diagnostics.ProcessStartInfo]::new(); $start.FileName=$TestwinPath; $start.Arguments='2'; $start.UseShellExecute=$false; $start.CreateNoWindow=$true
    $helper=[Diagnostics.Process]::Start($start)
    Wait-MoonCondition -Description 'two owned elevated test windows' -Condition {@([MoonElevatedNative]::Windows([uint32]$helper.Id)).Count -eq 2}
    $handles=@([MoonElevatedNative]::Windows([uint32]$helper.Id))
    $integrity=[MoonElevatedNative]::Integrity([uint32]$helper.Id)
    if ($integrity -lt 12288) {throw 'The helper did not inherit high integrity.'}
    Wait-MoonCondition -Description 'UIAccess daemon to tile the elevated windows' -Condition {Test-OwnedFrames}
    $placements=OwnedPlacements
    if (@($placements | Where-Object floating).Count) {throw 'A custom rule floated a test window; this probe needs two tiled helpers.'}
    $horizontal=[Math]::Abs($placements[0].rect.x-$placements[1].rect.x) -gt [Math]::Abs($placements[0].rect.y-$placements[1].rect.y)
    $ordered=if($horizontal){@($placements | Sort-Object {$_.rect.x})}else{@($placements | Sort-Object {$_.rect.y})}
    $first=[long]$ordered[0].id; $second=[long]$ordered[1].id
    [void][MoonElevatedNative]::SetForegroundWindow([IntPtr]$first)
    Wait-MoonCondition -Description 'owned elevated foreground before key injection' -Condition {[MoonElevatedNative]::GetForegroundWindow().ToInt64() -eq $first -and (Invoke-MoonRpc query world).focused -eq $first}
    $focusPass=0
    for($trial=0;$trial -lt 20;$trial++) {
        $from=if($trial%2 -eq 0){$first}else{$second}; $to=if($trial%2 -eq 0){$second}else{$first}
        $key=if($horizontal){if($trial%2 -eq 0){76}else{72}}else{if($trial%2 -eq 0){74}else{75}}
        [MoonElevatedNative]::Chord($from,[uint16]$key)
        Wait-MoonCondition -Description 'hook-driven focus between elevated owned windows' -Condition {[MoonElevatedNative]::GetForegroundWindow().ToInt64() -eq $to -and (Invoke-MoonRpc query world).focused -eq $to} -Seconds 3
        $focusPass++
    }
    $results.Add([pscustomobject]@{check='elevated foreground hotkey focus';passed=$true;successes=$focusPass;trials=20})
    $initial=([MoonElevatedNative]::Frame($first) | ConvertTo-Json -Compress)
    [MoonElevatedNative]::Chord($first,70)
    Wait-MoonCondition -Description 'hook-driven elevated fullscreen placement' -Condition {@(OwnedPlacements | Where-Object {$_.id -eq $first -and $_.fullscreen}).Count -eq 1 -and (Test-OwnedFrames)}
    if (([MoonElevatedNative]::Frame($first) | ConvertTo-Json -Compress) -eq $initial) {throw 'Fullscreen did not change the elevated native frame.'}
    [MoonElevatedNative]::Chord($first,70)
    Wait-MoonCondition -Description 'elevated helper returns to exact tiled placement' -Condition {@(OwnedPlacements | Where-Object fullscreen).Count -eq 0 -and (Test-OwnedFrames) -and ([MoonElevatedNative]::Frame($first) | ConvertTo-Json -Compress) -eq $initial}
    $results.Add([pscustomobject]@{check='elevated foreground hotkey fullscreen and exact placement restoration';passed=$true})
} catch {$failure=$_}
finally {
    if ($helper) {
        foreach($handle in $handles) {[void][MoonElevatedNative]::PostMessage([IntPtr]$handle,0x10,[IntPtr]::Zero,[IntPtr]::Zero)}
        if (!$helper.WaitForExit(3000)) {$helper.Kill(); $helper.WaitForExit()}
        $helper.Dispose()
        Wait-MoonCondition -Description 'owned helper removal from the layout' -Condition {@(OwnedPlacements).Count -eq 0}
        $afterTrees=@((Invoke-MoonRpc query world).workspaces | ForEach-Object {$_.tree}) | ConvertTo-Json -Depth 64 -Compress
        if ($afterTrees -ne $originalTrees -and !$failure) {$failure='Closing the owned probes did not restore the original workspace trees.'}
    }
    if ([MoonElevatedNative]::IsWindow([IntPtr]$PreviousForeground)) {
        [void][MoonElevatedNative]::SetForegroundWindow([IntPtr]$PreviousForeground)
        try {Wait-MoonCondition -Description 'previous foreground restoration' -Condition {[MoonElevatedNative]::GetForegroundWindow().ToInt64() -eq $PreviousForeground} -Seconds 3}
        catch {if(!$failure){$failure=$_}}
    }
    $report=[pscustomobject]@{recorded_at=[DateTime]::UtcNow.ToString('o');coordinator_integrity=[MoonInstallNative]::Integrity();daemon_token=$stats.token;helper_integrity=if($helper){$integrity}else{$null};checks=$results.ToArray();passed=(!$failure);error=if($failure){[string]$failure}else{$null};input_scope='known Alt chords only while an owned elevated helper is foreground'}
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
    Write-MoonJsonFile $ReportPath $report
}
if ($failure) {throw $failure}
$report | ConvertTo-Json -Depth 8
