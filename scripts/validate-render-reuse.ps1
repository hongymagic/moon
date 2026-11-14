[CmdletBinding()]
param([string]$BuildDirectory,[string]$OutputDirectory,[switch]$CompileOnly,[switch]$InspectUnderlay)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'install-common.ps1')
Add-Type -AssemblyName System.Drawing,System.Windows.Forms
if (!('MoonRenderReuseNative' -as [type])) {
Add-Type -TypeDefinition @'
using System;using System.Collections.Generic;using System.Runtime.InteropServices;using System.Text;
public static class MoonRenderReuseNative {
 [StructLayout(LayoutKind.Sequential)]public struct Rect{public int Left,Top,Right,Bottom;}
 delegate bool EnumProc(IntPtr h,IntPtr p);
 [DllImport("user32.dll")]static extern bool EnumWindows(EnumProc c,IntPtr p);
 [DllImport("user32.dll")]static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int GetClassName(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")]static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")]public static extern bool IsWindow(IntPtr h);
 [DllImport("user32.dll",SetLastError=true)]public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll",CharSet=CharSet.Unicode,SetLastError=true)]public static extern bool SetWindowText(IntPtr h,string text);
 [DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")]static extern IntPtr GetTopWindow(IntPtr h);
 [DllImport("user32.dll")]static extern IntPtr GetWindow(IntPtr h,uint command);
 [DllImport("user32.dll",EntryPoint="GetWindowLongPtrW")]static extern IntPtr GetWindowLongPtr(IntPtr h,int index);
 [DllImport("user32.dll",SetLastError=true)]static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int width,int height,uint flags);
 [DllImport("user32.dll")]public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")]public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [DllImport("user32.dll",SetLastError=true)]static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll",SetLastError=true)]static extern IntPtr GetDC(IntPtr h);
 [DllImport("user32.dll")]static extern int ReleaseDC(IntPtr h,IntPtr dc);
 [DllImport("gdi32.dll",SetLastError=true)]static extern bool BitBlt(IntPtr destination,int dx,int dy,int width,int height,IntPtr source,int sx,int sy,uint operation);
 [DllImport("dwmapi.dll")]static extern int DwmGetWindowAttribute(IntPtr h,uint a,out Rect r,uint n);
 [DllImport("dwmapi.dll")]static extern int DwmFlush();
 [DllImport("dwmapi.dll",EntryPoint="DwmGetWindowAttribute")]static extern int GetCloak(IntPtr h,uint a,out uint r,uint n);
 public static uint Owner(long h){uint id;GetWindowThreadProcessId(new IntPtr(h),out id);return id;}
 public static bool Underlay(long h){var s=new StringBuilder(256);GetClassName(new IntPtr(h),s,256);return s.ToString()=="moon.surface"&&(GetWindowLongPtr(new IntPtr(h),-20).ToInt64()&0x80020)==0x80020&&IsWindowVisible(new IntPtr(h));}
 public static long Lowest(long[] handles){var remaining=new HashSet<long>(handles);var seen=new HashSet<long>();var current=GetTopWindow(IntPtr.Zero);while(current!=IntPtr.Zero&&seen.Count<8192&&seen.Add(current.ToInt64())){if(remaining.Remove(current.ToInt64())&&remaining.Count==0)return current.ToInt64();current=GetWindow(current,2);}throw new InvalidOperationException("Could not locate every owned helper in the bounded Z-order walk.");}
 static void ValidateInspection(long[] underlays,uint daemon,long[] helpers,uint fixture){foreach(var h in underlays)if(Owner(h)!=daemon||!Underlay(h))throw new InvalidOperationException("Inspection underlay identity changed.");foreach(var h in helpers){var name=new StringBuilder(256);GetClassName(new IntPtr(h),name,256);if(Owner(h)!=fixture||name.ToString()!="MoonTestWindow"||!IsWindowVisible(new IntPtr(h)))throw new InvalidOperationException("Inspection helper identity changed.");}}
 static bool InspectionHeld(long[] underlays,uint daemon,long[] helpers,uint fixture,long lowest,Rect[] original){
  ValidateInspection(underlays,daemon,helpers,fixture);
  if(Lowest(helpers)!=lowest)return false;
  for(int i=0;i<underlays.Length;i++){
   var bounds=Bounds(underlays[i]);var saved=original[i];
   if(bounds.Left!=saved.Left||bounds.Top!=saved.Top||bounds.Right!=saved.Right||bounds.Bottom!=saved.Bottom)throw new InvalidOperationException("Inspection underlay geometry changed.");
   // Every insertion below the helper reverses this small underlay chain.
   long previous=i+1<underlays.Length?underlays[i+1]:lowest;
   if(GetWindow(new IntPtr(underlays[i]),3).ToInt64()!=previous)return false;
  }
  return true;
 }
 public sealed class InspectionCapture {public int Attempts;public bool HeldBeforeCopy,HeldAfterCopy;public long BelowHelper;public int UnderlayCount;public long[] AnchorHelpers;}
 public static InspectionCapture CopyScreenInspected(IntPtr destination,int x,int y,int width,int height,long[] underlays,uint daemon,long[] helpers,uint fixture){
  if(underlays.Length==0||underlays.Length>32||helpers.Length==0||helpers.Length>8)throw new InvalidOperationException("Inspection identity list is outside fixture limits.");
  ValidateInspection(underlays,daemon,helpers,fixture);
  var original=new Rect[underlays.Length];for(int i=0;i<underlays.Length;i++)original[i]=Bounds(underlays[i]);
  for(int attempt=1;attempt<=5;attempt++){
   ValidateInspection(underlays,daemon,helpers,fixture);long lowest=Lowest(helpers);
   foreach(var h in underlays)if(!SetWindowPos(new IntPtr(h),new IntPtr(lowest),0,0,0,0,0x213))throw new System.ComponentModel.Win32Exception();
   // Wait for this caller's queued display updates; not a global session barrier.
   Marshal.ThrowExceptionForHR(DwmFlush());
   if(!InspectionHeld(underlays,daemon,helpers,fixture,lowest,original))continue;
   CopyScreen(destination,x,y,width,height);
   // Check immediately after BitBlt, before PNG encoding or any disk access.
   if(InspectionHeld(underlays,daemon,helpers,fixture,lowest,original))return new InspectionCapture{Attempts=attempt,HeldBeforeCopy=true,HeldAfterCopy=true,BelowHelper=lowest,UnderlayCount=underlays.Length,AnchorHelpers=(long[])helpers.Clone()};
  }
  throw new InvalidOperationException("Inspection Z-order did not remain stable around pixel capture within five attempts; no image was accepted.");
 }
 public static long[] Windows(uint pid,string name){var v=new List<long>();EnumWindows((h,p)=>{var s=new StringBuilder(256);GetClassName(h,s,256);if(Owner(h.ToInt64())==pid&&IsWindowVisible(h)&&s.ToString()==name)v.Add(h.ToInt64());return true;},IntPtr.Zero);return v.ToArray();}
 public static Rect Frame(long h){Rect r;Marshal.ThrowExceptionForHR(DwmGetWindowAttribute(new IntPtr(h),9,out r,16));return r;}
 public static Rect Bounds(long h){Rect r;if(!GetWindowRect(new IntPtr(h),out r))throw new System.ComponentModel.Win32Exception();return r;}
 public static uint Cloak(long h){uint r;Marshal.ThrowExceptionForHR(GetCloak(new IntPtr(h),14,out r,4));return r;}
 public static void CopyScreen(IntPtr destination,int x,int y,int width,int height){var source=GetDC(IntPtr.Zero);if(source==IntPtr.Zero)throw new System.ComponentModel.Win32Exception();try{if(!BitBlt(destination,0,0,width,height,source,x,y,0x40CC0020))throw new System.ComponentModel.Win32Exception();}finally{ReleaseDC(IntPtr.Zero,source);}}
}
'@
}
if($CompileOnly){[pscustomobject]@{native_helpers_compiled=$true;desktop_changed=$false;executables_started=$false}|ConvertTo-Json;return}
if(!$BuildDirectory){throw 'Provide a directory containing the ordinary moon.exe candidate and testwin.exe.'}
$moon=(Resolve-Path -LiteralPath (Join-Path $BuildDirectory 'moon.exe')).Path
$testwin=(Resolve-Path -LiteralPath (Join-Path $BuildDirectory 'testwin.exe')).Path
if([MoonInstallNative]::Integrity() -ne 8192 -or [MoonInstallNative]::TokenWord(26) -ne 0 -or (Test-MoonAdministrator)){
 throw 'Run this isolated visual check with an ordinary medium-integrity token. No elevation is requested.'
}
[xml]$manifest=[MoonInstallNative]::Manifest($moon)
$execution=$manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
if(!$execution -or $execution.GetAttribute('uiAccess') -ne 'false' -or $execution.GetAttribute('level') -ne 'asInvoker'){throw 'This visual check requires the ordinary non-UIAccess candidate.'}
$conflicts=@(Get-Process -Name moon,testwin,glazewm,komorebi,whim,zebar,tacky-borders -ErrorAction SilentlyContinue)
if($conflicts.Count -or (Get-MoonDaemon -DevelopmentPath $moon)){throw 'Stop existing Moon/testwin and other desktop managers before this isolated check.'}
$journal=Join-Path $env:LOCALAPPDATA ('moon\journal-v1-session'+[Diagnostics.Process]::GetCurrentProcess().SessionId+'.json')
if(Test-Path -LiteralPath $journal){throw 'A previous recovery journal must be restored before this check.'}
if(!$OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('moon-dev\render-reuse-'+[guid]::NewGuid().ToString('N'))}
$OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
if((Test-Path -LiteralPath $OutputDirectory) -and (!(Test-Path -LiteralPath $OutputDirectory -PathType Container) -or @(Get-ChildItem -Force -LiteralPath $OutputDirectory).Count)){throw 'Choose an empty output directory; existing evidence is never overwritten.'}
[void][IO.Directory]::CreateDirectory($OutputDirectory)
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-render-reuse-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
$config=Join-Path $temporary 'owned visual fixture.toml'
$utf8=[Text.UTF8Encoding]::new($false)
$helper=$null;$daemon=$null;$helperIdentity=$null;$daemonIdentity=$null;$failure=$null
$daemonStdout=$null;$daemonStderr=$null
$originals=@{};$ownedWindows=@();$retired=@{};$previous=$null;$previousDpi=[IntPtr]::Zero
$expectedPlacementCount=2;$phase='startup';$requireForeground=$false
$cleanupErrors=[Collections.Generic.List[string]]::new()
$checks=[Collections.Generic.List[string]]::new()
$snapshots=[Collections.Generic.List[object]]::new()
$report=[ordered]@{
 started_at=[DateTime]::UtcNow.ToString('o');automated_passed=$false;visual_review_status='pending';
 inspection_underlay_override=[bool]$InspectUnderlay;
 z_order_scope=$(if($InspectUnderlay){'Inspection only: each window crop places owned underlays immediately below that verified target helper; bar crops use the lowest owned helper. Each image records the anchor list and before/after pixel-copy ordering checks. These images do not validate normal product Z-order.'}else{'Unmodified product Z-order; ignored background windows may obscure portions of the underlay.'});
 visual_review_scope='Review the captured rings, corners, focus colors, ellipsis and bundled glyphs; screenshot capture alone is not visual acceptance.';
 moon=[ordered]@{path=$moon;sha256=(Get-FileHash -LiteralPath $moon -Algorithm SHA256).Hash};
 testwin=[ordered]@{path=$testwin;sha256=(Get-FileHash -LiteralPath $testwin -Algorithm SHA256).Hash};
 config=$config;checks=@();snapshots=@();cleanup_errors=@();failure=$null
}
function Save-Report {
 $report.checks=$checks.ToArray();$report.snapshots=$snapshots.ToArray();$report.cleanup_errors=$cleanupErrors.ToArray()
 [IO.File]::WriteAllText((Join-Path $OutputDirectory 'report.json'),($report|ConvertTo-Json -Depth 64),$utf8)
}
function Assert-Process($Process,$Identity) {
 $Process.Refresh();if($Process.HasExited){throw 'An owned process exited unexpectedly.'}
 $current=[MoonInstallNative]::ProcessIdentity([uint32]$Process.Id)
 if($current.Birth -ne $Identity.Birth -or ![string]::Equals($current.Path,$Identity.Path,[StringComparison]::OrdinalIgnoreCase)){throw 'An owned process identity changed.'}
}
function Assert-Window([long]$Handle) {
 Assert-Process $helper $helperIdentity
 if($ownedWindows -notcontains $Handle -or $retired.ContainsKey([string]$Handle) -or
    [MoonRenderReuseNative]::Owner($Handle) -ne $helper.Id -or
    @([MoonRenderReuseNative]::Windows([uint32]$helper.Id,'MoonTestWindow')) -notcontains $Handle){throw 'An owned helper window identity changed.'}
}
function Assert-DaemonIdentity {
 Assert-Process $daemon $daemonIdentity
 $stats=Invoke-MoonRpc query stats -DevelopmentPath $moon
 if($stats.process_id -ne $daemon.Id -or ![string]::Equals($stats.executable,$moon,[StringComparison]::OrdinalIgnoreCase) -or
    ![string]::Equals($stats.config_path,$config,[StringComparison]::OrdinalIgnoreCase)){throw 'The IPC endpoint is not the exact owned daemon.'}
 if($stats.token.ui_access -or $stats.token.integrity_rid -ne 8192){throw 'The owned daemon is not ordinary medium integrity.'}
 return $stats
}
function Assert-Daemon {
 $stats=Assert-DaemonIdentity
 $world=Invoke-MoonRpc query world -DevelopmentPath $moon
 foreach($entry in $world.windows.PSObject.Properties){
  Assert-Window ([long]$entry.Name)
  if($entry.Value.info.process_id -ne $helper.Id){throw 'The isolated daemon admitted another process.'}
 }
 if($stats.last_error){throw "Renderer/daemon reported an error: $($stats.last_error)"}
 if(!$stats.renderer -or $stats.renderer.device_recreation_attempts -ne 0){throw 'Renderer diagnostics are missing or a device was recreated.'}
 return $stats
}
function Invoke-OwnedCommand([string[]]$Commands) {
 $null=Assert-Daemon
 $world=Invoke-MoonRpc query world -DevelopmentPath $moon
 if($world.focused){Assert-Window ([long]$world.focused)}
 if($world.focused){$null=Invoke-MoonRpc -Method command -DevelopmentPath $moon -Commands $Commands -ExpectedFocus ([long]$world.focused)}
 else{$null=Invoke-MoonRpc -Method command -DevelopmentPath $moon -Commands $Commands}
}
function Read-PlacementEvidence {
 $e=[ordered]@{phase=$phase;checked_at=[DateTime]::UtcNow.ToString('o');expected_count=$expectedPlacementCount;require_foreground=$requireForeground;actual_count=$null;count_matches=$false;world_focused=$null;actual_foreground=[MoonRenderReuseNative]::GetForegroundWindow().ToInt64();foreground_matches=$false;daemon_error=$null;renderer=$null;windows=@();error=$null}
 try{
  $stats=Assert-DaemonIdentity
  $e.daemon_error=$stats.last_error;$e.renderer=$stats.renderer
  $world=Invoke-MoonRpc query world -DevelopmentPath $moon;$e.world_focused=$world.focused
  $e.foreground_matches=[bool]($world.focused -and $e.actual_foreground -eq [long]$world.focused)
  $placements=@(Invoke-MoonRpc query windows -DevelopmentPath $moon);$e.actual_count=$placements.Count;$e.count_matches=$placements.Count -eq $expectedPlacementCount
  $rows=[Collections.Generic.List[object]]::new()
  # A failed scope check cannot produce an unbounded dump or expose other apps'
  # titles. Native inspection remains limited to verified fixture HWNDs.
  foreach($p in @($placements|Select-Object -First 8)){
   $row=[ordered]@{id=$p.id;planned_rect=$p.rect;visible=[bool]$p.visible;not_floating=(!$p.floating);not_fullscreen=(!$p.fullscreen);owned=$false;frame=$null;cloak=$null;uncloaked=$false;frame_matches=$false;differences=$null;error=$null}
   try{
    Assert-Window ([long]$p.id);$row.owned=$true
    $f=[MoonRenderReuseNative]::Frame([long]$p.id);$row.frame=$f
    $row.cloak=[MoonRenderReuseNative]::Cloak([long]$p.id);$row.uncloaked=$row.cloak -eq 0
    $differences=[ordered]@{left=($f.Left-$p.rect.x);top=($f.Top-$p.rect.y);width=($f.Right-$f.Left-$p.rect.width);height=($f.Bottom-$f.Top-$p.rect.height)}
    $row.differences=$differences;$row.frame_matches=$true
    foreach($delta in $differences.Values){if([Math]::Abs($delta) -gt 1){$row.frame_matches=$false}}
   }catch{$row.error=$_.Exception.Message}
   $rows.Add([pscustomobject]$row)
  }
  $e.windows=$rows.ToArray()
 }catch{$e.error=$_.Exception.Message}
 return [pscustomobject]$e
}
function Wait-Placement([int]$Count,[bool]$RequireForeground=$true) {
 $script:expectedPlacementCount=$Count;$script:requireForeground=$RequireForeground
 Wait-MoonCondition -Seconds 10 -Description "$Count owned windows with exact native placement" -Condition {
  $null=Assert-Daemon
  $placements=@(Invoke-MoonRpc query windows -DevelopmentPath $moon)
  if($placements.Count -ne $Count){return $false}
  foreach($p in $placements){
   Assert-Window ([long]$p.id)
   if(!$p.visible -or $p.floating -or $p.fullscreen -or [MoonRenderReuseNative]::Cloak([long]$p.id) -ne 0){return $false}
   $f=[MoonRenderReuseNative]::Frame([long]$p.id)
   if([Math]::Abs($f.Left-$p.rect.x)-gt 1 -or [Math]::Abs($f.Top-$p.rect.y)-gt 1 -or
      [Math]::Abs($f.Right-$f.Left-$p.rect.width)-gt 1 -or [Math]::Abs($f.Bottom-$f.Top-$p.rect.height)-gt 1){return $false}
  }
  $world=Invoke-MoonRpc query world -DevelopmentPath $moon
  return !$RequireForeground -or ($world.focused -and [MoonRenderReuseNative]::GetForegroundWindow().ToInt64() -eq [long]$world.focused)
 }
}
function Establish-OwnedFocus {
 $script:phase='establish-owned-focus'
 $world=Invoke-MoonRpc query world -DevelopmentPath $moon
 if(!$world.focused){throw 'The adopted fixture has no managed focus from which to issue a guarded focus command.'}
 Assert-Window ([long]$world.focused)
 $placements=@(Invoke-MoonRpc query windows -DevelopmentPath $moon)
 $current=@($placements|Where-Object{[long]$_.id -eq [long]$world.focused})
 $other=@($placements|Where-Object{[long]$_.id -ne [long]$world.focused})
 if($current.Count -ne 1 -or $other.Count -ne 1){throw 'Expected exactly two owned placements when establishing focus.'}
 Assert-Window ([long]$other[0].id)
 $dx=($other[0].rect.x+$other[0].rect.width/2)-($current[0].rect.x+$current[0].rect.width/2)
 $dy=($other[0].rect.y+$other[0].rect.height/2)-($current[0].rect.y+$current[0].rect.height/2)
 $direction=if([Math]::Abs($dx) -ge [Math]::Abs($dy)){if($dx -gt 0){'right'}else{'left'}}else{if($dy -gt 0){'down'}else{'up'}}
 Invoke-OwnedCommand @('focus '+$direction)
}
function Write-Fixture([bool]$Changed) {
 $theme=if($Changed){'light'}else{'dark'}
 $font=if($Changed){'Arial'}else{'Segoe UI'}
 $size=if($Changed){18}else{13}
 $start=if($Changed){'#25d366'}else{'#ff5c35'}
 $end=if($Changed){'#8e5cff'}else{'#ffc247'}
 $source=@'
config-version=1
theme="THEME"
workspace-animation=false
focus-follows-cursor=false
[bar]
height=40
font="FONT"
font-size=SIZE
widgets=[
 {id="workspaces",format="{workspaces}",align="left"},
 {id="title",format="{title}",align="center",max-width=160},
 {id="fixture",format="Symbols \uf269 \uf1eb",align="right",max-width=220}
]
[borders]
width=4
radius=12
focused="START"
focused-end="END"
unfocused="#68727d"
[[rules]]
commands=["ignore"]
matches=[{process={not-regex="(?i)^testwin(?:\\.exe)?$"}}]
'@
 $source=$source.Replace('THEME',$theme).Replace('FONT',$font).Replace('SIZE',[string]$size).Replace('START',$start).Replace('END',$end)
 [IO.File]::WriteAllText($config,$source,$utf8)
}
function Capture-Rectangle([string]$Name,[int]$Left,[int]$Top,[int]$Width,[int]$Height,[long[]]$InspectionUnderlays=@(),[long[]]$InspectionHelpers=@()) {
 if($Width -lt 1 -or $Height -lt 1 -or $Width -gt 16384 -or $Height -gt 16384 -or [long]$Width*$Height -gt 67108864){throw 'Snapshot dimensions are outside the bounded fixture limits.'}
 $screen=[Windows.Forms.SystemInformation]::VirtualScreen
 $clipped=[Drawing.Rectangle]::Intersect([Drawing.Rectangle]::new($Left,$Top,$Width,$Height),$screen)
 if($clipped.Width -lt 1 -or $clipped.Height -lt 1){throw 'The owned capture rectangle is outside the screen.'}
 $Left=$clipped.Left;$Top=$clipped.Top;$Width=$clipped.Width;$Height=$clipped.Height
 $bitmap=[Drawing.Bitmap]::new($Width,$Height);$graphics=[Drawing.Graphics]::FromImage($bitmap)
 $path=Join-Path $OutputDirectory ($Name+'.png')
 $bracket=$null
 try {
  # SRCCOPY | CAPTUREBLT includes the layered border underlay. Call BitBlt
  # directly because .NET Framework rejects that combined CopyPixelOperation.
  $destination=$graphics.GetHdc()
  try{
   if($InspectUnderlay){
    # Retained process handles bind PID/birth. No RPC is issued between ordering
    # the owned underlay and the native before/after pixel-copy checks.
    Assert-Process $daemon $daemonIdentity;Assert-Process $helper $helperIdentity
    $bracket=[MoonRenderReuseNative]::CopyScreenInspected($destination,$Left,$Top,$Width,$Height,$InspectionUnderlays,[uint32]$daemon.Id,$InspectionHelpers,[uint32]$helper.Id)
   }else{[MoonRenderReuseNative]::CopyScreen($destination,$Left,$Top,$Width,$Height)}
  }finally{$graphics.ReleaseHdc($destination)}
  $bitmap.Save($path,[Drawing.Imaging.ImageFormat]::Png)
 }finally{$graphics.Dispose();$bitmap.Dispose()}
 return [pscustomobject]@{path=$path;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash;left=$Left;top=$Top;width=$Width;height=$Height;inspection_bracket=$bracket}
}
function Capture-State([string]$Name,[int]$Count) {
 $script:phase=$Name
 Wait-Placement $Count
 # Settle before inspection: any later Moon update may restore HWND_BOTTOM.
 # This is not a timing measurement; each inspection copy is separately guarded.
 Start-Sleep -Milliseconds 250
 $stats=Assert-Daemon;$world=Invoke-MoonRpc query world -DevelopmentPath $moon
 if([MoonRenderReuseNative]::GetForegroundWindow().ToInt64() -ne [long]$world.focused){throw 'Foreground left the owned fixture; screenshot capture was refused.'}
 $placements=@(Invoke-MoonRpc query windows -DevelopmentPath $moon)
 $inspected=[Collections.Generic.List[object]]::new()
 $handles=@();$underlays=@()
 if($InspectUnderlay){
  $handles=@($placements|ForEach-Object{Assert-Window ([long]$_.id);[long]$_.id})
  $lowest=[MoonRenderReuseNative]::Lowest([long[]]$handles);Assert-Window $lowest
  foreach($handle in [MoonRenderReuseNative]::Windows([uint32]$daemon.Id,'moon.surface')){
   if(![MoonRenderReuseNative]::Underlay($handle)){continue}
   Assert-Process $daemon $daemonIdentity;Assert-Window $lowest
   $bounds=[MoonRenderReuseNative]::Bounds($handle)
   $inspected.Add([pscustomobject]@{underlay=$handle;daemon_process_id=$daemon.Id;helper_process_id=$helper.Id;bounds=$bounds})
  }
  if(!$inspected.Count){throw 'Inspection requested but no verified owned underlay was found.'}
  $underlays=@($inspected|ForEach-Object{[long]$_.underlay})
 }
 $images=[Collections.Generic.List[object]]::new();$native=[Collections.Generic.List[object]]::new()
 foreach($p in $placements){
  Assert-Window ([long]$p.id)
  $frame=[MoonRenderReuseNative]::Frame([long]$p.id)
  $native.Add([pscustomobject]@{id=$p.id;frame=$frame;cloak=[MoonRenderReuseNative]::Cloak([long]$p.id)})
  # Small margins include the ring and its rounded corners, rather than unrelated desktop regions.
  # Anchor each window crop below its target: an ignored window between the two
  # helpers must not hide this target's ring during explicit pixel inspection.
  $images.Add((Capture-Rectangle ($Name+'-window-'+$p.id) ($frame.Left-16) ($frame.Top-16) ($frame.Right-$frame.Left+32) ($frame.Bottom-$frame.Top+32) $underlays ([long[]]@([long]$p.id))))
 }
 $barIndex=0
 foreach($handle in [MoonRenderReuseNative]::Windows([uint32]$daemon.Id,'moon.surface')){
  if([MoonRenderReuseNative]::Owner($handle) -ne $daemon.Id){throw 'Surface owner changed.'}
  $bounds=[MoonRenderReuseNative]::Bounds($handle)
  if($bounds.Bottom-$bounds.Top -gt 200){continue}
  $images.Add((Capture-Rectangle ($Name+'-bar-'+$barIndex) $bounds.Left $bounds.Top ($bounds.Right-$bounds.Left) ($bounds.Bottom-$bounds.Top) $underlays $handles));$barIndex++
 }
 if(!$barIndex){throw 'No visible owned bar was available to capture.'}
 [IO.File]::Copy($config,(Join-Path $OutputDirectory ($Name+'.toml')),$false)
 $snapshots.Add([pscustomobject]@{name=$Name;stats=$stats;world=$world;native=$native.ToArray();inspection_underlays=$inspected.ToArray();images=$images.ToArray()})
 $checks.Add($Name+' captured with exact owned frames and no renderer error')
 Save-Report
}
function Stop-OwnedDaemon {
 if(!$daemon){return}
 $daemon.Refresh()
 if(!$daemon.HasExited){
  # Cleanup must remain possible after a renderer failure; only ownership is
  # required here, not successful rendering or surviving helper windows.
  $null=Assert-DaemonIdentity
  $null=Invoke-MoonRpc -Method command -DevelopmentPath $moon -Commands @('exit')
  if(!$daemon.WaitForExit(5000)){throw 'Owned daemon did not exit normally; no forced termination was used.'}
 }
 Wait-MoonCondition -Seconds 5 -Description 'owned recovery journal removed' -Condition {!(Test-Path -LiteralPath $journal)}
}
function Check-Restoration {
 foreach($handle in $ownedWindows){
  if($retired.ContainsKey([string]$handle)){continue}
  Assert-Window $handle
  $frame=[MoonRenderReuseNative]::Frame($handle);$original=$originals[[string]$handle]
  if([MoonRenderReuseNative]::Cloak($handle) -ne 0){throw 'An owned helper stayed cloaked after normal exit.'}
  foreach($edge in @('Left','Top','Right','Bottom')){if([Math]::Abs($frame.$edge-$original.$edge)-gt 1){throw 'An owned helper frame was not restored after normal exit.'}}
 }
}
function Close-OwnedHelpers {
 $timer=[Diagnostics.Stopwatch]::StartNew()
 $lastError=''
 while($timer.ElapsedMilliseconds -lt 5000){
  $helper.Refresh();if($helper.HasExited){return}
  try{Assert-Process $helper $helperIdentity}catch{$helper.Refresh();if($helper.HasExited){return};throw}
  # Startup can create another HWND after the previous enumeration. Keep
  # enumerating only this retained process until it exits or the deadline ends.
  foreach($handle in [MoonRenderReuseNative]::Windows([uint32]$helper.Id,'MoonTestWindow')){
   if([MoonRenderReuseNative]::Owner($handle) -ne $helper.Id){$lastError='A helper HWND changed owner during cleanup.';continue}
   if(![MoonRenderReuseNative]::PostMessage([IntPtr]$handle,0x10,[IntPtr]::Zero,[IntPtr]::Zero)){$lastError='Windows refused an owned helper close.'}
  }
  $remaining=5000-$timer.ElapsedMilliseconds
  if($remaining -gt 0 -and $helper.WaitForExit([int][Math]::Min(50,$remaining))){return}
 }
 $helper.Refresh();if($helper.HasExited){return}
 throw "Owned helper process did not close normally within five seconds. $lastError No forced termination was used."
}
try {
 Save-Report
 $previousDpi=[MoonRenderReuseNative]::SetThreadDpiAwarenessContext([IntPtr](-4))
 if($previousDpi -eq [IntPtr]::Zero){throw 'Physical-pixel capture requires per-monitor DPI awareness.'}
 $foreground=[MoonRenderReuseNative]::GetForegroundWindow().ToInt64()
 if($foreground){try{$owner=[MoonRenderReuseNative]::Owner($foreground);$identity=[MoonInstallNative]::ProcessIdentity($owner);$previous=[pscustomobject]@{handle=$foreground;process_id=$owner;identity=$identity}}catch{}}
 Write-Fixture $false
 $start=[Diagnostics.ProcessStartInfo]::new();$start.FileName=$testwin;$start.Arguments='2';$start.UseShellExecute=$false;$start.CreateNoWindow=$true
 $helper=[Diagnostics.Process]::Start($start);$null=$helper.Handle
 $helperIdentity=[MoonInstallNative]::ProcessIdentity([uint32]$helper.Id)
 $report.helper=[ordered]@{process_id=$helper.Id;birth=$helperIdentity.Birth;path=$helperIdentity.Path;windows=@();original_frames=@{}}
 Save-Report
 if(![string]::Equals($helperIdentity.Path,$testwin,[StringComparison]::OrdinalIgnoreCase)){throw 'Helper image differs from the requested candidate.'}
 Wait-MoonCondition -Seconds 10 -Description 'exactly two owned helper windows' -Condition {@([MoonRenderReuseNative]::Windows([uint32]$helper.Id,'MoonTestWindow')).Count -eq 2}
 $ownedWindows=@([MoonRenderReuseNative]::Windows([uint32]$helper.Id,'MoonTestWindow'))
 foreach($handle in $ownedWindows){Assert-Window $handle;$originals[[string]$handle]=[MoonRenderReuseNative]::Frame($handle);if(![MoonRenderReuseNative]::SetWindowText([IntPtr]$handle,'Moon renderer fixture - a deliberately long title for ellipsis and font validation')){throw 'Unable to name the owned fixture.'}}
 $report.helper=[ordered]@{process_id=$helper.Id;birth=$helperIdentity.Birth;path=$helperIdentity.Path;windows=$ownedWindows;original_frames=$originals}
 $words=@('start','--only','testwin','--no-hook','--surfaces','--config',$config)
 $daemonStart=[Diagnostics.ProcessStartInfo]::new();$daemonStart.FileName=$moon
 $daemonStart.Arguments=($words|ForEach-Object{ConvertTo-MoonArgument $_}) -join ' '
 $daemonStart.UseShellExecute=$false;$daemonStart.CreateNoWindow=$true
 $daemonStart.RedirectStandardOutput=$true;$daemonStart.RedirectStandardError=$true
 $daemon=[Diagnostics.Process]::Start($daemonStart)
 $daemonStdout=$daemon.StandardOutput.ReadToEndAsync();$daemonStderr=$daemon.StandardError.ReadToEndAsync()
 $null=$daemon.Handle;$daemonIdentity=[MoonInstallNative]::ProcessIdentity([uint32]$daemon.Id)
 $report.daemon=[ordered]@{process_id=$daemon.Id;birth=$daemonIdentity.Birth;path=$daemonIdentity.Path}
 Save-Report
 Wait-MoonCondition -Seconds 10 -Description 'the exact owned daemon' -Condition {$null=Assert-Daemon;return $true}
 # Adoption preserves the user's current foreground when it is outside this
 # fixture. Verify frames first, then deliberately focus an owned neighbor.
 $phase='startup-adoption';Wait-Placement 2 $false
 Establish-OwnedFocus
 Capture-State '01-initial' 2
 Invoke-OwnedCommand @('resize width +12%','resize height -8%','focus left')
 $oldFocus=[long](Invoke-MoonRpc query world -DevelopmentPath $moon).focused
 Invoke-OwnedCommand @('focus right')
 Wait-MoonCondition -Seconds 5 -Description 'focus changed between the two owned windows' -Condition {[long](Invoke-MoonRpc query world -DevelopmentPath $moon).focused -ne $oldFocus}
 Capture-State '02-resize-focus' 2
 Write-Fixture $true;Invoke-OwnedCommand @('reload')
 Wait-MoonCondition -Seconds 5 -Description 'fixture font and gradient reload' -Condition {$c=(Invoke-MoonRpc query world -DevelopmentPath $moon).config;return $c.theme -eq 'light' -and $c.bar.font -eq 'Arial' -and $c.bar.'font-size' -eq 18 -and $c.borders.focused -eq '#25d366' -and $c.borders.'focused-end' -eq '#8e5cff'}
 Capture-State '03-theme-font' 2
 $closing=[long](Invoke-MoonRpc query world -DevelopmentPath $moon).focused;Assert-Window $closing
 Invoke-OwnedCommand @('close')
 Wait-MoonCondition -Seconds 5 -Description 'the selected owned helper closes' -Condition {![MoonRenderReuseNative]::IsWindow([IntPtr]$closing)}
 $retired[[string]$closing]=$true
 Capture-State '04-slot-removal' 1
 Write-Fixture $false;Invoke-OwnedCommand @('reload')
 Wait-MoonCondition -Seconds 5 -Description 'baseline font and gradient restored' -Condition {$c=(Invoke-MoonRpc query world -DevelopmentPath $moon).config;return $c.theme -eq 'dark' -and $c.bar.font -eq 'Segoe UI' -and $c.bar.'font-size' -eq 13 -and $c.borders.focused -eq '#ff5c35' -and $c.borders.'focused-end' -eq '#ffc247'}
 Capture-State '05-restored-style' 1
 Stop-OwnedDaemon
 Wait-MoonCondition -Seconds 5 -Description 'surviving original frame restored' -Condition {Check-Restoration;return $true}
 $checks.Add('Normal daemon exit restored the surviving original helper frame and visibility')
 $report.automated_passed=$true
}catch{
 $failure=$_;$report.failure=$_.Exception.Message
 if($daemon){
  try{$report.failure_state=Read-PlacementEvidence;Save-Report}catch{$report.failure_evidence_error=$_.Exception.Message}
 }
}
finally {
 try{Stop-OwnedDaemon}catch{$cleanupErrors.Add($_.Exception.Message)}
 try{
  if($helper -and !$helper.HasExited){
   if($daemon -and !$daemon.HasExited){throw 'Owned daemon is still running; helper windows were retained for recovery.'}
   Assert-Process $helper $helperIdentity
   try{Check-Restoration}catch{$cleanupErrors.Add($_.Exception.Message)}
   Close-OwnedHelpers
  }
 }catch{$cleanupErrors.Add($_.Exception.Message)}
 if($previous){try{if([MoonRenderReuseNative]::Owner($previous.handle) -eq $previous.process_id){$now=[MoonInstallNative]::ProcessIdentity($previous.process_id);if($now.Birth -eq $previous.identity.Birth -and [string]::Equals($now.Path,$previous.identity.Path,[StringComparison]::OrdinalIgnoreCase)){[void][MoonRenderReuseNative]::SetForegroundWindow([IntPtr]$previous.handle)}}}catch{}}
 if($previousDpi -ne [IntPtr]::Zero){[void][MoonRenderReuseNative]::SetThreadDpiAwarenessContext($previousDpi)}
 $report.cleanup=[ordered]@{daemon_exited=(!$daemon -or $daemon.HasExited);helper_exited=(!$helper -or $helper.HasExited);journal_absent=!(Test-Path -LiteralPath $journal)}
 foreach($stream in @(@{name='stdout';task=$daemonStdout},@{name='stderr';task=$daemonStderr})){
  if($stream.task){
   try{
    if(!$stream.task.Wait(2000)){throw "Owned daemon $($stream.name) remained open after cleanup."}
    [IO.File]::WriteAllText((Join-Path $OutputDirectory ('daemon.'+$stream.name+'.log')),$stream.task.Result,$utf8)
   }catch{$cleanupErrors.Add($_.Exception.Message)}
  }
 }
 if($daemon){$daemon.Dispose()};if($helper){$helper.Dispose()}
 if($cleanupErrors.Count){$report.automated_passed=$false}
 $report.finished_at=[DateTime]::UtcNow.ToString('o')
 try{Save-Report}catch{if(!$failure){$failure=$_};Write-Warning "Unable to write renderer validation report: $($_.Exception.Message)"}
 if(!$cleanupErrors.Count){try{Remove-Item -LiteralPath $temporary -Recurse -Force}catch{Write-Warning "Owned temporary fixture retained: $temporary"}}
}
$report|ConvertTo-Json -Depth 64
if($failure){throw $failure}
if($cleanupErrors.Count){throw ($cleanupErrors -join ' ')}
