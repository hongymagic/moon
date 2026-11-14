[CmdletBinding()]
param(
    [string]$BuildDirectory,
    [string]$ExtensionPath,
    [string]$MoonPath = (Join-Path $env:LOCALAPPDATA 'moon-dev\moon.exe'),
    [string]$TestwinPath = (Join-Path $env:LOCALAPPDATA 'moon-dev\testwin.exe'),
    [string]$ReportPath = (Join-Path $env:LOCALAPPDATA 'moon-dev\integration-results.json'),
    [switch]$GuardOnly,
    [switch]$StartupAssignmentOnly,
    [switch]$BindingEffectsOnly
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (@($GuardOnly,$StartupAssignmentOnly,$BindingEffectsOnly | Where-Object { $_ }).Count -gt 1) { throw 'Choose only one focused integration mode.' }
if (!$ExtensionPath) { $ExtensionPath = Join-Path $PSScriptRoot '..\examples\workspace-state.ps1' }
if ($BuildDirectory) {
    $MoonPath = Join-Path $BuildDirectory 'moon.exe'
    $TestwinPath = Join-Path $BuildDirectory 'testwin.exe'
    if (!$PSBoundParameters.ContainsKey('ReportPath')) { $ReportPath = Join-Path $PSScriptRoot '..\target\integration-results.json' }
}
$MoonPath = (Resolve-Path $MoonPath).Path
$TestwinPath = (Resolve-Path $TestwinPath).Path
$testDirectory = Join-Path ([IO.Path]::GetTempPath()) ('moon-integration-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($testDirectory)
$configFile = Join-Path $testDirectory 'config with spaces.toml'
$utf8 = New-Object Text.UTF8Encoding($false)
[IO.File]::WriteAllText($configFile, "config-version = 1`n", $utf8)
if ($StartupAssignmentOnly) {
    [IO.File]::WriteAllText($configFile, @'
config-version = 1
[[rules]]
commands = ["move-to-workspace 10"]
matches = [{process={equals="testwin"},title={regex="^moon test window [12]$"}}]
[[rules]]
commands = ["move-to-workspace 1"]
matches = [{process={equals="testwin"},title={equals="moon test window 3"}}]
[[rules]]
commands = ["move-to-workspace 2"]
matches = [{process={equals="testwin"},title={equals="moon test window 4"}}]
'@, $utf8)
}
$buildEvidence = [pscustomobject]@{
    started_at = [DateTime]::UtcNow.ToString('o')
    os = [Environment]::OSVersion.VersionString
    moon = [pscustomobject]@{ path=$MoonPath; sha256=(Get-FileHash -LiteralPath $MoonPath -Algorithm SHA256).Hash }
    testwin = [pscustomobject]@{ path=$TestwinPath; sha256=(Get-FileHash -LiteralPath $TestwinPath -Algorithm SHA256).Hash }
    scope = if ($BindingEffectsOnly) { 'Owned four-window command effects via CLI/IPC; fixture config, hooks and surfaces disabled; not no-config or all-binding end-to-end acceptance.' } else { 'Existing lifecycle or focused integration scenario.' }
}
[IO.File]::WriteAllText((Join-Path $testDirectory 'build.json'), ($buildEvidence | ConvertTo-Json -Depth 4), $utf8)
$journalFile = Join-Path $env:LOCALAPPDATA ('moon\journal-v1-session' + [Diagnostics.Process]::GetCurrentProcess().SessionId + '.json')
$results = New-Object 'System.Collections.Generic.List[object]'
$helper = $null
$daemon = $null
$ownedDaemon = $null
$originals = @{}

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class MoonIntegrationNative {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    public delegate bool EnumProc(IntPtr hwnd, IntPtr param);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr param);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder text, int size);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint process);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll",SetLastError=true)] static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr hwnd, int command);
    [DllImport("user32.dll",EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetWindowStyle(IntPtr hwnd, int index);
    [DllImport("user32.dll",EntryPoint="SetWindowLongPtrW")] public static extern IntPtr SetWindowStyle(IntPtr hwnd, int index, IntPtr value);
    [DllImport("user32.dll",SetLastError=true)] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr hwnd, uint attribute, out Rect rect, uint size);
    [DllImport("dwmapi.dll", EntryPoint="DwmGetWindowAttribute")] static extern int DwmGetCloaked(IntPtr hwnd, uint attribute, out uint value, uint size);
    public static long[] Windows(uint process) { var handles = new List<long>(); EnumWindows((h,p) => { uint id; GetWindowThreadProcessId(h,out id); var name=new StringBuilder(256); GetClassName(h,name,256); if(id == process && IsWindowVisible(h) && name.ToString()=="MoonTestWindow") handles.Add(h.ToInt64()); return true; },IntPtr.Zero); return handles.ToArray(); }
    public static Rect Frame(long hwnd) { Rect value; int hr=DwmGetWindowAttribute(new IntPtr(hwnd),9,out value,16); Marshal.ThrowExceptionForHR(hr); return value; }
    public static uint Cloaked(long hwnd) { uint value; int hr=DwmGetCloaked(new IntPtr(hwnd),14,out value,4); Marshal.ThrowExceptionForHR(hr); return value; }
    public static uint Owner(long hwnd) { uint process; GetWindowThreadProcessId(new IntPtr(hwnd),out process); return process; }
    public static Rect Outer(long hwnd) { Rect value; if(!GetWindowRect(new IntPtr(hwnd),out value))throw new System.ComponentModel.Win32Exception();return value; }
}
'@
[void][MoonIntegrationNative]::SetThreadDpiAwarenessContext([IntPtr](-4))

function Invoke-Moon {
    [CmdletBinding(PositionalBinding=$false)]
    param([Parameter(ValueFromRemainingArguments=$true)][string[]]$Words, [int]$TimeoutSeconds=8)
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $MoonPath
    # ProcessStartInfo.ArgumentList is unavailable in Windows PowerShell 5.1.
    $quoted = foreach ($word in $Words) {
        if ($word -notmatch '["\s]' -and $word.Length -gt 0) { $word; continue }
        $escaped = [regex]::Replace($word, '(\\*)"', '$1$1\"')
        '"' + [regex]::Replace($escaped, '(\\+)$', '$1$1') + '"'
    }
    $start.Arguments = $quoted -join ' '
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $client = New-Object Diagnostics.Process
    $client.StartInfo = $start
    try {
        [void]$client.Start()
        $stdout = $client.StandardOutput.ReadToEndAsync()
        $stderr = $client.StandardError.ReadToEndAsync()
        if (!$client.WaitForExit($TimeoutSeconds*1000)) { $client.Kill(); throw 'moon client timed out' }
        if (!$stdout.Wait(2000) -or !$stderr.Wait(2000)) {
            if ($Words[0] -eq 'start') {
                try {
                    $running = Invoke-Moon query stats
                    if ([string]::Equals($running.executable, $MoonPath, [StringComparison]::OrdinalIgnoreCase)) { $script:ownedDaemon = [int]$running.process_id }
                } catch { }
            }
            throw 'An exited moon client left a redirected output stream open in a descendant process'
        }
        $text = $stdout.Result
        if ($client.ExitCode -ne 0) { throw "moon $($Words -join ' '): $($stderr.Result)" }
    } finally { $client.Dispose() }
    if ($text.Trim().StartsWith('{') -or $text.Trim().StartsWith('[')) { $parsed = $text | ConvertFrom-Json; foreach ($item in $parsed) { Write-Output $item }; return }
    return $text
}
function Wait-Until {
    param([scriptblock]$Condition, [string]$Description, [int]$Seconds=15)
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    $lastError = ''
    do {
        try { if (& $Condition) { return } } catch { $lastError = $_.Exception.Message }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out: $Description. $lastError"
}
function Record-Pass([string]$Name) {
    $results.Add([pscustomobject]@{ name=$Name; passed=$true; at=[DateTime]::UtcNow.ToString('o') })
    Write-Host "PASS $Name"
}
function Assert-Placements {
    Wait-Until -Description 'native frames match the committed placements' -Condition {
        $placements = @(Invoke-Moon query windows)
        if ($placements.Count -eq 0) { return $false }
        foreach ($placement in $placements) {
            $native = [MoonIntegrationNative]::Frame([long]$placement.id)
            $cloak = [MoonIntegrationNative]::Cloaked([long]$placement.id)
            if ($placement.visible) {
                if ($cloak -ne 0) { return $false }
                $actual = @($native.Left, $native.Top, ($native.Right-$native.Left), ($native.Bottom-$native.Top))
                $desired = @($placement.rect.x, $placement.rect.y, $placement.rect.width, $placement.rect.height)
                for ($index=0; $index -lt 4; $index++) { if ([Math]::Abs($actual[$index]-$desired[$index]) -gt 1) { return $false } }
            } elseif ($cloak -eq 0 -and $native.Left -gt -30000) { return $false }
        }
        return $true
    }
}
function State-Signature {
    $state = Invoke-Moon query world
    [pscustomobject]@{ active=$state.active; focused=$state.focused; workspaces=$state.workspaces; placements=@(Invoke-Moon query windows | Sort-Object id) } | ConvertTo-Json -Depth 64 -Compress
}
function Test-OriginalsRestored {
    foreach ($handle in [MoonIntegrationNative]::Windows([uint32]$helper.Id)) {
        if ([MoonIntegrationNative]::Cloaked($handle) -ne 0) { return $false }
        $frame = [MoonIntegrationNative]::Frame($handle)
        $original = $originals[[string]$handle]
        foreach ($edge in @('Left','Top','Right','Bottom')) { if ([Math]::Abs($frame.$edge-$original.$edge) -gt 1) { return $false } }
    }
    return !(Test-Path $journalFile)
}
function Read-RecoveryBytes {
    # A background writer atomically replaces the file. Keep delete-sharing on
    # this observer handle so reading a checkpoint cannot interrupt persistence.
    $stream = [IO.File]::Open($journalFile, [IO.FileMode]::Open, [IO.FileAccess]::Read, ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    $memory = New-Object IO.MemoryStream
    try { $stream.CopyTo($memory); return ,$memory.ToArray() }
    finally { $memory.Dispose(); $stream.Dispose() }
}
function Read-Recovery {
    $bytes = Read-RecoveryBytes
    return [Text.Encoding]::UTF8.GetString($bytes) | ConvertFrom-Json
}
function Invoke-PublicRequest([hashtable]$Request) {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try { $pipeName = [MoonExtensionIdentity]::PipeName($identity.Token) } finally { $identity.Dispose() }
    $pipe = [IO.Pipes.NamedPipeClientStream]::new('.', $pipeName, [IO.Pipes.PipeDirection]::InOut, [IO.Pipes.PipeOptions]::Asynchronous, [Security.Principal.TokenImpersonationLevel]::Identification)
    try {
        $pipe.Connect(5000)
        if ([MoonExtensionIdentity]::ServerImage($pipe.SafePipeHandle) -ine [IO.Path]::GetFullPath($MoonPath)) {throw 'The pipe does not belong to the owned Moon fixture.'}
        $reader = New-Object IO.StreamReader($pipe, $utf8, $false, 4096, $true)
        $writer = New-Object IO.StreamWriter($pipe, $utf8, 4096, $true)
        try {
            $writer.NewLine = "`n"
            $writer.AutoFlush = $true
            $writer.WriteLine(($Request | ConvertTo-Json -Compress))
            $response = $reader.ReadLineAsync()
            if (!$response.Wait(5000)) { throw 'Public IPC request timed out' }
            if ($null -eq $response.Result) { throw 'Public IPC response ended before a complete frame' }
            return $response.Result | ConvertFrom-Json
        } finally { $reader.Dispose(); try { $writer.Dispose() } catch [IO.IOException] { } }
    } finally { $pipe.Dispose() }
}

function Assert-EffectIdentity([long]$Id) {
    $helper.Refresh()
    if ($helper.HasExited -or !$originals.ContainsKey([string]$Id) -or ![MoonIntegrationNative]::IsWindow([IntPtr]$Id) -or [MoonIntegrationNative]::Owner($Id) -ne $helper.Id) { throw 'An owned binding-effect window identity changed.' }
}
function Focus-EffectWindow([long]$Id) {
    Assert-EffectIdentity $Id
    $placement = @(Invoke-Moon query windows | Where-Object { [long]$_.id -eq $Id })
    if ($placement.Count -ne 1 -or !$placement[0].visible) { throw 'Refused to focus an inactive binding-effect window.' }
    if (![MoonIntegrationNative]::SetForegroundWindow([IntPtr]$Id)) { throw 'Windows refused owned binding-effect foreground.' }
    Wait-Until -Seconds 5 -Description 'exact owned native and logical focus' -Condition {
        [MoonIntegrationNative]::GetForegroundWindow().ToInt64() -eq $Id -and [long](Invoke-Moon query world).focused -eq $Id
    }
}
function Test-EffectFrame($Actual,$Expected) {
    foreach ($edge in @('Left','Top','Right','Bottom')) { if ([Math]::Abs($Actual.$edge-$Expected.$edge) -gt 1) { return $false } }
    return $true
}
function Assert-EffectWorkspace([string]$Name,[hashtable]$Members,[long]$ExpectedFocus=0) {
    Wait-Until -Seconds 5 -Description "workspace $Name exact memberships, visibility and focus" -Condition {
        $world = Invoke-Moon query world
        $active = $world.active.PSObject.Properties[[string]$world.active_monitor].Value
        if ($world.workspaces[$active].name -ne $Name -or @($world.windows.PSObject.Properties).Count -ne 4) { return $false }
        $placements = @(Invoke-Moon query windows)
        if ($placements.Count -ne 4) { return $false }
        foreach ($entry in $world.windows.PSObject.Properties) {
            if (!$Members.ContainsKey($entry.Name) -or $world.workspaces[$entry.Value.workspace].name -ne $Members[$entry.Name]) { return $false }
            Assert-EffectIdentity ([long]$entry.Name)
            $placement = @($placements | Where-Object { [string]$_.id -eq $entry.Name })
            if ($placement.Count -ne 1 -or $placement[0].visible -ne ($Members[$entry.Name] -eq $Name)) { return $false }
        }
        if ($ExpectedFocus -and [long]$world.focused -ne $ExpectedFocus) { return $false }
        return !$world.focused -or [MoonIntegrationNative]::GetForegroundWindow().ToInt64() -eq [long]$world.focused
    }
    Assert-Placements
}
function Save-EffectStep([string]$Name) {
    $world = Invoke-Moon query world
    $native = foreach ($key in $originals.Keys) {
        Assert-EffectIdentity ([long]$key)
        [pscustomobject]@{id=[long]$key;frame=[MoonIntegrationNative]::Frame([long]$key);cloak=[MoonIntegrationNative]::Cloaked([long]$key)}
    }
    $effectSteps.Add([pscustomobject]@{name=$Name;active=$world.active;focused=$world.focused;paused=$world.paused;workspaces=$world.workspaces;native=@($native)})
    [IO.File]::WriteAllText((Join-Path $testDirectory 'binding-effects.json'), ($effectSteps.ToArray() | ConvertTo-Json -Depth 64), $utf8)
}
function Invoke-BindingEffects {
    $effectSteps = New-Object 'System.Collections.Generic.List[object]'
    $world = Invoke-Moon query world
    if (@($world.monitors).Count -ne 1 -or @($world.workspaces).Count -ne 10) { throw 'The focused binding-effects fixture requires one monitor and the ten default workspaces; monitor scenarios are separate.' }
    for ($index=0; $index -lt 10; $index++) { if ($world.workspaces[$index].name -ne [string]($index+1)) { throw 'The numbered workspace defaults changed.' } }
    $ids = @($originals.Keys | ForEach-Object { [long]$_ } | Sort-Object)
    if ($ids.Count -ne 4) { throw 'Expected exactly four retained binding-effect HWNDs.' }
    $anchor=$ids[0];$inserted=$ids[1];$third=$ids[2]
    $members=@{};foreach ($id in $ids) { $members[[string]$id]='1' }
    $null=Invoke-Moon workspace 1
    Focus-EffectWindow $anchor
    Assert-EffectWorkspace '1' $members
    # Reuse owned windows as new insertions; never create or manipulate a user app.
    foreach ($id in $ids[1..3]) {
        Focus-EffectWindow $id;$null=Invoke-Moon move-to-workspace 10
        $members[[string]$id]='10';Assert-EffectWorkspace '1' $members
    }
    foreach ($axis in @('vertical','horizontal')) {
        Focus-EffectWindow $anchor
        $opposite=if ($axis -eq 'vertical') { 'horizontal' } else { 'vertical' }
        $null=Invoke-Moon -Words @('direction',$opposite)
        $before=[MoonIntegrationNative]::Frame($anchor)
        $null=Invoke-Moon toggle-direction
        if ((Invoke-Moon query world).workspaces[0].tree.axis -ne $axis) { throw 'Toggle direction did not change the single-window insertion axis.' }
        Assert-Placements
        if (!(Test-EffectFrame ([MoonIntegrationNative]::Frame($anchor)) $before)) { throw 'Changing an insertion hint unexpectedly resized a single window.' }
        $null=Invoke-Moon workspace 10;Assert-EffectWorkspace '10' $members
        Focus-EffectWindow $inserted;$null=Invoke-Moon move-to-workspace 1 --follow
        $members[[string]$inserted]='1';Assert-EffectWorkspace '1' $members $inserted
        $first=[MoonIntegrationNative]::Frame($anchor);$second=[MoonIntegrationNative]::Frame($inserted)
        if ($axis -eq 'vertical') {
            $arranged=$first.Bottom -lt $second.Top -and $first.Left -eq $second.Left -and $first.Right -eq $second.Right
            $forward='down';$back='up'
        } else {
            $arranged=$first.Right -lt $second.Left -and $first.Top -eq $second.Top -and $first.Bottom -eq $second.Bottom
            $forward='right';$back='left'
        }
        if (!$arranged -or (Test-EffectFrame $first $before)) { throw "New insertion did not produce the independently expected $axis split." }
        Save-EffectStep "$axis split after toggle and owned insertion"
        Focus-EffectWindow $anchor
        foreach ($direction in @($forward,$back)) {
            $target=if ($direction -eq $forward) { $inserted } else { $anchor }
            $null=Invoke-Moon -Words @('focus',$direction)
            Assert-EffectWorkspace '1' $members $target
            Save-EffectStep "focus $direction activates the exact neighbor"
        }
        $null=Invoke-Moon -Words @('move',$forward)
        Assert-EffectWorkspace '1' $members $anchor
        if (!(Test-EffectFrame ([MoonIntegrationNative]::Frame($anchor)) $second) -or !(Test-EffectFrame ([MoonIntegrationNative]::Frame($inserted)) $first)) { throw "Move $forward did not exchange the two native slots." }
        Save-EffectStep "move $forward exchanges native slots"
        $null=Invoke-Moon -Words @('move',$back)
        Assert-EffectWorkspace '1' $members $anchor
        if (!(Test-EffectFrame ([MoonIntegrationNative]::Frame($anchor)) $first) -or !(Test-EffectFrame ([MoonIntegrationNative]::Frame($inserted)) $second)) { throw "Move $back did not restore both native slots." }
        Save-EffectStep "move $back restores native slots"
        Record-Pass "$axis insertion, exact neighbor focus, and both directional reorders"
        if ($axis -eq 'vertical') {
            Focus-EffectWindow $inserted;$null=Invoke-Moon move-to-workspace 10
            $members[[string]$inserted]='10';Assert-EffectWorkspace '1' $members
        }
    }
    # With siblings, toggle-direction is deferred until insertion, not a reflow.
    Focus-EffectWindow $anchor
    $left=[MoonIntegrationNative]::Frame($anchor);$right=[MoonIntegrationNative]::Frame($inserted)
    $null=Invoke-Moon toggle-direction;Assert-Placements
    if (!(Test-EffectFrame ([MoonIntegrationNative]::Frame($anchor)) $left) -or !(Test-EffectFrame ([MoonIntegrationNative]::Frame($inserted)) $right)) { throw 'A deferred split hint moved existing siblings before insertion.' }
    $null=Invoke-Moon workspace 10;Assert-EffectWorkspace '10' $members
    Focus-EffectWindow $third;$null=Invoke-Moon move-to-workspace 1 --follow
    $members[[string]$third]='1';Assert-EffectWorkspace '1' $members $third
    $top=[MoonIntegrationNative]::Frame($anchor);$bottom=[MoonIntegrationNative]::Frame($third)
    if ($top.Bottom -ge $bottom.Top -or $top.Left -ne $left.Left -or $top.Right -ne $left.Right -or $bottom.Left -ne $left.Left -or $bottom.Right -ne $left.Right -or $top.Top -ne $left.Top -or $bottom.Bottom -ne $left.Bottom -or !(Test-EffectFrame ([MoonIntegrationNative]::Frame($inserted)) $right)) { throw 'Deferred insertion did not split only the anchored native slot vertically.' }
    Save-EffectStep 'deferred split changes only the anchored slot on insertion'
    Record-Pass 'toggle direction preserves existing siblings and affects the next owned insertion'

    Focus-EffectWindow $anchor
    $tiled=[MoonIntegrationNative]::Frame($anchor);$state=Invoke-Moon query world
    $bounds=$state.monitors[0].bounds
    $full=[pscustomobject]@{Left=$bounds.x;Top=$bounds.y;Right=($bounds.x+$bounds.width);Bottom=($bounds.y+$bounds.height)}
    $null=Invoke-Moon toggle-fullscreen
    Wait-Until -Seconds 5 -Description 'manual fullscreen flag and full physical monitor bounds' -Condition {
        $w=Invoke-Moon query world
        $w.windows.PSObject.Properties[[string]$anchor].Value.fullscreen -and (Test-EffectFrame ([MoonIntegrationNative]::Frame($anchor)) $full)
    }
    if (Test-EffectFrame $tiled $full) { throw 'Fullscreen fixture did not start with a smaller tiled frame.' }
    Assert-Placements;Save-EffectStep 'manual fullscreen fills physical monitor bounds'
    $null=Invoke-Moon toggle-fullscreen
    Wait-Until -Seconds 5 -Description 'manual fullscreen clears and restores the original tile' -Condition {
        $w=Invoke-Moon query world
        !$w.windows.PSObject.Properties[[string]$anchor].Value.fullscreen -and (Test-EffectFrame ([MoonIntegrationNative]::Frame($anchor)) $tiled)
    }
    Assert-Placements;Record-Pass 'manual fullscreen changes native bounds and restores the exact tile'

    $treeBefore=(Invoke-Moon query world).workspaces | ConvertTo-Json -Depth 64 -Compress
    $null=Invoke-Moon pause
    if (!(Invoke-Moon query world).paused -or @(Invoke-Moon query windows).Count -ne 0) { throw 'Pause did not disable placement effects.' }
    Assert-EffectIdentity $anchor
    $outer=[MoonIntegrationNative]::Outer($anchor)
    $offset=[pscustomobject]@{Left=($tiled.Left+17);Top=($tiled.Top+11);Right=($tiled.Right+17);Bottom=($tiled.Bottom+11)}
    if (![MoonIntegrationNative]::SetWindowPos([IntPtr]$anchor,[IntPtr]::Zero,($outer.Left+17),($outer.Top+11),0,0,0x15)) { throw 'Could not move the owned paused fixture.' }
    Wait-Until -Seconds 5 -Description 'owned paused window accepts manual placement' -Condition { Test-EffectFrame ([MoonIntegrationNative]::Frame($anchor)) $offset }
    Start-Sleep -Milliseconds 1250
    if (!(Invoke-Moon query world).paused -or !(Test-EffectFrame ([MoonIntegrationNative]::Frame($anchor)) $offset) -or (((Invoke-Moon query world).workspaces | ConvertTo-Json -Depth 64 -Compress) -ne $treeBefore)) { throw 'Paused maintenance changed the owned native frame or workspace trees.' }
    Save-EffectStep 'pause preserves a manual owned-window move across maintenance'
    $null=Invoke-Moon pause;Assert-EffectWorkspace '1' $members $anchor
    if ((Invoke-Moon query world).paused -or !(Test-EffectFrame ([MoonIntegrationNative]::Frame($anchor)) $tiled) -or (((Invoke-Moon query world).workspaces | ConvertTo-Json -Depth 64 -Compress) -ne $treeBefore)) { throw 'Resume did not restore the exact tiled frame and persistent trees.' }
    Save-EffectStep 'resume reapplies the saved native tile'
    Record-Pass 'pause leaves manual placement alone and resume restores tiling'

    foreach ($number in @(2,3,4,5,6,7,8,9,10,1)) {
        $name=[string]$number
        $null=Invoke-Moon -Words @('move-to-workspace',$name,'--follow')
        $members[[string]$anchor]=$name
        Assert-EffectWorkspace $name $members $anchor
        Save-EffectStep "move-and-follow $name exact membership, focus and visibility"
    }
    Record-Pass 'move-and-follow visits every numbered destination with exact native visibility'
    $null=Invoke-Moon workspace 10;Assert-EffectWorkspace '10' $members
    foreach ($number in 1..10) {
        $name=[string]$number;$null=Invoke-Moon -Words @('workspace',$name)
        Assert-EffectWorkspace $name $members
        Save-EffectStep "workspace $name exact active workspace and unchanged memberships"
    }
    Record-Pass 'all ten workspace selections preserve membership and show only their owned windows'
    $null=Invoke-Moon exit
    Wait-Until -Description 'all four original binding-effect windows survive and regain their frames' -Condition {
        $helper.Refresh();if ($helper.HasExited) { return $false }
        $survivors=@([MoonIntegrationNative]::Windows([uint32]$helper.Id))
        if ($survivors.Count -ne 4 -or @($survivors | Where-Object { !$originals.ContainsKey([string]$_) }).Count) { return $false }
        return Test-OriginalsRestored
    }
    Record-Pass 'binding-effect fixture restores original frames on exit'
}

try {
    # Existing daemons and helper windows belong to the user or another test run.
    $existing = @(Get-Process -Name @('moon','testwin') -ErrorAction SilentlyContinue)
    if ($existing.Count -gt 0) { throw 'Close existing moon/testwin test instances before running integration tests.' }
    if ($BindingEffectsOnly -and @(Get-Process -Name @('glazewm','komorebi','whim','zebar','tacky','tacky-borders') -ErrorAction SilentlyContinue).Count) { throw 'The binding-effects fixture requires Moon to be the sole manager and bar/border owner.' }
    if (Test-Path $journalFile) { throw 'A previous recovery journal exists; restore it before running the isolated integration suite.' }
    if (Get-Process -Name glazewm -ErrorAction SilentlyContinue) {
        $glazeConfig = Join-Path $env:USERPROFILE '.glzr\glazewm\config.yaml'
        if (!(Test-Path $glazeConfig) -or !(Select-String -Path $glazeConfig -Pattern 'testwin' -Quiet)) { throw 'GlazeWM must ignore testwin before running the isolated integration suite.' }
    }
    $helperStart = New-Object Diagnostics.ProcessStartInfo
    $helperStart.FileName = $TestwinPath
    $helperStart.Arguments = '4'
    $helperStart.UseShellExecute = $false
    $helperStart.CreateNoWindow = $true
    $helper = [Diagnostics.Process]::Start($helperStart)
    Wait-Until -Description 'four helper windows open' -Condition { @([MoonIntegrationNative]::Windows([uint32]$helper.Id)).Count -eq 4 }
    foreach ($handle in [MoonIntegrationNative]::Windows([uint32]$helper.Id)) { $originals[[string]$handle] = [MoonIntegrationNative]::Frame($handle) }
    $background = Invoke-Moon -TimeoutSeconds 20 start --background --only testwin --no-hook --config $configFile
    $ownedDaemon = [int]$background.process_id
    $daemon = [Diagnostics.Process]::GetProcessById($ownedDaemon)
    if (!$background.ready -or $daemon.HasExited) { throw 'Background launcher returned before its daemon was ready' }
    [IO.File]::WriteAllText((Join-Path $testDirectory 'background-start.json'), ($background | ConvertTo-Json), $utf8)
    Record-Pass 'background launcher exits with a ready independent daemon'
    Wait-Until -Description 'daemon adopts four test windows' -Condition { (Invoke-Moon query stats).managed_windows -eq 4 }
    if ($BindingEffectsOnly) { Invoke-BindingEffects;return }
    if ($StartupAssignmentOnly) {
        $state = Invoke-Moon query world
        $stats = Invoke-Moon query stats
        if (![string]::Equals([IO.Path]::GetFullPath($stats.config_path), [IO.Path]::GetFullPath($configFile), [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Startup loaded a different configuration file'
        }
        if ($stats.last_error) { throw "Startup reported an error: $($stats.last_error)" }
        $expected = @{'moon test window 1'='10';'moon test window 2'='10';'moon test window 3'='1';'moon test window 4'='2'}
        $seen = @{}
        foreach ($entry in $state.windows.PSObject.Properties) {
            $window = $entry.Value
            $name = [string]$window.info.title
            if (!$expected.ContainsKey($name) -or $seen.ContainsKey($name) -or $window.info.process_id -ne $helper.Id) {
                throw 'Startup adopted an unexpected or duplicate window identity'
            }
            $seen[$name] = [long]$window.info.id
            if ($state.workspaces[$window.workspace].name -ne $expected[$name] -or $window.floating -or $window.suspended) {
                throw "Pre-existing helper $name was not tiled in its configured workspace"
            }
        }
        if ($seen.Count -ne 4) { throw 'Startup did not adopt every pre-existing helper' }
        [IO.File]::WriteAllText((Join-Path $testDirectory 'startup-assignments.json'), ($state | ConvertTo-Json -Depth 64), $utf8)
        Record-Pass 'pre-existing windows receive their individual workspace rules at startup'
        foreach ($name in @('1','2','10')) {
            $null = Invoke-Moon -Words @('workspace', $name)
            Assert-Placements
            $visible = @((Invoke-Moon query windows) | Where-Object visible | ForEach-Object {[long]$_.id})
            foreach ($title in $expected.Keys) {
                if ($expected[$title] -eq $name -and $seen[$title] -notin $visible) {
                    throw "Assigned helper $title did not appear on workspace $name"
                }
            }
        }
        Record-Pass 'assigned workspaces reveal exact native tiles and hide inactive tiles'
        $null = Invoke-Moon exit
        Wait-Until -Description 'startup assignment fixture restores all original windows' -Condition { Test-OriginalsRestored }
        Record-Pass 'startup assignment fixture restores original frames on exit'
        return
    }
    Assert-Placements
    Record-Pass 'four native windows tile exactly'
    $extension = (& $ExtensionPath -Once -DaemonPath $MoonPath) | ConvertFrom-Json
    if (@($extension.workspaces).Count -ne 10) { throw 'The standalone IPC extension could not query all workspaces' }
    $guardState = Invoke-Moon query world
    $guardWindows = @($guardState.windows.PSObject.Properties.Name | Sort-Object)
    $wrongFocus = [long](@($guardWindows | Where-Object { [long]$_ -ne [long]$guardState.focused })[0])
    $guardResponse = Invoke-PublicRequest @{ version=1; id='guard-regression'; method='command'; commands=@('close'); expected_focus=$wrongFocus }
    [IO.File]::WriteAllText((Join-Path $testDirectory 'guard-rejection.json'), ($guardResponse | ConvertTo-Json -Depth 8), $utf8)
    if ($guardResponse.ok -or $guardResponse.error -ne 'focus changed; guarded commands were not executed') { throw 'Mismatched focus guard did not explicitly reject close' }
    Start-Sleep -Milliseconds 100
    $afterGuard = @((Invoke-Moon query world).windows.PSObject.Properties.Name | Sort-Object)
    if (($afterGuard -join ',') -ne ($guardWindows -join ',')) { throw 'Rejected close changed the managed window set' }
    foreach ($id in $guardWindows) { if (![MoonIntegrationNative]::IsWindow([IntPtr][long]$id)) { throw 'Rejected close destroyed a native helper HWND' } }
    if (@([MoonIntegrationNative]::Windows([uint32]$helper.Id)).Count -ne 4) { throw 'Rejected close changed the native helper window set' }
    Record-Pass 'standalone public IPC extension and guarded close rejection'
    if ($GuardOnly) { return }
    $beforeMinimize = (Invoke-Moon query world).workspaces[0].tree | ConvertTo-Json -Depth 64 -Compress
    $minimizeId = (Invoke-Moon query world).focused
    [void][MoonIntegrationNative]::ShowWindowAsync([IntPtr][long]$minimizeId, 6)
    Wait-Until -Description 'minimized window suspends without being moved' -Condition { @(Invoke-Moon query windows).Count -eq 3 }
    Assert-Placements
    if (((Invoke-Moon query world).workspaces[0].tree | ConvertTo-Json -Depth 64 -Compress) -ne $beforeMinimize) { throw 'Minimizing changed persistent tree membership' }
    [void][MoonIntegrationNative]::ShowWindowAsync([IntPtr][long]$minimizeId, 9)
    Wait-Until -Description 'restored window returns to tiling' -Condition { @(Invoke-Moon query windows).Count -eq 4 }
    Assert-Placements
    Record-Pass 'minimize restores the same tree slot'


    foreach ($command in @(@('focus','left'),@('move','right'),@('resize','width','+2%'),@('toggle-direction'))) {
        $null = Invoke-Moon @command
        Assert-Placements
        if ($command[0] -eq 'focus') {
            Wait-Until -Description 'focus command activates the owned helper' -Seconds 5 -Condition {
                [MoonIntegrationNative]::GetForegroundWindow().ToInt64() -eq [long](Invoke-Moon query world).focused
            }
        }
    }
    Record-Pass 'focus move resize and direction commands'
    $null = Invoke-Moon toggle-floating
    if (!(@(Invoke-Moon query windows | Where-Object floating).Count -eq 1)) { throw 'Floating toggle did not change exactly one window' }
    Assert-Placements
    $null = Invoke-Moon toggle-floating
    $null = Invoke-Moon toggle-fullscreen
    Assert-Placements
    $null = Invoke-Moon toggle-fullscreen
    Assert-Placements
    Record-Pass 'floating and fullscreen transitions'

    # Simulate a game's native borderless fullscreen transition on our own helper.
    $null = Invoke-Moon workspace 1
    $state = Invoke-Moon query world
    $nativeId = [long]$state.focused
    $nativeHandle = [IntPtr]$nativeId
    Wait-Until -Description "native fullscreen fixture foreground must be owned helper $nativeId" -Seconds 5 -Condition {
        [MoonIntegrationNative]::GetForegroundWindow().ToInt64() -eq $nativeId
    }
    $originalStyle = [MoonIntegrationNative]::GetWindowStyle($nativeHandle, -16)
    $nativeMonitor = @($state.monitors | Where-Object { $_.id -eq $state.active_monitor })[0]
    $beforeNative = [MoonIntegrationNative]::Frame($nativeId)
    try {
        $borderless = [IntPtr](($originalStyle.ToInt64() -band (-bnot [long]0x00CF0000)) -bor [long]2147483648)
        [void][MoonIntegrationNative]::SetWindowStyle($nativeHandle, -16, $borderless)
        $positioned = [MoonIntegrationNative]::SetWindowPos($nativeHandle, [IntPtr]::Zero, $nativeMonitor.bounds.x, $nativeMonitor.bounds.y, $nativeMonitor.bounds.width, $nativeMonitor.bounds.height, 0x24)
        if (!$positioned) { throw "Native fullscreen placement failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())" }
        $probe = [pscustomobject]@{ id=$nativeId; foreground=[MoonIntegrationNative]::GetForegroundWindow().ToInt64(); style=[MoonIntegrationNative]::GetWindowStyle($nativeHandle,-16).ToInt64(); frame=[MoonIntegrationNative]::Frame($nativeId); bounds=$nativeMonitor.bounds }
        [IO.File]::WriteAllText((Join-Path $testDirectory 'native-fullscreen-probe.json'),($probe | ConvertTo-Json -Depth 8),$utf8)
        Wait-Until -Description 'native borderless fullscreen is observed' -Condition { [bool](Invoke-Moon query world).native_fullscreen }
        $nativeState = Invoke-Moon query world
        if ($nativeState.windows.PSObject.Properties[[string]$nativeId].Value.fullscreen) { throw 'Native fullscreen incorrectly changed the manual fullscreen flag' }
        Assert-Placements
    } finally {
        [void][MoonIntegrationNative]::SetWindowStyle($nativeHandle, -16, $originalStyle)
        [void][MoonIntegrationNative]::SetWindowPos($nativeHandle, [IntPtr]::Zero, $beforeNative.Left, $beforeNative.Top, ($beforeNative.Right-$beforeNative.Left), ($beforeNative.Bottom-$beforeNative.Top), 0x24)
    }
    Wait-Until -Description 'native fullscreen exit returns to tiling' -Condition { !(Invoke-Moon query world).native_fullscreen }
    Assert-Placements
    Record-Pass 'native borderless fullscreen preserves manual state'

    $null = Invoke-Moon move-to-workspace 2 --follow
    Assert-Placements
    $visibleAfterMove = @(Invoke-Moon query windows | Where-Object visible).Count
    if ($visibleAfterMove -ne 1) { throw "Move-and-follow did not isolate destination workspace (visible=$visibleAfterMove)" }
    $null = Invoke-Moon recent-workspace
    Assert-Placements
    if (@(Invoke-Moon query windows | Where-Object visible).Count -ne 3) { throw 'Recent-workspace did not restore the previous workspace' }
    Record-Pass 'workspace cloak uncloak and back-and-forth'

    $beforeGap = (Invoke-Moon query config).gaps.inner
    [IO.File]::WriteAllText($configFile, "config-version=1`n[gaps]`ninnre=99`n", $utf8)
    $null = Invoke-Moon reload
    Wait-Until -Description 'invalid reload surfaces an error' -Condition { [bool](Invoke-Moon query stats).last_error }
    $reloadError = (Invoke-Moon query stats).last_error
    if ($reloadError -notmatch 'config with spaces\.toml' -or $reloadError -notmatch 'line 3' -or $reloadError -notmatch 'unknown field.*innre' -or $reloadError -notmatch 'expected.*inner.*outer') {
        throw "Reload error omitted the file, line, invalid field or expected spelling: $reloadError"
    }
    if ((Invoke-Moon query config).gaps.inner -ne $beforeGap) { throw 'Invalid configuration replaced the last good configuration' }
    [IO.File]::WriteAllText($configFile, "config-version=1`n[gaps]`ninner=24`n", $utf8)
    $null = Invoke-Moon reload
    Wait-Until -Description 'valid reload changes gaps' -Condition { (Invoke-Moon query config).gaps.inner -eq 24 }
    Assert-Placements
    Record-Pass 'invalid reload preserves state and valid reload applies'

    $beforeRestart = State-Signature
    [IO.File]::WriteAllText((Join-Path $testDirectory 'before-restart.json'), $beforeRestart, $utf8)
    $oldOwner = (Read-Recovery).owner
    $null = Invoke-Moon restart
    Wait-Until -Description 'new process adopts restart journal' -Condition {
        $record = Read-Recovery
        if ($record.owner -eq $oldOwner) { return $false }
        $script:ownedDaemon = $record.owner
        return (Invoke-Moon query stats).managed_windows -eq 4
    }
    $afterRestart = State-Signature
    [IO.File]::WriteAllText((Join-Path $testDirectory 'after-restart.json'), $afterRestart, $utf8)
    if ($afterRestart -ne $beforeRestart) { throw 'Restart changed workspace layout or focus' }
    Assert-Placements
    Record-Pass 'restart preserves workspace trees focus and exact frames'

    $null = Invoke-Moon workspace 2
    $null = Invoke-Moon close
    Wait-Until -Description 'closed window disappears from world' -Condition { (Invoke-Moon query stats).managed_windows -eq 3 }
    $null = Invoke-Moon workspace 1
    Assert-Placements
    Record-Pass 'window close reflows survivors'

    $null = Invoke-Moon workspace 2
    Assert-Placements
    Wait-Until -Description 'background checkpoint reaches the committed world before snapshotting' -Condition {
        $record = Read-Recovery
        return $record.world.revision -eq (Invoke-Moon query world).revision
    }
    $crashJournal = Read-RecoveryBytes
    $beforeKill = State-Signature
    $killWatch = [Diagnostics.Stopwatch]::StartNew()
    Stop-Process -Id $ownedDaemon -Force
    $ownedDaemon = $null
    Wait-Until -Description 'watchdog restores every surviving helper window' -Condition { Test-OriginalsRestored }
    $killWatch.Stop()
    if ($killWatch.ElapsedMilliseconds -gt 1000) { throw "Watchdog recovery exceeded one-second budget: $($killWatch.ElapsedMilliseconds) ms" }
    Record-Pass 'forced kill watchdog restores all windows within one second'

    Wait-Until -Description 'previous daemon and watchdog have exited' -Condition { @(Get-Process -Name moon -ErrorAction SilentlyContinue).Count -eq 0 }
    [IO.File]::WriteAllBytes($journalFile, $crashJournal)
    $backgroundResume = Invoke-Moon -TimeoutSeconds 20 start --background --resume --only testwin --no-hook --config $configFile
    $ownedDaemon = [int]$backgroundResume.process_id
    if ((State-Signature) -ne $beforeKill) { throw 'Background resume discarded the saved workspace layout' }
    Assert-Placements
    $null = Invoke-Moon exit
    Wait-Until -Description 'background resumed daemon restores on exit' -Condition { Test-OriginalsRestored }
    $ownedDaemon = $null
    Record-Pass 'background resume adopts saved state and restores on exit'

    # Recreate a recoverable interrupted restart, then make IPC startup fail.
    # Every HWND in this journal belongs to our still-running helper process.
    Wait-Until -Description 'previous daemon and watchdog have exited' -Condition { @(Get-Process -Name moon -ErrorAction SilentlyContinue).Count -eq 0 }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try { $pipeName = [MoonExtensionIdentity]::PipeName($identity.Token) } finally { $identity.Dispose() }
    $blocker = New-Object IO.Pipes.NamedPipeServerStream($pipeName, [IO.Pipes.PipeDirection]::InOut, 1, [IO.Pipes.PipeTransmissionMode]::Byte, [IO.Pipes.PipeOptions]::Asynchronous)
    try {
        $parked = [MoonIntegrationNative]::Windows([uint32]$helper.Id)[0]
        $original = $originals[[string]$parked]
        [IO.File]::WriteAllBytes($journalFile, $crashJournal)
        if (![MoonIntegrationNative]::SetWindowPos([IntPtr]$parked, [IntPtr]::Zero, -32000, -32000, ($original.Right-$original.Left), ($original.Bottom-$original.Top), 0x14)) { throw 'Could not park the owned failure-injection window' }
        $startupError = $null
        try { $null = Invoke-Moon -TimeoutSeconds 20 start --resume --only testwin --no-hook --config $configFile }
        catch { $startupError = $_.Exception.Message }
        if (!$startupError) { throw 'Conflicting pipe unexpectedly permitted resumed startup' }
        if ($startupError -notmatch '0x80070005|[Aa]ccess.*denied|pipe') { throw "Startup failed outside the intended IPC collision: $startupError" }
        [IO.File]::WriteAllText((Join-Path $testDirectory 'startup-failure.txt'), $startupError, $utf8)
        Wait-Until -Description 'failed resumed startup restores original frames and removes journal' -Condition { Test-OriginalsRestored }
        Record-Pass 'failed resumed startup restores its inherited recovery journal'
    } finally { $blocker.Dispose() }
} catch {
    $failure = $_
    if (Test-Path $journalFile) { [IO.File]::WriteAllBytes((Join-Path $testDirectory 'recovery-at-failure.json'), (Read-RecoveryBytes)) }
    if ($helper) {
        $nativeFailure = @([MoonIntegrationNative]::Windows([uint32]$helper.Id) | ForEach-Object {
            [pscustomobject]@{ id=$_; frame=[MoonIntegrationNative]::Frame($_); cloak=[MoonIntegrationNative]::Cloaked($_); original=$originals[[string]$_] }
        })
        [IO.File]::WriteAllText((Join-Path $testDirectory 'native-at-failure.json'), ($nativeFailure | ConvertTo-Json -Depth 8), $utf8)
    }
    if ($ownedDaemon) {
        foreach ($query in @('world','windows','stats','transactions')) {
            try { [IO.File]::WriteAllText((Join-Path $testDirectory ($query+'.json')), ((Invoke-Moon query $query) | ConvertTo-Json -Depth 64), $utf8) } catch { }
        }
    }
    $results.Add([pscustomobject]@{ name='integration failure'; passed=$false; error=$failure.Exception.Message; at=[DateTime]::UtcNow.ToString('o') })
    throw $failure
} finally {
    if ($ownedDaemon -and (Get-Process -Id $ownedDaemon -ErrorAction SilentlyContinue)) {
        try { $null = Invoke-Moon exit } catch { Stop-Process -Id $ownedDaemon -Force -ErrorAction SilentlyContinue }
        Start-Sleep -Milliseconds 500
    }
    if ($helper -and (!$ownedDaemon -or !(Get-Process -Id $ownedDaemon -ErrorAction SilentlyContinue))) {
        try { $null = & $MoonPath restore 2>&1 } catch { }
    }
    if ($helper) { Stop-Process -Id $helper.Id -Force -ErrorAction SilentlyContinue }
    if (Test-Path (Join-Path $env:LOCALAPPDATA 'moon\moon.log')) {
        Copy-Item -LiteralPath (Join-Path $env:LOCALAPPDATA 'moon\moon.log') -Destination (Join-Path $testDirectory 'daemon.log')
    }
    [void][IO.Directory]::CreateDirectory((Split-Path -Parent $ReportPath))
    [IO.File]::WriteAllText($ReportPath, ($results | ConvertTo-Json -Depth 12), $utf8)
    $evidenceDirectory = Join-Path (Join-Path (Split-Path -Parent $ReportPath) 'integration-logs') (Split-Path -Leaf $testDirectory)
    [void][IO.Directory]::CreateDirectory($evidenceDirectory)
    Get-ChildItem -LiteralPath $testDirectory -File | Copy-Item -Destination $evidenceDirectory -Force
    Write-Host "Integration report: $ReportPath"
    Write-Host "Retained evidence: $evidenceDirectory"
    Write-Host "Process logs: $testDirectory"
}
