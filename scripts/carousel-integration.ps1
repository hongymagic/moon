[CmdletBinding()]
param([Parameter(Mandatory)][string]$BuildDirectory,[string]$ReportPath,[switch]$Measure)
. (Join-Path $PSScriptRoot 'install-common.ps1')
. (Join-Path $PSScriptRoot 'test-window-foreground.ps1')
$ErrorActionPreference='Stop'
if(!$ReportPath){$ReportPath=Join-Path $env:LOCALAPPDATA 'moon-dev\carousel-integration.json'}
$moon=Join-Path $BuildDirectory 'moon.exe'
$testwin=Join-Path $BuildDirectory 'testwin.exe'
if(@(Get-Process moon,testwin,glazewm,komorebi -ErrorAction SilentlyContinue).Count){throw 'Carousel integration needs the previous manager stopped and no active Moon/testwin instances.'}
$folder=Join-Path ([IO.Path]::GetTempPath()) ('moon-carousel-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($folder)
$config=Join-Path $folder 'config.toml'
[IO.File]::WriteAllText($config,"config-version=1`nworkspace-animation=true`n",[Text.UTF8Encoding]::new($false))
$journal=Join-Path $env:LOCALAPPDATA ('moon\journal-v1-session'+[Diagnostics.Process]::GetCurrentProcess().SessionId+'.json')
if(Test-Path $journal){throw 'A previous recovery journal needs restoration before this test.'}
Add-Type -TypeDefinition @'
using System;using System.Collections.Generic;using System.Runtime.InteropServices;using System.Text;
public static class MoonCarouselIntegration {
 [StructLayout(LayoutKind.Sequential)]public struct Rect{public int Left,Top,Right,Bottom;}
 delegate bool EnumProc(IntPtr h,IntPtr p);
 [DllImport("user32.dll")]static extern bool EnumWindows(EnumProc c,IntPtr p);
 [DllImport("user32.dll")]static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int GetClassName(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")]public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")]public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")]public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [DllImport("dwmapi.dll")]static extern int DwmGetWindowAttribute(IntPtr h,uint a,out Rect r,uint n);
 [DllImport("dwmapi.dll",EntryPoint="DwmGetWindowAttribute")]static extern int GetCloak(IntPtr h,uint a,out uint r,uint n);
 public static long[] Windows(uint pid,string name){var v=new List<long>();EnumWindows((h,p)=>{uint id;GetWindowThreadProcessId(h,out id);var s=new StringBuilder(256);GetClassName(h,s,256);if(id==pid&&s.ToString()==name)v.Add(h.ToInt64());return true;},IntPtr.Zero);return v.ToArray();}
 public static Rect Frame(long h){Rect r;Marshal.ThrowExceptionForHR(DwmGetWindowAttribute(new IntPtr(h),9,out r,16));return r;}
 public static uint Cloak(long h){uint r;Marshal.ThrowExceptionForHR(GetCloak(new IntPtr(h),14,out r,4));return r;}
}
'@
[void][MoonCarouselIntegration]::SetThreadDpiAwarenessContext([IntPtr](-4))
function Moon([string[]]$Words) {
 $text=(Invoke-MoonNative $moon $Words).Output
 if($text.Trim().StartsWith('{') -or $text.Trim().StartsWith('[')) {
  $parsed=$text|ConvertFrom-Json
  foreach($item in $parsed){Write-Output $item}
 } else {$text}
}
function Wait-Workspace([int]$VisibleCount,[bool]$RequireForeground=$true) {
 Wait-MoonCondition -Description "$VisibleCount visible windows with exact native frames and no transition overlay" -Condition {
  $stats=Moon -Words @('query','stats');if($stats.carousel_active){return $false}
  $placements=@(Moon -Words @('query','windows'));$visible=@($placements|Where-Object visible)
  $script:lastObserved=[pscustomobject]@{stats=$stats;placements=$placements;foreground=[MoonCarouselIntegration]::GetForegroundWindow().ToInt64();native=@($placements|ForEach-Object{[pscustomobject]@{id=$_.id;frame=[MoonCarouselIntegration]::Frame([long]$_.id);cloak=[MoonCarouselIntegration]::Cloak([long]$_.id)}})}
  if($placements.Count -ne 4 -or $visible.Count -ne $VisibleCount){return $false}
  foreach($p in $placements){
   $cloak=[MoonCarouselIntegration]::Cloak([long]$p.id)
   if($p.visible){
    if($cloak -ne 0){return $false};$r=[MoonCarouselIntegration]::Frame([long]$p.id)
    if([Math]::Abs($r.Left-$p.rect.x)-gt 1 -or [Math]::Abs($r.Top-$p.rect.y)-gt 1 -or [Math]::Abs(($r.Right-$r.Left)-$p.rect.width)-gt 1 -or [Math]::Abs(($r.Bottom-$r.Top)-$p.rect.height)-gt 1){return $false}
   }elseif($cloak -eq 0){return $false}
  }
  if(!$RequireForeground){return $true}
  $world=Moon -Words @('query','world')
  return $world.focused -and [MoonCarouselIntegration]::GetForegroundWindow().ToInt64() -eq [long]$world.focused
 }
}
function Record([string]$Message){$checks.Add($Message);Write-Host "PASS $Message"}
function Measure-WorkspaceSwitch([string]$Workspace) {
 $process=[Diagnostics.Process]::GetProcessById($owned)
 try {
  $process.Refresh();$before=[long]$process.WorkingSet64;$peak=$before
  $timer=[Diagnostics.Stopwatch]::StartNew()
  $null=Invoke-MoonRpc -Method command -DevelopmentPath $moon -Commands @("workspace $Workspace")
  $response=$timer.Elapsed.TotalMilliseconds
  $observed=$false;$bounds=@();$samples=0;$lastSample=$response;$maxGap=0.0
  do {
   # External HWND and process queries do not wake/redraw the daemon every sample.
   $overlays=@([MoonCarouselIntegration]::Windows([uint32]$owned,'moon.carousel'))
   $process.Refresh();$memory=[long]$process.WorkingSet64;$peak=[Math]::Max($peak,$memory)
   $now=$timer.Elapsed.TotalMilliseconds;$maxGap=[Math]::Max($maxGap,$now-$lastSample);$lastSample=$now;$samples++
   if(!$overlays.Count){break}
   if(!$observed){$bounds=@($overlays|ForEach-Object{[MoonCarouselIntegration]::Frame($_)})}
   $observed=$true
   if($timer.Elapsed.TotalSeconds -gt 2){throw 'The owned workspace overlay did not close within two seconds.'}
   Start-Sleep -Milliseconds 10
  }while($true)
  $ended=$timer.Elapsed.TotalMilliseconds
  if(!$observed){throw 'No workspace overlay was observed; an animation timing sample cannot be claimed.'}
  [pscustomobject]@{workspace=$Workspace;command_response_ms=$response;command_to_overlay_gone_ms=$ended;overlay_observed=$observed;overlay_bounds=$bounds;working_set_before=$before;working_set_peak_observed=$peak;working_set_after=$memory;external_samples=$samples;requested_poll_interval_ms=10;maximum_observation_gap_ms=$maxGap}
 }finally{$process.Dispose()}
}
function Stop-OwnedDaemon {
 # A restart may have replaced the recorded PID. The unique fixture path and
 # exact executable identify its successor; a different daemon is never stopped.
 try {$current=Moon -Words @('query','stats')}catch{return}
 if(![string]::Equals($current.executable,$moon,[StringComparison]::OrdinalIgnoreCase) -or ![string]::Equals($current.config_path,$config,[StringComparison]::OrdinalIgnoreCase)){throw 'A different daemon is running; it was left unchanged.'}
 $closing=[int]$current.process_id
 $null=Moon -Words @('exit')
 Wait-MoonCondition -Description 'owned workspace daemon exit' -Condition {!(Get-Process -Id $closing -ErrorAction SilentlyContinue)}
}
$helper=$null;$owned=$null;$original=@{};$checks=[Collections.Generic.List[string]]::new();$script:lastObserved=$null
$measurements=[Collections.Generic.List[object]]::new();$measurementMonitors=$null
try {
 $start=[Diagnostics.ProcessStartInfo]::new();$start.FileName=$testwin;$start.Arguments='4';$start.UseShellExecute=$false
 $helper=[Diagnostics.Process]::Start($start)
 Wait-MoonCondition -Description 'four owned windows' -Condition {@([MoonCarouselIntegration]::Windows([uint32]$helper.Id,'MoonTestWindow')).Count -eq 4}
 foreach($h in [MoonCarouselIntegration]::Windows([uint32]$helper.Id,'MoonTestWindow')){$original[[string]$h]=[MoonCarouselIntegration]::Frame($h)}
 $startArguments=@('start','--background','--only','testwin','--no-hook','--config',$config)
 if($Measure){$startArguments+='--surfaces'}
 $ready=Moon -Words $startArguments;$owned=[int]$ready.process_id
 Wait-Workspace 4 $false
 Set-MoonTestForeground (Moon -Words @('query','world')).focused $helper.Id
 Grant-MoonTestForeground $helper.Id $owned
 Record 'Workspace motion preserves the ordinary four-window tiled layout'
 $null=Moon -Words @('move-to-workspace','2')
 $null=Moon -Words @('move-to-workspace','2')
 Wait-Workspace 2
 Record 'Two populated workspaces retain two native tiles each'
 $activeObserved=$false
 if($Measure){$measurementMonitors=(Invoke-MoonRpc -Method query -Query world -DevelopmentPath $moon).monitors}
 $switches=if($Measure){@('2','1','2','1','2','1')}else{@('2','1','2','1')}
 foreach($workspace in $switches){
  if($Measure){$sample=Measure-WorkspaceSwitch $workspace;$measurements.Add($sample);$activeObserved=$activeObserved -or $sample.overlay_observed}
  else{$null=Moon -Words @('workspace',$workspace);$activeObserved=$activeObserved -or [bool](Moon -Words @('query','stats')).carousel_active}
  Wait-Workspace 2
 }
 Record 'Whole-workspace switches settle to exact native frames and cloak state'
 $hidden=@(Moon -Words @('query','windows')|Where-Object {!$_.visible})[0]
 Grant-MoonTestForeground $helper.Id $PID
 if(![MoonCarouselIntegration]::SetForegroundWindow([IntPtr][long]$hidden.id)){throw 'Windows refused foreground for the owned hidden-workspace fixture.'}
 Wait-MoonCondition -Description 'external window focus activates its containing workspace' -Condition {(Moon -Words @('query','world')).focused -eq [long]$hidden.id}
 Wait-Workspace 2
 Grant-MoonTestForeground $helper.Id $owned
 Record 'Focusing an owned window activates the workspace containing it'
 $activeBefore=(Moon -Words @('query','world')).active|ConvertTo-Json -Compress
 $null=Moon -Words @('focus','left');$null=Moon -Words @('focus','right')
 if((Moon -Words @('query','stats')).carousel_active -or ((Moon -Words @('query','world')).active|ConvertTo-Json -Compress) -ne $activeBefore){throw 'Focus inside a workspace incorrectly started a workspace transition.'}
 Wait-Workspace 2
 Record 'Focus within one workspace preserves the canvas and has no slide'
 foreach($workspace in @('1','2','1','2','1','2')){$null=Moon -Words @('workspace',$workspace)}
 Wait-Workspace 2
 Record 'Rapid workspace changes settle without stale overlays or lost windows'
 $null=Moon -Words @('workspace','3');Wait-Workspace 0 $false
 $null=Moon -Words @('workspace','1');Wait-Workspace 2
 Record 'Switching to and from an empty workspace restores all native visibility'
 [IO.File]::WriteAllText($config,"config-version=1`nworkspace-animation=false`n",[Text.UTF8Encoding]::new($false))
 $null=Moon -Words @('reload');$null=Moon -Words @('workspace','2')
 if((Moon -Words @('query','stats')).carousel_active){throw 'Disabled workspace animation still presented an overlay.'}
 Wait-Workspace 2
 Record 'Disabling animation retains native tiling and workspace visibility'
 $before=Get-MoonLayoutFingerprint -World (Moon -Words @('query','world')) -Placements @(Moon -Words @('query','windows'))
 $null=Moon -Words @('restart')
 Wait-MoonCondition -Description 'workspace daemon restarts' -Condition {$s=Moon -Words @('query','stats');$s.process_id -ne $owned}
 $owned=[int](Moon -Words @('query','stats')).process_id
 Wait-Workspace 2
 $after=Get-MoonLayoutFingerprint -World (Moon -Words @('query','world')) -Placements @(Moon -Words @('query','windows'))
 if($after -ne $before){throw 'Restart changed workspace membership, ordering, focus or geometry.'}
 Record 'Restart preserves workspace trees, membership, focus and native geometry'
 Stop-OwnedDaemon
 Wait-MoonCondition -Description 'recovery journal closes normally' -Condition {!(Test-Path $journal)}
 foreach($h in [MoonCarouselIntegration]::Windows([uint32]$helper.Id,'MoonTestWindow')){
  if([MoonCarouselIntegration]::Cloak($h) -ne 0){throw 'Exit left an owned window cloaked.'}
  $frame=[MoonCarouselIntegration]::Frame($h)
  foreach($edge in @('Left','Top','Right','Bottom')){if([Math]::Abs($frame.$edge-$original[[string]$h].$edge)-gt 1){throw 'Exit did not restore original owned window frames.'}}
 }
 Record 'Exit restores all original frames and visibility'
 $report=[pscustomobject]@{passed=$true;checks=$checks.ToArray();animation_active_observed=$activeObserved;moon_sha256=(Get-FileHash $moon -Algorithm SHA256).Hash;config=$config;measurement=[pscustomobject]@{enabled=[bool]$Measure;bar_and_borders=[bool]$Measure;monitors=$measurementMonitors;switches=$measurements.ToArray();clock_scope='Direct IPC request through disappearance of the owned overlay HWND; not hook-to-placement or compositor presentation latency'}}
 [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
 [IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 12),[Text.UTF8Encoding]::new($false));$report|ConvertTo-Json -Depth 12
}catch{
 if($script:lastObserved){[IO.File]::WriteAllText(($ReportPath+'.failure.json'),($script:lastObserved|ConvertTo-Json -Depth 30),[Text.UTF8Encoding]::new($false))}
 throw
}finally{
 try {if($owned){Stop-OwnedDaemon}}
 finally {
  try {
   if($helper -and !$helper.HasExited){
    foreach($h in [MoonCarouselIntegration]::Windows([uint32]$helper.Id,'MoonTestWindow')){[void][MoonCarouselIntegration]::PostMessage([IntPtr]$h,0x10,[IntPtr]::Zero,[IntPtr]::Zero)}
    if(!$helper.WaitForExit(5000)){throw 'Owned workspace test windows did not close.'}
   }
  }finally{if($helper){$helper.Dispose()}}
 }
}
