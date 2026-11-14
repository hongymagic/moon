[CmdletBinding()]
param(
    [string]$ReportPath,
    [int]$ExpectedDaemonId,
    [string]$ExpectedDaemonPath,
    [switch]$DiscoverOnly,
    [switch]$AllowOwnedFixtureTermination,
    [switch]$CompileOnly
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'install-common.ps1')
& (Join-Path $PSScriptRoot 'application-integration.ps1') -CompileOnly | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class MoonPaintDialogNative {
 [StructLayout(LayoutKind.Sequential)] public struct Point {public int X,Y;}
 [StructLayout(LayoutKind.Sequential)] struct Mouse {public int X,Y;public uint Data,Flags,Time;public UIntPtr Extra;}
 [StructLayout(LayoutKind.Explicit)] struct InputUnion {[FieldOffset(0)]public Mouse Mouse;}
 [StructLayout(LayoutKind.Sequential)] struct Input {public uint Type;public InputUnion Value;}
 public sealed class SavedPointer {public int X,Y,TargetX,TargetY;}
 [StructLayout(LayoutKind.Sequential)] struct LastInput {public uint Size,Time;}
 [DllImport("user32.dll")] static extern bool GetLastInputInfo(ref LastInput info);
 public static uint LastInputTime(){var info=new LastInput{Size=(uint)Marshal.SizeOf(typeof(LastInput))};if(!GetLastInputInfo(ref info))throw new System.ComponentModel.Win32Exception();return info.Time;}
 [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
 [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr window,uint flags);
 [DllImport("user32.dll")] static extern short GetAsyncKeyState(int key);
 [DllImport("user32.dll")] static extern bool GetCursorPos(out Point point);
 [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] static extern int GetSystemMetrics(int index);
 [DllImport("user32.dll",SetLastError=true)] static extern uint SendInput(uint count,Input[] input,int size);
 public static int InputSize(){return Marshal.SizeOf(typeof(Input));}
 public static SavedPointer ClickObservedButton(long window,int x,int y) {
  if(GetForegroundWindow().ToInt64()!=window || GetAncestor(WindowFromPoint(new Point{X=x,Y=y}),2).ToInt64()!=window)throw new InvalidOperationException("The observed control is no longer in the foreground owned Paint window.");
  foreach(int key in new int[]{1,2,4,5,6,16,17,18,91,92})if((GetAsyncKeyState(key)&0x8000)!=0)throw new InvalidOperationException("A physical input is already held; no click was sent.");
  Point previous;if(!GetCursorPos(out previous))throw new System.ComponentModel.Win32Exception();
  int left=GetSystemMetrics(76),top=GetSystemMetrics(77),width=GetSystemMetrics(78),height=GetSystemMetrics(79);
  if(width<2||height<2||x<left||x>=left+width||y<top||y>=top+height)throw new InvalidOperationException("Observed button center is outside the physical desktop.");
  var input=new Input[]{new Input{Value=new InputUnion{Mouse=new Mouse{X=(int)Math.Round((x-left)*65535.0/(width-1)),Y=(int)Math.Round((y-top)*65535.0/(height-1)),Flags=0xC001}}},new Input{Value=new InputUnion{Mouse=new Mouse{Flags=2}}},new Input{Value=new InputUnion{Mouse=new Mouse{Flags=4}}}};
  uint sent=SendInput(3,input,InputSize());
  if(sent!=3){if(sent==2)SendInput(1,new Input[]{input[2]},InputSize());throw new System.ComponentModel.Win32Exception();}
  return new SavedPointer{X=previous.X,Y=previous.Y,TargetX=x,TargetY=y};
 }
 public static bool RestorePointer(SavedPointer saved) {
  Point current;if(!GetCursorPos(out current)||current.X!=saved.TargetX||current.Y!=saved.TargetY)return false;
  foreach(int key in new int[]{1,2,4,5,6})if((GetAsyncKeyState(key)&0x8000)!=0)return false;
  return SetCursorPos(saved.X,saved.Y);
 }

 [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr window,uint command);
 [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr window);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
 public static bool IsOwnedPopup(long window,long owner) {
  for(int i=0;i<8 && window!=0;i++) { if(window==owner)return true;window=GetWindow(new IntPtr(window),4).ToInt64(); }
  return false;
 }
}
'@
if($CompileOnly){if([MoonPaintDialogNative]::InputSize() -ne 40){throw 'Unexpected x64 mouse INPUT size.'};[pscustomobject]@{compiled=$true;mouse_input_size=40;desktop_changed=$false}|ConvertTo-Json;return}
if(!$ReportPath){$ReportPath=Join-Path $env:LOCALAPPDATA 'moon-dev\paint-dialog-results.json'}
if((Test-MoonAdministrator) -or [MoonInstallNative]::Integrity() -ne 8192){throw 'Run the Paint dialog scenario from ordinary medium-integrity PowerShell.'}
if(@(Get-Process -Name mspaint -ErrorAction SilentlyContinue).Count){throw 'Paint is already running; existing documents were not touched.'}
if(!$ExpectedDaemonId -or !$ExpectedDaemonPath){throw 'Provide the exact running daemon process ID and executable path for this coordinated desktop scenario.'}
$baselineStats=Invoke-MoonRpc query stats -DevelopmentPath $ExpectedDaemonPath
if($baselineStats.process_id -ne $ExpectedDaemonId -or ![string]::Equals($baselineStats.executable,$ExpectedDaemonPath,[StringComparison]::OrdinalIgnoreCase)){throw 'The running daemon differs from the explicitly coordinated process/path.'}
$baselineWorld=Invoke-MoonRpc query world -DevelopmentPath $ExpectedDaemonPath
if($baselineWorld.paused -or $baselineWorld.mode -ne 'default' -or $baselineWorld.native_fullscreen){throw 'The existing Moon session must be active, in its default mode and outside fullscreen.'}
$baselinePlacements=@(Invoke-MoonRpc query windows -DevelopmentPath $ExpectedDaemonPath)
$configHashBefore=if(Test-Path -LiteralPath $baselineStats.config_path){(Get-FileHash -LiteralPath $baselineStats.config_path -Algorithm SHA256).Hash}else{$null}
$baselineFingerprint=Get-MoonLayoutFingerprint -World $baselineWorld -Placements $baselinePlacements
$baselineIdentities=@($baselineWorld.windows.PSObject.Properties | ForEach-Object {[pscustomobject]@{id=[long]$_.Name;process_id=$_.Value.info.process_id;birth=[MoonApplicationNative]::Birth([uint32]$_.Value.info.process_id)}})
$initialWorkspace=@($baselineWorld.workspaces | Where-Object {$_.id -eq $baselineWorld.active.PSObject.Properties[[string]$baselineWorld.active_monitor].Value})
if($initialWorkspace.Count -ne 1){throw 'The original active workspace is ambiguous.'}
# Check membership outside the nested pipeline so its $_ cannot shadow the workspace.
$emptyWorkspace=@($baselineWorld.workspaces | Where-Object {$_.monitor -eq $initialWorkspace[0].monitor -and $_.id -ne $initialWorkspace[0].id} | Where-Object {
 $workspaceId=$_.id
 @($baselineWorld.windows.PSObject.Properties | Where-Object {$_.Value.workspace -eq $workspaceId}).Count -eq 0
})
if(!$DiscoverOnly -and !$emptyWorkspace.Count){throw 'The dialog visibility scenario needs an unused workspace on the active monitor.'}
$package=Get-AppxPackage -Name Microsoft.Paint
if(!$package){throw 'Microsoft Paint is not installed.'}
$manifest=$package|Get-AppxPackageManifest
$paintPath=(Resolve-Path -LiteralPath (Join-Path $package.InstallLocation $manifest.Package.Applications.Application.Executable)).ProviderPath
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-paint-dialog-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
$bitmap=Join-Path $temporary ('moon-owned-dialog-'+[guid]::NewGuid().ToString('N')+'.bmp')
Copy-Item -LiteralPath (Join-Path $package.InstallLocation 'Assets\NewBitmapImage.bmp') -Destination $bitmap
$bitmapHash=(Get-FileHash -LiteralPath $bitmap -Algorithm SHA256).Hash
$documentName=[IO.Path]::GetFileNameWithoutExtension($bitmap)
$previous=[MoonElevatedNative]::GetForegroundWindow().ToInt64()
$previousPid=[MoonApplicationNative]::ProcessId($previous)
$previousBirth=[MoonApplicationNative]::Birth($previousPid)
[void][MoonElevatedNative]::SetThreadDpiAwarenessContext([IntPtr](-4))
$owned=$null;$actualProcess=$null;$launcher=$null;$failure=$null;$dialogWasOpened=$false;$switched=$false;$savedPointer=$null;$lastOwnedInput=[MoonPaintDialogNative]::LastInputTime()
$checks=[Collections.Generic.List[object]]::new();$cleanupErrors=[Collections.Generic.List[string]]::new()
$report=[ordered]@{version=1;uia_worker_timeout_seconds=8;original_config_sha256=$configHashBefore;started_at=[DateTime]::UtcNow.ToString('o');passed=$false;discover_only=[bool]$DiscoverOnly;daemon=$baselineStats;package=[pscustomobject]@{name=$package.Name;version=$package.Version.ToString();image=$paintPath};owned_document=$bitmap;original_document_sha256=$bitmapHash;temporary_directory=$temporary;baseline_fingerprint=$baselineFingerprint}
$report.original_foreground=[pscustomobject]@{id=$previous;process_id=$previousPid;birth=$previousBirth}
function Save-Report {
 [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
 [IO.File]::WriteAllText($ReportPath,($report|ConvertTo-Json -Depth 50),[Text.UTF8Encoding]::new($false))
}
function Assert-Daemon {
 $stats=Invoke-MoonRpc query stats -DevelopmentPath $ExpectedDaemonPath
 if($stats.process_id -ne $baselineStats.process_id -or ![string]::Equals($stats.executable,$baselineStats.executable,[StringComparison]::OrdinalIgnoreCase)){throw 'The running daemon identity changed; no command was sent.'}
 return $stats
}
function Assert-Owned {
 if(!$owned -or !$actualProcess){throw 'No Paint window has been proved to belong to this test.'}
 $actualProcess.Refresh()
 if($actualProcess.HasExited -or ![MoonElevatedNative]::IsWindow([IntPtr][long]$owned.id) -or [MoonApplicationNative]::ProcessId($owned.id) -ne $owned.process_id -or [MoonApplicationNative]::Birth([uint32]$owned.process_id) -ne $owned.birth -or ![string]::Equals([MoonApplicationNative]::ImagePath([uint32]$owned.process_id),$paintPath,[StringComparison]::OrdinalIgnoreCase)){throw 'The owned Paint identity changed.'}
}
function Invoke-PaintUia([string]$Action,[object[]]$RuntimeId=@()) {
 Assert-Owned
 $requestPath=Join-Path $temporary ('uia-'+$Action+'-'+[guid]::NewGuid().ToString('N')+'.json')
 $request=[ordered]@{version=1;action=$Action;owned=$owned;runtime_id=@($RuntimeId)}
 [IO.File]::WriteAllText($requestPath,($request|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
 $report.last_uia_request=$requestPath;Save-Report
 $worker=Join-Path $PSScriptRoot 'paint-dialog-uia-worker.ps1'
 $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
 $responsePath=$requestPath+'.response.json';$watch=[Diagnostics.Stopwatch]::StartNew()
 try {
  $response=Invoke-MoonNative -Path $powershell -Arguments @('-NoProfile','-NonInteractive','-Mta','-ExecutionPolicy','Bypass','-File',$worker,'-RequestPath',$requestPath) -TimeoutSeconds 8
  $parsed=$response.Output|ConvertFrom-Json
  [IO.File]::WriteAllText($responsePath,($parsed|ConvertTo-Json -Depth 12),[Text.UTF8Encoding]::new($false))
  return $parsed
 }catch {
  [IO.File]::WriteAllText($responsePath,([pscustomobject]@{error=$_.Exception.Message}|ConvertTo-Json),[Text.UTF8Encoding]::new($false));throw
 }finally{$watch.Stop();$report.last_uia_response=[pscustomobject]@{path=$responsePath;elapsed_ms=$watch.Elapsed.TotalMilliseconds};Save-Report}
}
function Capture-Tree([string]$Name) {
 $response=Invoke-PaintUia 'Inspect'
 [IO.File]::WriteAllText((Join-Path $temporary ($Name+'.json')),($response|ConvertTo-Json -Depth 10),[Text.UTF8Encoding]::new($false))
 return @($response.elements)
}
function Find-Dialog {
 $response=Invoke-PaintUia 'FindDialog'
 return $response.dialog
}
function Cancel-Dialog {
 $expected=@()
 if($report.Contains('dialog')){$expected=@($report.dialog.runtime_id)}
 $null=Invoke-PaintUia 'CancelDialog' -RuntimeId $expected
 Wait-MoonCondition -Description 'the owned WinUI colour dialog to cancel' -Seconds 5 -Condition {! (Find-Dialog)}
}
function Assert-Placement([bool]$Floating) {
 Wait-MoonCondition -Description 'Paint native frame and planned placement agree' -Seconds 5 -Condition {
  Assert-Owned
  $placements=@(Invoke-MoonRpc query windows -DevelopmentPath $ExpectedDaemonPath | Where-Object {$_.id -eq $owned.id})
  if($placements.Count -ne 1 -or !$placements[0].visible -or $placements[0].floating -ne $Floating -or [MoonApplicationNative]::Cloaked($owned.id) -ne 0){return $false}
  $r=$placements[0].rect;$f=[MoonElevatedNative]::Frame($owned.id)
  return [Math]::Abs($f.Left-$r.x) -le 1 -and [Math]::Abs($f.Top-$r.y) -le 1 -and [Math]::Abs($f.Right-$f.Left-$r.width) -le 1 -and [Math]::Abs($f.Bottom-$f.Top-$r.height) -le 1
 }
}
function Record([string]$Name) {$checks.Add([pscustomobject]@{check=$Name;passed=$true});$report.checks=$checks.ToArray();Save-Report}
function Capture-Owned([string]$Name) {
 Assert-Owned
 if([MoonApplicationNative]::ProcessId([MoonElevatedNative]::GetForegroundWindow().ToInt64()) -ne $owned.process_id){throw 'The owned application is not foreground; its screenshot was not captured.'}
 $frame=[MoonElevatedNative]::Frame($owned.id)
 if($frame.Right-$frame.Left -lt 1 -or $frame.Bottom-$frame.Top -lt 1 -or $frame.Right-$frame.Left -gt 16384 -or $frame.Bottom-$frame.Top -gt 16384){throw 'The owned screenshot dimensions are invalid.'}
 $image=[Drawing.Bitmap]::new($frame.Right-$frame.Left,$frame.Bottom-$frame.Top);$graphics=[Drawing.Graphics]::FromImage($image)
 try {$graphics.CopyFromScreen($frame.Left,$frame.Top,0,0,$image.Size);$image.Save((Join-Path $temporary ($Name+'.png')),[Drawing.Imaging.ImageFormat]::Png)} finally {$graphics.Dispose();$image.Dispose()}
}
try {
 Save-Report
 $launchedAfter=[DateTime]::UtcNow.AddSeconds(-1).ToFileTimeUtc()
 $start=[Diagnostics.ProcessStartInfo]::new();$start.FileName=$paintPath;$start.Arguments=ConvertTo-MoonArgument $bitmap;$start.UseShellExecute=$false
 $launcher=[Diagnostics.Process]::Start($start)
 Wait-MoonCondition -Description 'the uniquely named new Paint document' -Seconds 20 -Condition {
  $found=@([MoonApplicationNative]::Windows() | Where-Object {$_.Title.Contains($documentName) -and [string]::Equals([MoonApplicationNative]::ImagePath($_.ProcessId),$paintPath,[StringComparison]::OrdinalIgnoreCase)})
  if($found.Count -ne 1){return $false}
  $birth=[MoonApplicationNative]::Birth($found[0].ProcessId)
  if($birth -lt $launchedAfter){throw 'Paint reused an older process; the test did not acquire ownership.'}
  $script:actualProcess=Get-Process -Id $found[0].ProcessId;[void]$script:actualProcess.Handle
  $script:owned=[pscustomobject]@{id=$found[0].Id;process_id=$found[0].ProcessId;birth=$birth;path=$paintPath;title=$found[0].Title;class=$found[0].Class;integrity_rid=[MoonElevatedNative]::Integrity($found[0].ProcessId)}
  return $true
 }
 $report.owned=$owned;Save-Report
 if($owned.integrity_rid -ne 8192){throw 'The newly owned Paint process is not medium integrity.'}
 Wait-MoonCondition -Description 'Paint to load actual WinUI3 and join Moon' -Seconds 15 -Condition {
  Assert-Owned
  $module=@($actualProcess.Modules | Where-Object {$_.ModuleName -eq 'Microsoft.UI.Xaml.dll' -and $_.FileVersionInfo.FileMajorPart -eq 3})
  $state=(Invoke-MoonRpc query world -DevelopmentPath $ExpectedDaemonPath).windows.PSObject.Properties[[string]$owned.id]
  if($module.Count -ne 1 -or !$state){return $false}
  $script:originalFloating=[bool]$state.Value.floating;$script:paintWorkspace=$state.Value.workspace
  $report.winui_module=[pscustomobject]@{path=$module[0].FileName;version=$module[0].FileVersionInfo.FileVersion;major=3}
  return $true
 }
 if($paintWorkspace -ne $initialWorkspace[0].id){throw 'Paint was assigned to another workspace; existing configuration was not changed.'}
 Wait-MoonCondition -Description 'the owned Paint foreground' -Seconds 5 -Condition {[MoonApplicationNative]::ProcessId([MoonElevatedNative]::GetForegroundWindow().ToInt64()) -eq $owned.process_id}
 $report.initial_tree=@(Capture-Tree 'before');Capture-Owned 'before'
 Record 'A uniquely owned medium-integrity Paint document uses Microsoft.UI.Xaml3'
 if(!$DiscoverOnly){
  $buttons=@($report.initial_tree | Where-Object {$_.type -eq 'ControlType.Button' -and $_.name -match '^Edit colou?rs(?:\s*\([^)]*\))?$' -and $_.enabled -and !$_.offscreen})
  if($buttons.Count -ne 1){throw 'The current owned Paint UI has no unique Edit colors button; inspect the captured tree before adapting the selector.'}
  $observed=Invoke-PaintUia 'LocateEditColours' -RuntimeId @($buttons[0].runtime_id)
  $report.open_control=$observed.control;Save-Report;Assert-Owned
  $r=$observed.control.rect
  if($r.Count -ne 4 -or $r[2] -le 0 -or $r[3] -le 0){throw 'The observed Edit colours bounds are invalid.'}
  $savedPointer=[MoonPaintDialogNative]::ClickObservedButton($owned.id,[int][Math]::Floor($r[0]+$r[2]/2),[int][Math]::Floor($r[1]+$r[3]/2));$dialogWasOpened=$true
  $lastOwnedInput=[MoonPaintDialogNative]::LastInputTime();$report.open_method='Single mouse click at the freshly observed UIA button center, with foreground/hit-test/held-input guards';Save-Report
  Wait-MoonCondition -Description 'the observed owned WinUI colour dialog' -Seconds 5 -Condition {$null -ne (Find-Dialog)}
  $report.dialog=Find-Dialog;$dialogRuntime=($report.dialog.runtime_id -join '-');Save-Report;Capture-Owned 'dialog'
  Record 'Edit colours opens the observed WinUI3 dialog with its scoped spectrum and OK/Cancel controls'
  $null=Assert-Daemon
  $null=Invoke-MoonRpc command -DevelopmentPath $ExpectedDaemonPath -Commands @('set-floating') -ExpectedFocus $owned.id
  Assert-Placement $true
  if(!(Find-Dialog)){throw 'Floating the owner lost the active WinUI colour dialog.'}
  Record 'The dialog remains accessible when its owner floats at its exact native frame'
  $null=Invoke-MoonRpc command -DevelopmentPath $ExpectedDaemonPath -Commands @('workspace '+$emptyWorkspace[0].name) -ExpectedFocus $owned.id;$switched=$true
  Wait-MoonCondition -Description 'Paint and its dialog to leave the visible workspace' -Seconds 5 -Condition {
   $placement=@(Invoke-MoonRpc query windows -DevelopmentPath $ExpectedDaemonPath | Where-Object {$_.id -eq $owned.id})
   $f=[MoonElevatedNative]::Frame($owned.id)
   $placement.Count -eq 1 -and !$placement[0].visible -and ([MoonApplicationNative]::Cloaked($owned.id) -ne 0 -or $f.Left -le -30000)
  }
  $report.hidden_native_windows=@([MoonApplicationNative]::Windows() | Where-Object {$_.ProcessId -eq $owned.process_id -and [MoonPaintDialogNative]::IsOwnedPopup($_.Id,$owned.id)} | ForEach-Object {[pscustomobject]@{id=$_.Id;class=$_.Class;cloaked=[MoonApplicationNative]::Cloaked($_.Id);frame=[MoonElevatedNative]::Frame($_.Id)}})
  if(@($report.hidden_native_windows | Where-Object {$_.cloaked -eq 0 -and $_.frame.Left -gt -30000}).Count){throw 'An owned Paint popup remained uncloaked on the inactive workspace.'}
  $null=Assert-Daemon;$null=Invoke-MoonRpc command -DevelopmentPath $ExpectedDaemonPath -Commands @('workspace '+$initialWorkspace[0].name);$switched=$false
  Assert-Placement $true
  Wait-MoonCondition -Description 'the same owned dialog to return' -Seconds 5 -Condition {$null -ne (Find-Dialog)}
  if(((Find-Dialog).runtime_id -join '-') -ne $dialogRuntime){throw 'The original WinUI colour dialog accessibility identity did not survive the workspace round trip.'}
  Capture-Owned 'restored-dialog'
  Record 'Workspace hide/show restores the owner and its live WinUI colour dialog'
  Cancel-Dialog;$dialogWasOpened=$false
  $null=Invoke-MoonRpc command -DevelopmentPath $ExpectedDaemonPath -Commands @('set-tiling') -ExpectedFocus $owned.id
  Assert-Placement $false
  Record 'Cancel dismisses the dialog and tiling resumes with exact geometry'
  if($originalFloating){$null=Invoke-MoonRpc command -DevelopmentPath $ExpectedDaemonPath -Commands @('set-floating') -ExpectedFocus $owned.id;Assert-Placement $true}
 }
 if((Get-FileHash -LiteralPath $bitmap -Algorithm SHA256).Hash -ne $bitmapHash){throw 'The owned bitmap changed unexpectedly.'}
 Record 'The temporary document bytes remain unchanged'
} catch {$failure=$_;$report.error=$failure.Exception.Message;if($owned){try{$report.failure_native=@([MoonApplicationNative]::Windows()|Where-Object {$_.ProcessId -eq $owned.process_id}|ForEach-Object {[pscustomobject]@{id=$_.Id;process_id=$_.ProcessId;class=$_.Class;title=$_.Title;owner=[MoonPaintDialogNative]::GetWindow([IntPtr][long]$_.Id,4).ToInt64();enabled=[MoonPaintDialogNative]::IsWindowEnabled([IntPtr][long]$_.Id);frame=[MoonElevatedNative]::Frame($_.Id);cloaked=[MoonApplicationNative]::Cloaked($_.Id)}});$report.failure_world=Invoke-MoonRpc query world -DevelopmentPath $ExpectedDaemonPath;$report.failure_stats=Invoke-MoonRpc query stats -DevelopmentPath $ExpectedDaemonPath}catch{$report.snapshot_error=$_.Exception.Message};try{Capture-Owned 'failure'}catch{$report.failure_screenshot_error=$_.Exception.Message}};Save-Report}
finally {
 if($owned){
  try {
   Assert-Owned
   if($switched){$null=Assert-Daemon;$null=Invoke-MoonRpc command -DevelopmentPath $ExpectedDaemonPath -Commands @('workspace '+$initialWorkspace[0].name);$switched=$false;Start-Sleep -Milliseconds 300}
   if($dialogWasOpened){try{Cancel-Dialog}catch{$cleanupErrors.Add('Dialog cancellation failed: '+$_.Exception.Message)}}
   $window=@([MoonApplicationNative]::Windows() | Where-Object {$_.Id -eq $owned.id})
   if($window.Count -ne 1 -or !$window[0].Title.Contains($documentName) -or $window[0].Title.StartsWith('*')){throw 'The owned document title changed; Paint was left open without discarding anything.'}
   [void][MoonElevatedNative]::PostMessage([IntPtr][long]$owned.id,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
   if(!$actualProcess.WaitForExit(5000)){
    if(!$AllowOwnedFixtureTermination){throw 'Owned Paint did not close normally; forced fixture cleanup was not enabled.'}
    Assert-Owned
    $native=@([MoonApplicationNative]::Windows()|Where-Object {$_.ProcessId -eq $owned.process_id})
    $main=@($native|Where-Object {$_.Id -eq $owned.id})
    if($main.Count -ne 1 -or $main[0].Title -ne $owned.title -or @($native|Where-Object {$_.Class -eq 'MSPaintApp' -and $_.Id -ne $owned.id}).Count -or (Get-FileHash -LiteralPath $bitmap -Algorithm SHA256).Hash -ne $bitmapHash -or [MoonPaintDialogNative]::LastInputTime() -ne $lastOwnedInput){throw 'The blank-fixture/input identity changed; Paint was left open to preserve possible user intervention.'}
    $report.forced_cleanup=[pscustomobject]@{owned=$owned;native=$native;fixture_sha256=$bitmapHash;last_input_time=$lastOwnedInput;reason='Normal close failed; explicit fixture-only cleanup enabled'};Save-Report
    $actualProcess.Kill();if(!$actualProcess.WaitForExit(5000)){throw 'The exact retained owned fixture process did not exit.'}
    $cleanupErrors.Add('The unchanged owned blank fixture required forced termination; this scenario is not a pass.')
   }
   $report.owned_paint_closed=$true
  }catch{$cleanupErrors.Add($_.Exception.Message)}
 }
 try {
  if([MoonElevatedNative]::IsWindow([IntPtr]$previous) -and [MoonApplicationNative]::ProcessId($previous) -eq $previousPid -and [MoonApplicationNative]::Birth($previousPid) -eq $previousBirth){[void][MoonPaintDialogNative]::SetForegroundWindow([IntPtr]$previous)}
  Wait-MoonCondition -Description 'the original live Moon layout after owned Paint cleanup' -Seconds 5 -Condition {
   $null=Assert-Daemon
   $script:finalFingerprint=Get-MoonLayoutFingerprint -DevelopmentPath $ExpectedDaemonPath
   $script:finalFingerprint -eq $baselineFingerprint
  }
  foreach($identity in $baselineIdentities){if([MoonApplicationNative]::ProcessId($identity.id) -ne $identity.process_id -or [MoonApplicationNative]::Birth([uint32]$identity.process_id) -ne $identity.birth){throw 'A preexisting managed window identity changed during validation.'}}
  $configHashAfter=if(Test-Path -LiteralPath $baselineStats.config_path){(Get-FileHash -LiteralPath $baselineStats.config_path -Algorithm SHA256).Hash}else{$null}
  if($configHashAfter -ne $configHashBefore){throw 'The live configuration file changed during the scenario.'}
  $report.final_foreground=[MoonElevatedNative]::GetForegroundWindow().ToInt64()
  if($report.final_foreground -ne $previous){throw 'The original native foreground was not restored.'}
  $report.original_layout_restored=$true;$report.final_fingerprint=$script:finalFingerprint;$report.preserved_identities=$baselineIdentities;$report.final_config_sha256=$configHashAfter
 }catch{$cleanupErrors.Add($_.Exception.Message)}
 if($savedPointer -and [MoonPaintDialogNative]::LastInputTime() -eq $lastOwnedInput){$report.cursor_restored=[MoonPaintDialogNative]::RestorePointer($savedPointer)}
 if($actualProcess){$actualProcess.Dispose()};if($launcher){$launcher.Dispose()}
 $report.checks=$checks.ToArray();$report.cleanup_errors=$cleanupErrors.ToArray();$report.error=if($failure){$failure.Exception.Message}else{$null};$report.passed=(!$failure -and !$cleanupErrors.Count);$report.finished_at=[DateTime]::UtcNow.ToString('o');Save-Report
}
[pscustomobject]@{passed=$report.passed;report=$ReportPath;checks=$checks.ToArray();owned=$owned;cleanup_errors=$cleanupErrors.ToArray();error=$report.error}|ConvertTo-Json -Depth 8
if(!$report.passed){throw 'The Paint transient-dialog scenario did not pass; inspect its retained ownership/evidence report.'}
