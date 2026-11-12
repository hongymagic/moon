[CmdletBinding()]
param(
    [string]$BuildDirectory,
    [string]$OutputDirectory,
    [ValidateSet('Default','Static')][string]$Mode='Default',
    [ValidateRange(30,120)][int]$Seconds=60,
    [switch]$CompileOnly
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
# Counter contracts (cycles are deliberately not converted into time):
# https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes
# https://learn.microsoft.com/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryprocesscycletime
# https://learn.microsoft.com/windows/win32/sysinfo/acquiring-high-resolution-time-stamps
# This branch only compiles the existing ownership/frame helpers. It neither
# launches their application scenarios nor sends input or changes foreground.
if (!('MoonApplicationNative' -as [type])) {
    & (Join-Path $PSScriptRoot 'application-integration.ps1') -CompileOnly | Out-Null
}
if (!('MoonIdleNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public sealed class MoonIdleNative : IDisposable {
    public sealed class Sample {
        public long qpc_before, qpc_after, qpc_frequency, process_birth;
        public long kernel_100ns, user_100ns;
        public ulong cycles;
    }
    [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,uint id);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetProcessTimes(IntPtr process,out long created,out long exited,out long kernel,out long user);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool QueryProcessCycleTime(IntPtr process,out ulong cycles);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool QueryPerformanceCounter(out long value);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool QueryPerformanceFrequency(out long value);
    [DllImport("kernel32.dll",SetLastError=true)] static extern uint WaitForSingleObject(IntPtr handle,uint milliseconds);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    IntPtr handle; readonly long birth, frequency;
    public MoonIdleNative(uint process,long expectedBirth) {
        handle=OpenProcess(0x1000|0x100000,false,process);
        if(handle==IntPtr.Zero)throw new Win32Exception();
        try {
            birth=expectedBirth;
            if(!QueryPerformanceFrequency(out frequency)||frequency<=0)throw new Win32Exception();
            Read();
        } catch {Dispose();throw;}
    }
    public Sample Read() {
        if(handle==IntPtr.Zero)throw new ObjectDisposedException("MoonIdleNative");
        if(WaitForSingleObject(handle,0)!=258)throw new InvalidOperationException("Owned daemon is no longer running");
        var sample=new Sample{qpc_frequency=frequency};long exited;
        if(!QueryPerformanceCounter(out sample.qpc_before))throw new Win32Exception();
        if(!GetProcessTimes(handle,out sample.process_birth,out exited,out sample.kernel_100ns,out sample.user_100ns))throw new Win32Exception();
        if(!QueryProcessCycleTime(handle,out sample.cycles))throw new Win32Exception();
        if(!QueryPerformanceCounter(out sample.qpc_after))throw new Win32Exception();
        if(sample.process_birth!=birth||WaitForSingleObject(handle,0)!=258)throw new InvalidOperationException("Owned daemon identity or liveness changed");
        return sample;
    }
    public void Dispose(){if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}}
}
'@
}
if ($CompileOnly) {
    [pscustomobject]@{compiled=$true;desktop_changed=$false;processes_started=0} | ConvertTo-Json
    return
}
if (!$BuildDirectory -or !$OutputDirectory) {throw 'Provide BuildDirectory and a new OutputDirectory for this isolated measurement.'}
if ([MoonInstallNative]::Integrity() -ne 8192 -or (Test-MoonAdministrator)) {throw 'Use an ordinary medium-integrity Windows coordinator.'}
$moon=(Resolve-Path -LiteralPath (Join-Path $BuildDirectory 'moon.exe')).ProviderPath
$testwin=(Resolve-Path -LiteralPath (Join-Path $BuildDirectory 'testwin.exe')).ProviderPath
if (@(Get-Process -Name @('moon','testwin','glazewm','komorebi','whim','zebar','tacky','tacky-borders') -ErrorAction SilentlyContinue).Count) {
    throw 'The wrapper must stop Moon and the previous desktop stack first; existing processes were left unchanged.'
}
$journal=Join-Path $env:LOCALAPPDATA ('moon\journal-v1-session'+[Diagnostics.Process]::GetCurrentProcess().SessionId+'.json')
if (Test-Path -LiteralPath $journal) {throw 'An existing recovery journal belongs to an earlier run; it was left unchanged.'}
$OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $OutputDirectory) {throw 'OutputDirectory must be new; previous evidence will not be overwritten.'}
[void][IO.Directory]::CreateDirectory($OutputDirectory)
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-idle-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
$config=Join-Path $temporary 'owned-idle.toml'
$encoding=[Text.UTF8Encoding]::new($false)
$source=@'
config-version = 1
theme = "dark"
cursor-jump = false
focus-follows-cursor = false
[[rules]]
commands = ["ignore"]
matches = [{process={not-regex='(?i)^testwin(?:\.exe)?$'}}]
[bar]
enabled = true
'@
if ($Mode -eq 'Static') {
    # Default order, icons, styles and caps. Fixed representative text deliberately
    # does not claim identical widths/provider availability to the Default run.
    # Workspaces alone retain their special renderer; the endpoint world and
    # revision checks establish whether their state stayed fixed.
    $source+="`n"+@'
widgets = [
  {id="logo", format="\u263e", color="terracotta", click="cheatsheet"},
  {id="workspaces", format="{workspaces}"},
  {id="clock", format="Fri 13 Nov 12:00", align="center", background="surface0"},
  {id="title", format="moon test window 8", align="center", background="surface0", color="subtext0", max-width=420},
  {id="state", format="horizontal", color="terracotta", background="surface0", click="toggle-direction"},
  {id="media", icon="\uf001", icon-color="rose-dust", background="surface0", format="Idle audio", align="right", max-width=220},
  {id="keyboard", icon="\uf11c", icon-color="clay", background="surface0", format="en", align="right"},
  {id="network", icon="\uf1eb", icon-color="terracotta", background="surface0", format="Connected", align="right"},
  {id="memory", icon="\uefc5", icon-color="olive", background="surface0", format="32%", align="right"},
  {id="cpu", icon="\uf4bc", icon-color="amber", background="surface0", format="2%", align="right"},
  {id="battery", icon="\uf240", icon-color="gold", background="surface0", format="100%", align="right"},
  {id="volume", icon="\uf028", icon-color="amber", background="surface0", format="50%", align="right"},
  {id="weather", icon="\uf0c2", icon-color="steel", background="surface0", format="20 C", align="right"},
]
'@
}
$source+="`n[borders]`nenabled = true`n"
[IO.File]::WriteAllText($config,$source,$encoding)
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'fixture.toml'),$source,$encoding)
$owned=[Collections.Generic.List[object]]::new()
$windows=@();$sampler=$null;$daemon=$null;$helper=$null;$failure=$null
$cleanupErrors=[Collections.Generic.List[string]]::new()
$oldDpi=[MoonElevatedNative]::SetThreadDpiAwarenessContext([IntPtr](-4))
$report=[ordered]@{
    started_at=[DateTime]::UtcNow.ToString('o');mode=$Mode;requested_seconds=$Seconds;warmup_seconds=5
    moon=[ordered]@{path=$moon;sha256=(Get-FileHash -LiteralPath $moon -Algorithm SHA256).Hash}
    testwin=[ordered]@{path=$testwin;sha256=(Get-FileHash -LiteralPath $testwin -Algorithm SHA256).Hash}
    config_path=$config;config_sha256=(Get-FileHash -LiteralPath $config -Algorithm SHA256).Hash
    providers='Default provider worker remains enabled in both modes; Static changes displayed templates only.'
    measurement='Process-wide user/kernel CPU in 100ns units; cycles retained without time conversion; QPC midpoint wall interval. State/counter queries bracket, and are outside, the raw CPU interval.'
    static_geometry='Default widget order/icons/styles/caps with representative literal text; optional provider presence and exact widths can differ from Default.'
    quiet_interval_ipc_requests=0;measurement_completed=$false;static_qualified=$false
    environment=[ordered]@{MOON_PROFILE_INPUT=$env:MOON_PROFILE_INPUT;MOON_GRAPHICS_DRIVER=$env:MOON_GRAPHICS_DRIVER;MOON_WARP_SERIAL=$env:MOON_WARP_SERIAL;MOON_COMPOSITION_WAIT=$env:MOON_COMPOSITION_WAIT;MOON_TESTWIN_PLACEMENT_TIMING=$env:MOON_TESTWIN_PLACEMENT_TIMING}
}
function Start-IdleProcess([string]$Path,[string[]]$Words) {
    $start=[Diagnostics.ProcessStartInfo]::new();$start.FileName=$Path
    $start.Arguments=(@($Words|ForEach-Object {ConvertTo-MoonArgument $_}) -join ' ')
    $start.UseShellExecute=$false;$start.CreateNoWindow=$true
    $start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
    $process=[Diagnostics.Process]::Start($start)
    # Retain the actual process handle, not a later PID lookup, for forced cleanup.
    $item=[pscustomobject]@{process=$process;id=$process.Id;birth=0L;path=$Path;stdout=$process.StandardOutput.ReadToEndAsync();stderr=$process.StandardError.ReadToEndAsync()}
    $owned.Add($item)
    $identity=[MoonInstallNative]::ProcessIdentity([uint32]$process.Id)
    if (![string]::Equals($identity.Path,$Path,[StringComparison]::OrdinalIgnoreCase)) {throw 'An owned process launched from an unexpected path.'}
    $item.birth=$identity.Birth
    return $item
}
function Assert-IdleProcess($Item) {
    if ($Item.process.HasExited) {throw 'An owned fixture process exited unexpectedly.'}
    $identity=[MoonInstallNative]::ProcessIdentity([uint32]$Item.id)
    if ($identity.Birth -ne $Item.birth -or ![string]::Equals($identity.Path,$Item.path,[StringComparison]::OrdinalIgnoreCase)) {throw 'An owned fixture process identity changed.'}
}
function Assert-IdleDaemonOwner($Stats) {
    Assert-IdleProcess $daemon
    if ($Stats.process_id -ne $daemon.id -or ![string]::Equals($Stats.executable,$moon,[StringComparison]::OrdinalIgnoreCase) -or ![string]::Equals($Stats.config_path,$config,[StringComparison]::OrdinalIgnoreCase)) {throw 'IPC is connected to a different daemon; it was left unchanged.'}
}
function Assert-IdleStats($Stats) {
    Assert-IdleDaemonOwner $Stats
    if ($Stats.managed_windows -ne 8 -or $null -ne $Stats.last_error -or $null -eq $Stats.renderer) {throw 'The fixture needs eight managed windows, active surfaces and no daemon error.'}
}
function Read-IdleEndpoint {
    $stats=Invoke-MoonRpc query stats
    Assert-IdleStats $stats
    Assert-IdleProcess $helper
    $world=Invoke-MoonRpc query world
    $placements=@(Invoke-MoonRpc query windows)
    if ($placements.Count -ne 8 -or @($world.windows.PSObject.Properties).Count -ne 8 -or $world.revision -ne $stats.revision) {throw 'Fixture state changed while its endpoint was read.'}
    $native=@(foreach ($item in $windows) {
        if (![MoonElevatedNative]::IsWindow([IntPtr][long]$item.id) -or [MoonApplicationNative]::ProcessId([long]$item.id) -ne $helper.id) {throw 'An owned window identity changed.'}
        $placement=@($placements|Where-Object {$_.id -eq $item.id})
        if ($placement.Count -ne 1 -or !$placement[0].visible -or $placement[0].floating -or $placement[0].fullscreen) {throw 'An owned fixture window is missing, hidden or no longer tiled.'}
        $frame=[MoonElevatedNative]::Frame([long]$item.id)
        $cloak=[MoonApplicationNative]::Cloaked([long]$item.id)
        $rect=$placement[0].rect
        if ($cloak -ne 0 -or [Math]::Abs($frame.Left-$rect.x) -gt 1 -or [Math]::Abs($frame.Top-$rect.y) -gt 1 -or [Math]::Abs($frame.Right-$frame.Left-$rect.width) -gt 1 -or [Math]::Abs($frame.Bottom-$frame.Top-$rect.height) -gt 1) {throw 'An owned native frame does not match its committed placement.'}
        [pscustomobject]@{id=$item.id;process_id=$helper.id;process_birth=$helper.birth;frame=$frame;cloaked=$cloak}
    })
    [pscustomobject]@{stats=$stats;world=$world;placements=$placements;native=$native}
}
function Get-IdleCounters($Endpoint) {
    $renderer=$Endpoint.stats.renderer
    [ordered]@{revision=[long]$Endpoint.stats.revision;scenes=[long]$renderer.changed_scenes;bar_draws=[long]$renderer.bar.begin_draw_calls;border_draws=[long]$renderer.border.begin_draw_calls;layouts=[long]$renderer.text_layouts_created;render_calls=[long]$renderer.render_calls;commits=[long]$renderer.commits}
}
try {
    Invoke-MoonNative $moon @('config','validate','--config',$config) | Out-Null
    $helper=Start-IdleProcess -Path $testwin -Words @('8')
    Wait-MoonCondition -Description 'eight owned fixture windows' -Condition {@([MoonElevatedNative]::Windows([uint32]$helper.id)).Count -eq 8}
    $windows=@(foreach ($handle in [MoonElevatedNative]::Windows([uint32]$helper.id)) {[pscustomobject]@{id=$handle;original=[MoonElevatedNative]::Frame($handle)}})
    $report.owned_windows=$windows
    # The owned ignore rule provides isolation. --only would disable surfaces
    # and keyboard hooks, invalidating this full-product idle comparison.
    $daemon=Start-IdleProcess -Path $moon -Words @('start','--config',$config)
    Wait-MoonCondition -Description 'owned daemon adopts and arranges all eight windows' -Condition {$null=Read-IdleEndpoint;return $true}
    Start-Sleep -Seconds 5
    $before=Read-IdleEndpoint
    $report.before=$before
    $sampler=[MoonIdleNative]::new([uint32]$daemon.id,[long]$daemon.birth)
    $rawBefore=$sampler.Read()
    # No IPC, process sampling, native-frame reads or report writes in this interval.
    [Threading.Thread]::Sleep($Seconds*1000)
    $rawAfter=$sampler.Read()
    $report.raw_before=$rawBefore;$report.raw_after=$rawAfter
    $after=Read-IdleEndpoint
    $report.after=$after
    $elapsedTicks=(([decimal]$rawAfter.qpc_before+$rawAfter.qpc_after)-([decimal]$rawBefore.qpc_before+$rawBefore.qpc_after))/2
    $elapsedSeconds=[double]($elapsedTicks/$rawBefore.qpc_frequency)
    $kernelDelta=$rawAfter.kernel_100ns-$rawBefore.kernel_100ns
    $userDelta=$rawAfter.user_100ns-$rawBefore.user_100ns
    $cycleDelta=[decimal]$rawAfter.cycles-[decimal]$rawBefore.cycles
    if ($elapsedSeconds -le 0 -or $kernelDelta -lt 0 -or $userDelta -lt 0 -or $cycleDelta -lt 0) {throw 'Invalid or non-monotonic raw process counters.'}
    $report.wall_seconds=$elapsedSeconds
    $report.endpoint_read_uncertainty_seconds=[double](($rawBefore.qpc_after-$rawBefore.qpc_before+$rawAfter.qpc_after-$rawAfter.qpc_before)/(2.0*$rawBefore.qpc_frequency))
    $report.delta=[ordered]@{kernel_100ns=$kernelDelta;user_100ns=$userDelta;cycles=$cycleDelta;cpu_percent_one_core=(100.0*($kernelDelta+$userDelta)/10000000.0/$elapsedSeconds)}
    $startCounters=Get-IdleCounters $before;$endCounters=Get-IdleCounters $after
    $counterDelta=[ordered]@{}
    foreach ($key in $startCounters.Keys) {$counterDelta[$key]=$endCounters[$key]-$startCounters[$key];if($counterDelta[$key] -lt 0){throw 'A renderer counter reset during the idle measurement.'}}
    $report.counter_delta=$counterDelta
    $report.world_unchanged=($before.world|ConvertTo-Json -Depth 64 -Compress) -ceq ($after.world|ConvertTo-Json -Depth 64 -Compress)
    $report.native_unchanged=($before.native|ConvertTo-Json -Depth 8 -Compress) -ceq ($after.native|ConvertTo-Json -Depth 8 -Compress)
    $report.placements_unchanged=($before.placements|ConvertTo-Json -Depth 8 -Compress) -ceq ($after.placements|ConvertTo-Json -Depth 8 -Compress)
    $report.measurement_completed=$true
    $report.static_qualified=$Mode -eq 'Static' -and $report.world_unchanged -and $report.native_unchanged -and $report.placements_unchanged
    foreach ($key in @('revision','scenes','bar_draws','border_draws','layouts','commits')) {if($counterDelta[$key] -ne 0){$report.static_qualified=$false}}
    if (!$report.world_unchanged -or !$report.native_unchanged -or !$report.placements_unchanged) {throw 'Fixture world or native geometry changed during the quiet interval; CPU data is retained but the idle comparison is not qualified.'}
    if ($Mode -eq 'Static' -and !$report.static_qualified) {throw 'Static scene counters changed; this is not a no-scene-change idle sample.'}
} catch {$failure=$_.Exception.Message}
finally {
    if ($sampler) {$sampler.Dispose()}
    if ($daemon -and !$daemon.process.HasExited) {
        try {
            $current=Invoke-MoonRpc query stats
            Assert-IdleDaemonOwner $current
            Invoke-MoonRpc -Method command -Commands @('exit') | Out-Null
            if (!$daemon.process.WaitForExit(10000)) {throw 'Owned daemon did not exit cleanly.'}
        } catch {
            $cleanupErrors.Add($_.Exception.Message)
            try {Assert-IdleProcess $daemon;$daemon.process.Kill();if(!$daemon.process.WaitForExit(5000)){throw 'Owned daemon did not terminate.'}} catch {$cleanupErrors.Add($_.Exception.Message)}
        }
    }
    if ($daemon) {
        try {
            Wait-MoonCondition -Description 'owned daemon/watchdog restores originals and removes its journal' -Seconds 20 -Condition {
                if (Test-Path -LiteralPath $journal) {return $false}
                if (@(Get-Process -Name moon -ErrorAction SilentlyContinue).Count) {return $false}
                if (!$helper -or $helper.process.HasExited) {return $false}
                Assert-IdleProcess $helper
                foreach($item in $windows) {
                    if([MoonApplicationNative]::ProcessId([long]$item.id) -ne $helper.id -or [MoonApplicationNative]::Cloaked([long]$item.id) -ne 0){return $false}
                    $frame=[MoonElevatedNative]::Frame([long]$item.id)
                    foreach($edge in @('Left','Top','Right','Bottom')) {if([Math]::Abs($frame.$edge-$item.original.$edge) -gt 1){return $false}}
                }
                return $true
            }
            $report.originals_restored=$true
        } catch {$cleanupErrors.Add($_.Exception.Message);$report.originals_restored=$false}
    }
    foreach ($item in $owned) {
        try {
            if (!$item.process.HasExited) {
                Assert-IdleProcess $item
                $item.process.Kill()
                if (!$item.process.WaitForExit(5000)) {throw 'An owned process survived cleanup.'}
            }
            if ($item.stdout.Wait(2000)) {[IO.File]::WriteAllText((Join-Path $OutputDirectory ($item.id.ToString()+'.stdout.txt')),$item.stdout.Result,$encoding)}
            if ($item.stderr.Wait(2000)) {[IO.File]::WriteAllText((Join-Path $OutputDirectory ($item.id.ToString()+'.stderr.txt')),$item.stderr.Result,$encoding)}
        } catch {$cleanupErrors.Add($_.Exception.Message)}
        finally {$item.process.Dispose()}
    }
    if ($oldDpi -ne [IntPtr]::Zero) {[void][MoonElevatedNative]::SetThreadDpiAwarenessContext($oldDpi)}
    # Never delete a recovery journal or an unrecognized process. Preserve the
    # unique config if recovery did not finish so the wrapper can diagnose it.
    if (!$cleanupErrors.Count) {
        try {
            if ((Get-FileHash -LiteralPath $config -Algorithm SHA256).Hash -ne $report.config_sha256) {throw 'Fixture config was changed externally; it was preserved.'}
            Remove-Item -LiteralPath $config -Force
            [IO.Directory]::Delete($temporary)
        } catch {$cleanupErrors.Add($_.Exception.Message)}
    }
    $report.cleanup_errors=@($cleanupErrors)
    $report.error=$failure
    $report.finished_at=[DateTime]::UtcNow.ToString('o')
    $report.passed=($report.measurement_completed -and !$failure -and !$cleanupErrors.Count)
    [IO.File]::WriteAllText((Join-Path $OutputDirectory 'report.json'),($report|ConvertTo-Json -Depth 64),$encoding)
}
if ($failure) {throw $failure}
if ($cleanupErrors.Count) {throw ('Owned fixture cleanup failed: '+($cleanupErrors -join '; '))}
$report | ConvertTo-Json -Depth 64
