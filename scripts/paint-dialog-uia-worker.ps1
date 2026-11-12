# Each invocation runs in its own ordinary MTA process. The coordinator enforces
# a process timeout because even a single UIA provider call can block.
[CmdletBinding()]
param([string]$RequestPath,[switch]$CompileOnly)
$ErrorActionPreference='Stop'
& (Join-Path $PSScriptRoot 'application-integration.ps1') -CompileOnly|Out-Null
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class MoonPaintUiaNative {
 [DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr window,uint command);
 [DllImport("user32.dll")] static extern bool IsWindowEnabled(IntPtr window);
 public static long Owner(long window){return GetWindow(new IntPtr(window),4).ToInt64();}
 public static bool Enabled(long window){return IsWindowEnabled(new IntPtr(window));}
 public static bool IsOwnedPopup(long window,long owner){for(int i=0;i<8&&window!=0;i++){if(window==owner)return true;window=GetWindow(new IntPtr(window),4).ToInt64();}return false;}
}
'@
if($CompileOnly){[pscustomobject]@{compiled=$true;desktop_changed=$false}|ConvertTo-Json;return}
if(!$RequestPath){throw 'An owned Paint UIA request is required.'}
$request=[IO.File]::ReadAllText($RequestPath)|ConvertFrom-Json
if($request.version -ne 1 -or $request.action -notin @('Inspect','FindDialog','LocateEditColours','CancelDialog')){throw 'Unsupported Paint UIA request.'}
$owned=$request.owned
if($owned.title -notmatch '^moon-owned-dialog-[a-f0-9]{32}[.]bmp - Paint$'){throw 'The request is not for a uniquely named test document.'}
$process=Get-Process -Id $owned.process_id -ErrorAction Stop;[void]$process.Handle
function Assert-Owned {
 $process.Refresh()
 if($process.HasExited -or [MoonApplicationNative]::ProcessId([long]$owned.id) -ne $owned.process_id -or [MoonApplicationNative]::Birth([uint32]$owned.process_id) -ne [long]$owned.birth -or ![string]::Equals([MoonApplicationNative]::ImagePath([uint32]$owned.process_id),$owned.path,[StringComparison]::OrdinalIgnoreCase)){throw 'Paint UIA process/window ownership changed.'}
 $main=@([MoonApplicationNative]::Windows()|Where-Object {$_.Id -eq $owned.id})
 if($main.Count -ne 1 -or $main[0].Title -ne $owned.title){throw 'Paint UIA document identity changed.'}
}
function Evidence($Element) {
 $c=$Element.Current;$r=$c.BoundingRectangle
 if($c.ProcessId -ne $owned.process_id){throw 'An accessibility element belongs to another process.'}
 [pscustomobject]@{name=$c.Name;automation_id=$c.AutomationId;class=$c.ClassName;framework=$c.FrameworkId;type=$c.ControlType.ProgrammaticName;process_id=$c.ProcessId;native_window=$c.NativeWindowHandle;enabled=$c.IsEnabled;offscreen=$c.IsOffscreen;rect=@($r.X,$r.Y,$r.Width,$r.Height);runtime_id=@($Element.GetRuntimeId())}
}
function Roots {
 Assert-Owned
 # The WinUI popup root is examined before the document's large canvas tree.
 $windows=@([MoonApplicationNative]::Windows()|Where-Object {$_.ProcessId -eq $owned.process_id -and [MoonPaintUiaNative]::IsOwnedPopup($_.Id,$owned.id)}|Sort-Object @{Expression={$_.Id -eq $owned.id}})
 foreach($window in $windows){$element=[Windows.Automation.AutomationElement]::FromHandle([IntPtr][long]$window.Id);if($element.Current.ProcessId -ne $owned.process_id){throw 'An accessibility root belongs to another process.'};$element}
}
function Walk([object[]]$Roots,[int]$MaximumNodes,[int]$MaximumDepth,[scriptblock]$Match) {
 $queue=[Collections.Generic.Queue[object]]::new();$seen=[Collections.Generic.HashSet[string]]::new();$elements=[Collections.Generic.List[object]]::new();$truncated=$false
 foreach($root in $Roots){$queue.Enqueue([pscustomobject]@{element=$root;depth=0})}
 $walker=[Windows.Automation.TreeWalker]::ControlViewWalker
 while($queue.Count){
  if($elements.Count -ge $MaximumNodes){$truncated=$true;break}
  $item=$queue.Dequeue();$element=$item.element;$key=($element.GetRuntimeId() -join '-')
  if(!$seen.Add($key)){continue}
  $data=Evidence $element;$elements.Add($data)
  if($Match -and (& $Match $data)){return [pscustomobject]@{element=$element;evidence=$data;elements=$elements.ToArray();truncated=$truncated}}
  if($item.depth -ge $MaximumDepth){$truncated=$true;continue}
  $child=$walker.GetFirstChild($element);$siblings=[Collections.Generic.HashSet[string]]::new()
  while($child){
   if(($queue.Count+$elements.Count) -ge $MaximumNodes){$truncated=$true;break}
   # A provider may repeat a sibling identity (observed in Paint's palette).
   # Stop only that branch; queued unique siblings remain bounded and usable.
   if(!$siblings.Add(($child.GetRuntimeId() -join '-'))){$truncated=$true;break}
   $queue.Enqueue([pscustomobject]@{element=$child;depth=$item.depth+1});$child=$walker.GetNextSibling($child)
  }
 }
 return [pscustomobject]@{element=$null;evidence=$null;elements=$elements.ToArray();truncated=$truncated}
}
function Test-ColourDialogRoot($Element,[int]$ProcessId) {
 $Element.process_id -eq $ProcessId -and $Element.framework -eq 'XAML' -and $Element.type -eq 'ControlType.Window' -and $Element.class -eq 'Popup' -and $Element.name -match '^Edit colou?rs$' -and $Element.enabled -and !$Element.offscreen
}
function Get-ColourDialogControls([object[]]$Elements,[int]$ProcessId) {
 $visible=@($Elements|Where-Object {$_.process_id -eq $ProcessId -and $_.framework -eq 'XAML' -and $_.enabled -and !$_.offscreen})
 $spectrum=@($visible|Where-Object {$_.automation_id -eq 'ColorSpectrum' -and $_.class -eq 'Microsoft.UI.Xaml.Controls.Primitives.ColorSpectrum' -and $_.type -eq 'ControlType.Slider'})
 $primary=@($visible|Where-Object {$_.automation_id -eq 'PrimaryButton' -and $_.name -eq 'OK' -and $_.type -eq 'ControlType.Button'})
 $close=@($visible|Where-Object {$_.automation_id -eq 'CloseButton' -and $_.name -eq 'Cancel' -and $_.type -eq 'ControlType.Button'})
 if($spectrum.Count -eq 1 -and $primary.Count -eq 1 -and $close.Count -eq 1){[pscustomobject]@{spectrum=$spectrum[0];primary=$primary[0];close=$close[0]}}
}
function Find-Dialog {
 $found=Walk -Roots @(Roots) -MaximumNodes 96 -MaximumDepth 6 -Match {param($e) Test-ColourDialogRoot $e $owned.process_id}
 if(!$found.element){return $found}
 # Paint exposes this real colour dialog as a named XAML Popup. Require its
 # observed controls within that popup, not an arbitrary similarly named pane.
 $scope=Walk -Roots @($found.element) -MaximumNodes 256 -MaximumDepth 10 -Match $null
 $controls=Get-ColourDialogControls $scope.elements $owned.process_id
 if(!$controls){return [pscustomobject]@{element=$null;evidence=$null;elements=$scope.elements;truncated=$scope.truncated}}
 $found.evidence|Add-Member -NotePropertyName controls -NotePropertyValue $controls
 [pscustomobject]@{element=$found.element;evidence=$found.evidence;elements=$scope.elements;truncated=$scope.truncated}
}
function Invoke-Control($Element) {
 Assert-Owned
 if($Element.Current.ProcessId -ne $owned.process_id -or !$Element.Current.IsEnabled -or $Element.Current.IsOffscreen){throw 'The requested action is outside the visible enabled owned control.'}
 $pattern=$null
 if(!$Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)){throw 'The observed control has no Invoke pattern; coordinates were not guessed.'}
 # A modal provider may block here. Only this expendable worker waits; the
 # coordinator kills this worker at its fixed deadline and retains ownership.
 ([Windows.Automation.InvokePattern]$pattern).Invoke()
}
try {
 Assert-Owned
 switch($request.action){
  'Inspect' {$found=Walk -Roots @(Roots) -MaximumNodes 512 -MaximumDepth 24 -Match $null;$result=[pscustomobject]@{elements=$found.elements;truncated=$found.truncated}}
  'FindDialog' {$found=Find-Dialog;$result=[pscustomobject]@{dialog=$found.evidence;examined=$found.elements;truncated=$found.truncated}}
  'LocateEditColours' {
   $expected=($request.runtime_id -join '-')
   $found=Walk -Roots @(Roots) -MaximumNodes 512 -MaximumDepth 24 -Match {param($e) ($e.runtime_id -join '-') -eq $expected -and $e.type -eq 'ControlType.Button' -and $e.name -match '^Edit colou?rs(?:\s*\([^)]*\))?$' -and $e.enabled -and !$e.offscreen}
   if(!$found.element){throw 'The exact observed Edit colours control was not found in the bounded owned tree.'}
   $result=[pscustomobject]@{control=$found.evidence}
  }
  'CancelDialog' {
   $dialog=Find-Dialog
   if(!$dialog.element){$result=[pscustomobject]@{cancelled=$false;dialog=$null};break}
   if($request.runtime_id.Count -and ($dialog.evidence.runtime_id -join '-') -ne ($request.runtime_id -join '-')){throw 'The owned dialog identity changed; no Cancel action was sent.'}
   $cancelId=($dialog.evidence.controls.close.runtime_id -join '-')
   $found=Walk -Roots @($dialog.element) -MaximumNodes 256 -MaximumDepth 10 -Match {param($e) ($e.runtime_id -join '-') -eq $cancelId -and $e.automation_id -eq 'CloseButton' -and $e.type -eq 'ControlType.Button' -and $e.name -eq 'Cancel' -and $e.enabled -and !$e.offscreen}
   if(!$found.element){throw 'The owned dialog has no observed Cancel button in the bounded tree.'}
   Invoke-Control $found.element;$result=[pscustomobject]@{cancelled=$true;dialog=$dialog.evidence;control=$found.evidence}
  }
 }
 $native=@([MoonApplicationNative]::Windows()|Where-Object {$_.ProcessId -eq $owned.process_id}|ForEach-Object {[pscustomobject]@{id=$_.Id;class=$_.Class;title=$_.Title;owner=[MoonPaintUiaNative]::Owner($_.Id);enabled=[MoonPaintUiaNative]::Enabled($_.Id);owned_popup=[MoonPaintUiaNative]::IsOwnedPopup($_.Id,$owned.id)}})
 $result|Add-Member -NotePropertyName native_windows -NotePropertyValue $native
 $result|ConvertTo-Json -Depth 10 -Compress
}finally{$process.Dispose()}
