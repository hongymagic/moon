[CmdletBinding()]
param(
    [string]$BuildDirectory,
    [string]$ExtensionPath = (Join-Path $PSScriptRoot '..\examples\workspace-state.ps1'),
    [string]$MoonPath = (Join-Path $env:LOCALAPPDATA 'moon-dev\moon.exe'),
    [string]$TestwinPath = (Join-Path $env:LOCALAPPDATA 'moon-dev\testwin.exe'),
    [string]$ReportPath = (Join-Path $env:LOCALAPPDATA 'moon-dev\integration-results.json'),
    [switch]$GuardOnly
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
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
$buildEvidence = [pscustomobject]@{
    started_at = [DateTime]::UtcNow.ToString('o')
    os = [Environment]::OSVersion.VersionString
    moon = [pscustomobject]@{ path=$MoonPath; sha256=(Get-FileHash -LiteralPath $MoonPath -Algorithm SHA256).Hash }
    testwin = [pscustomobject]@{ path=$TestwinPath; sha256=(Get-FileHash -LiteralPath $TestwinPath -Algorithm SHA256).Hash }
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
    $pipe = New-Object IO.Pipes.NamedPipeClientStream('.', $pipeName, [IO.Pipes.PipeDirection]::InOut, [IO.Pipes.PipeOptions]::Asynchronous)
    try {
        $pipe.Connect(5000)
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

try {
    # Existing daemons and helper windows belong to the user or another test run.
    $existing = @(Get-Process -Name moon,testwin -ErrorAction SilentlyContinue)
    if ($existing.Count -gt 0) { throw 'Close existing moon/testwin test instances before running integration tests.' }
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
    Assert-Placements
    Record-Pass 'four native windows tile exactly'
    $extension = (& $ExtensionPath -Once) | ConvertFrom-Json
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
