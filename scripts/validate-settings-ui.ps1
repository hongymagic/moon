[CmdletBinding()]
param([Parameter(Mandatory)][string]$Binary,[string]$OutputDirectory,[string]$MoonBinary)
. (Join-Path $PSScriptRoot 'install-common.ps1')
$Binary=(Resolve-Path -LiteralPath $Binary).Path
if($MoonBinary){$MoonBinary=(Resolve-Path -LiteralPath $MoonBinary).Path}
$ErrorActionPreference='Stop'
if (!$OutputDirectory) {$OutputDirectory=Join-Path $env:LOCALAPPDATA 'moon-dev\settings-ui-validation'}
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;using System.Text;
public static class MoonSettingsValidation {
 delegate bool EnumWindowProc(IntPtr window,IntPtr parameter);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowProc callback,IntPtr parameter);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window,StringBuilder text,int capacity);
 [StructLayout(LayoutKind.Sequential)] public struct Rect {public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint flags);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 public static IntPtr FindVisibleWindow(uint processId) {
  IntPtr result=IntPtr.Zero;bool ambiguous=false;
  EnumWindows((window,parameter)=>{
   uint owner;GetWindowThreadProcessId(window,out owner);
   if(owner!=processId || !IsWindowVisible(window))return true;
   var title=new StringBuilder(256);GetWindowText(window,title,title.Capacity);
   if(title.ToString()!="Moon Settings")return true;
   Rect bounds;if(!GetWindowRect(window,out bounds) || bounds.Right-bounds.Left<200 || bounds.Bottom-bounds.Top<120)return true;
   if(result!=IntPtr.Zero){ambiguous=true;return false;}
   result=window;return true;
  },IntPtr.Zero);
  if(ambiguous)throw new InvalidOperationException("Multiple visible Moon Settings windows belong to the owned process.");
  return result;
 }
}
'@
[IO.Directory]::CreateDirectory($OutputDirectory)|Out-Null
# Only this owned test document is changed. The user's configuration is never opened.
$configDirectory=Join-Path $env:LOCALAPPDATA 'moon-dev\settings-ui-fixtures'
[void][IO.Directory]::CreateDirectory($configDirectory)
$config=Join-Path $configDirectory ('settings-'+[guid]::NewGuid().ToString('N')+'.toml')
$original="# Owned native settings validation`r`nconfig-version = 1`r`n[bar]`r`nenabled = false`r`n[borders]`r`nenabled = false`r`n"
[IO.File]::WriteAllText($config,$original,[Text.UTF8Encoding]::new($false))
$process=$null;$daemon=$null;$failure=$null;$cleanupErrors=[Collections.Generic.List[string]]::new()
$journal=Join-Path $env:LOCALAPPDATA ('moon\journal-v1-session'+[Diagnostics.Process]::GetCurrentProcess().SessionId+'.json')
$filter='moon-settings-validation-'+[guid]::NewGuid().ToString('N')
$otherConfig=$null;$liveEvidence=$null;$liveHelpEvidence=$null;$undoEvidence=$null
$checks=[Collections.Generic.List[string]]::new()
function Wait-MoonCondition([scriptblock]$Condition,[string]$Description) {
 $until=[DateTime]::UtcNow.AddSeconds(15)
 do {if(&$Condition){return};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $until)
 throw "Timed out: $Description"
}
function Get-OwnedSettingsHandle {
 $process.Refresh();if($process.HasExited){throw 'Owned Settings process exited unexpectedly.'}
 $handle=[MoonSettingsValidation]::FindVisibleWindow([uint32]$process.Id)
 if($handle -ne [IntPtr]::Zero){
  [uint32]$owner=0;[void][MoonSettingsValidation]::GetWindowThreadProcessId($handle,[ref]$owner)
  if($owner -ne $process.Id){throw 'Settings HWND owner changed.'}
 }
 return $handle
}
function Get-OwnedSettingsWindow {
 $handle=Get-OwnedSettingsHandle
 if($handle -eq [IntPtr]::Zero){return $null}
 $window=[Windows.Automation.AutomationElement]::FromHandle($handle)
 if($window.Current.ProcessId -ne $process.Id -or $window.Current.Name -ne 'Moon Settings'){throw 'Settings automation owner or title changed.'}
 return $window
}
function Find-MoonControl([string]$Name) {
 $window=Get-OwnedSettingsWindow
 if(!$window){return $null}
 $window.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$Name))
}
function Invoke-MoonButton([string]$Name) {
 $element=Find-MoonControl $Name
 if(!$element -or !$element.Current.IsEnabled){throw "Settings action unavailable: $Name"}
 ([Windows.Automation.InvokePattern]$element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)).Invoke()
}
function Find-MoonValue([string]$Name) {
 $window=Get-OwnedSettingsWindow
 if(!$window){return $null}
 $elements=$window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$Name))
 foreach($element in $elements){$pattern=$null;if($element.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern,[ref]$pattern)){return [Windows.Automation.ValuePattern]$pattern}}
 return $null
}
function Set-MoonField([string]$Name,[string]$Value) {
 Wait-MoonCondition {Find-MoonValue $Name} "Editable settings field: $Name"
 (Find-MoonValue $Name).SetValue($Value)
}
function Get-MoonField([string]$Name) {
 $value=Find-MoonValue $Name
 if($value){$value.Current.Value}
}

function Read-SettingsSource([string]$Path) {
 $stream=[IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
 try {
  $reader=[IO.StreamReader]::new($stream,[Text.UTF8Encoding]::new($false,$true))
  try{return $reader.ReadToEnd()}finally{$reader.Dispose()}
 }finally{$stream.Dispose()}
}
function Start-OwnedSettings([string]$Path) {
 $script:process=Start-Process -FilePath $Binary -ArgumentList @('--config',(ConvertTo-MoonArgument $Path)) -PassThru
 $null=$process.Handle
 Wait-MoonCondition {(Get-OwnedSettingsHandle) -ne [IntPtr]::Zero} 'Visible owned Moon Settings window'
 Wait-MoonCondition {Find-MoonValue 'Inner gap'} 'Settings content'
}
function Close-OwnedSettings {
 if(!$process){return}
 try {
  $process.Refresh()
  if(!$process.HasExited){
   $handle=Get-OwnedSettingsHandle
   if($handle -eq [IntPtr]::Zero){throw 'No visible owned Moon Settings window is available; close was refused.'}
   [uint32]$owner=0
   [void][MoonSettingsValidation]::GetWindowThreadProcessId($handle,[ref]$owner)
   if($owner -ne $process.Id){throw 'Settings HWND owner changed; close was refused.'}
   if(![MoonSettingsValidation]::PostMessage($handle,0x10,[IntPtr]::Zero,[IntPtr]::Zero)){throw 'Windows refused the owned Settings close message.'}
   if(!$process.WaitForExit(1000)){
    # Only this harness's document can be discarded.
    $discard=Find-MoonControl 'Discard and close'
    if($discard){([Windows.Automation.InvokePattern]$discard.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)).Invoke()}
    if(!$process.WaitForExit(5000)){throw 'Owned Settings remains open; no forced termination was used.'}
   }
  }
 } finally {$process.Dispose();$script:process=$null}
}
function Assert-OwnedDaemon {
 if(!$daemon){throw 'No daemon is owned by this test.'}
 $daemon.Refresh();if($daemon.HasExited){throw 'Owned settings daemon exited unexpectedly.'}
 $state=Invoke-MoonRpc -Method query -Query stats -DevelopmentPath $MoonBinary
 if($state.process_id -ne $daemon.Id -or
    ![string]::Equals($state.executable,$MoonBinary,[StringComparison]::OrdinalIgnoreCase) -or
    ![string]::Equals($state.config_path,$config,[StringComparison]::OrdinalIgnoreCase)){
  throw 'Running daemon identity differs from the owned executable, process or configuration.'
 }
 if($state.managed_windows -ne 0){throw 'The isolated settings daemon unexpectedly manages a window.'}
 return $state
}
function Stop-OwnedDaemon {
 if(!$daemon){return}
 $stopFailure=$null
 try {
  $daemon.Refresh()
  if(!$daemon.HasExited){
   try {
    $null=Assert-OwnedDaemon
    $null=Invoke-MoonRpc -Method command -Commands @('exit') -DevelopmentPath $MoonBinary
    if(!$daemon.WaitForExit(5000)){throw 'Owned daemon did not exit after the exit command.'}
   } catch {
    # This retained Process object identifies only the child created by this test.
    # Never send an exit command to a daemon that fails the identity check.
    $daemon.Refresh()
    $stopFailure=$_
    if(!$daemon.HasExited){$daemon.Kill();[void]$daemon.WaitForExit()}
   }
  }
 } finally {
  $daemon.Dispose();$script:daemon=$null
 }
 try{Wait-MoonCondition {!(Test-Path -LiteralPath $journal)} 'Owned daemon recovery journal removed'}catch{if(!$stopFailure){$stopFailure=$_}}
 if($stopFailure){throw $stopFailure}
}

try {
 if($MoonBinary){
  if(@(Get-Process -Name moon -ErrorAction SilentlyContinue).Count -or (Test-Path -LiteralPath $journal)){
   throw 'Live Settings validation requires no existing Moon process or recovery journal.'
  }
  $running=$null
  try{$running=Invoke-MoonRpc -Method query -Query stats -DevelopmentPath $MoonBinary}catch{}
  if($running){throw 'A Moon daemon already owns this session; it was left unchanged.'}
  if(Get-Process -Name $filter -ErrorAction SilentlyContinue){throw 'The unique process filter unexpectedly matches an existing process.'}
  $daemon=Start-Process -FilePath $MoonBinary -ArgumentList @('start','--only',$filter,'--no-hook','--no-surfaces','--config',(ConvertTo-MoonArgument $config)) -WindowStyle Hidden -PassThru -RedirectStandardOutput ($config+'.daemon.out') -RedirectStandardError ($config+'.daemon.err')
  Wait-MoonCondition {try{$null=Assert-OwnedDaemon;return $true}catch{$daemon.Refresh();if($daemon.HasExited){throw};return $false}} 'Owned daemon ready with zero managed windows'
  $checks.Add('An isolated daemon uses the exact fixture path and manages no windows')
 }
 Start-OwnedSettings $config
 $checks.Add('Native WebView loads all settings from the selected owned file')
 $initialInner=Get-MoonField 'Inner gap'
 if($initialInner -eq '24'){throw 'The Undo fixture must start with a different inner gap.'}
 Set-MoonField 'Inner gap' '24'
 Wait-MoonCondition {(Get-MoonField 'Inner gap') -eq '24' -and (Find-MoonControl 'Save changes').Current.IsEnabled -and (Find-MoonControl 'Undo last change').Current.IsEnabled} 'A real draft edit enables Save and Undo'
 Invoke-MoonButton 'Undo last change'
 Wait-MoonCondition {(Get-MoonField 'Inner gap') -eq $initialInner -and !(Find-MoonControl 'Save changes').Current.IsEnabled -and !(Find-MoonControl 'Undo last change').Current.IsEnabled} 'Undo restores the previous draft and consumes its checkpoint'
 if((Read-SettingsSource $config) -cne $original){throw 'Draft Undo unexpectedly changed the owned file.'}
 $undoEvidence=[pscustomobject]@{initial_inner=$initialInner;edited_inner='24';restored_inner=(Get-MoonField 'Inner gap');saved_source_unchanged=$true}
 $checks.Add('One-step Undo restores the draft without writing the owned file')
 Set-MoonField 'Inner gap' '24'
 Wait-MoonCondition {(Find-MoonControl 'Save changes').Current.IsEnabled} 'Save enabled after editing'
 Invoke-MoonButton 'Save changes'
 Wait-MoonCondition {(Read-SettingsSource $config) -match 'inner\s*=\s*24'} 'Native save persists the edit'
 $saved=(Read-SettingsSource $config)
 if(!$saved.StartsWith('# Owned native settings validation')){throw 'Saving lost the existing comment.'}
 Wait-MoonCondition {(Find-MoonControl 'All changes saved') -and (Find-MoonControl 'Reload').Current.IsEnabled -and (Find-MoonValue 'Outer gap')} 'Saved state and editable content after the native reply'
 $checks.Add('Real native bridge saves validated TOML and preserves comments')
 if($MoonBinary){
  Wait-MoonCondition { (Invoke-MoonRpc -Method query -Query config -DevelopmentPath $MoonBinary).gaps.inner -eq 24 } 'The owned daemon loads the saved configuration'
  $state=Assert-OwnedDaemon
  $actual=Invoke-MoonRpc -Method query -Query config -DevelopmentPath $MoonBinary
  if($actual.gaps.inner -ne 24 -or $actual.bar.enabled -or $actual.borders.enabled){throw 'The daemon did not load the exact isolated configuration.'}
  $liveEvidence=[pscustomobject]@{process_id=$state.process_id;config_path=$state.config_path;managed_windows=$state.managed_windows;inner_gap=$actual.gaps.inner;revision=$state.revision}
  Wait-MoonCondition {Find-MoonControl 'Saved and loaded by Moon.'} 'Persistent UI status confirms loaded configuration without claiming completed window effects'
  $checks.Add('The UI reports loaded settings and an independent authenticated query confirms the exact owned configuration')
  Invoke-MoonButton 'Shortcuts'
  Wait-MoonCondition {Find-MoonValue 'Find a shortcut'} 'Searchable in-app shortcut help'
  $null=Assert-OwnedDaemon
  $help=Invoke-MoonRpc -Method query -Query shortcuts -DevelopmentPath $MoonBinary
  if($help.hook_enabled -ne $false -or $help.mode -ne 'default' -or $help.paused -ne $false -or $help.emergency -ne 'ctrl+alt+shift+f12'){
   throw 'The isolated daemon did not expose the expected inactive-hook help state.'
  }
  if(!@($help.bindings|Where-Object {$_.commands -contains 'focus left'}).Count){throw 'The help fixture lacks an independently queried focus-left binding.'}
  $inactiveNotice='Mode: default '+[char]0x00B7+' Configured shortcuts are disabled for this Moon session. The recovery shortcut above remains available.'
  Wait-MoonCondition {(Find-MoonControl 'Configured shortcuts') -and (Find-MoonControl 'Configured shortcuts (inactive)') -and (Find-MoonControl $inactiveNotice) -and (Find-MoonControl 'Ctrl + Alt + Shift + F12')} 'Configured hooks are labelled inactive and emergency recovery remains discoverable'
  Set-MoonField 'Find a shortcut' 'focus left'
  Wait-MoonCondition {Find-MoonControl 'Focus the left window'} 'Shortcut search shows an independently queried action with a readable label'
  Set-MoonField 'Find a shortcut' $filter
  Wait-MoonCondition {(Find-MoonControl 'No current shortcuts match. Try a key, action, or workspace name.') -and !(Find-MoonControl 'Focus the left window')} 'A unique unmatched search reports an empty result'
  if((Read-SettingsSource $config) -cne $saved){throw 'Read-only shortcut help changed the owned configuration.'}
  $liveHelpEvidence=[pscustomobject]@{mode=$help.mode;hook_enabled=$help.hook_enabled;paused=$help.paused;binding_count=@($help.bindings).Count;emergency=$help.emergency;saved_source_unchanged=$true}
  $checks.Add('In-app help labels disabled hooks truthfully, searches current queried bindings, and exposes emergency recovery without editing settings')
  Invoke-MoonButton 'General'
  Wait-MoonCondition {Find-MoonValue 'Outer gap'} 'General settings remain editable after read-only help'
 }
 Set-MoonField 'Outer gap' '48'
 [IO.File]::AppendAllText($config,"# concurrent editor`r`n",[Text.UTF8Encoding]::new($false))
 $external=(Read-SettingsSource $config)
 Invoke-MoonButton 'Save changes'
 Wait-MoonCondition {Find-MoonControl 'The configuration changed after these settings were opened. Reload the settings before saving; your changes have not overwritten the file.'} 'Concurrent edit warning'
 if((Read-SettingsSource $config) -ne $external -or (Get-MoonField 'Outer gap') -ne '48'){throw 'Conflict handling lost the file or draft.'}
 $checks.Add('Concurrent external edits are rejected while the draft stays visible')
 Invoke-MoonButton 'Reload'
 Wait-MoonCondition {Find-MoonControl 'Cancel'} 'Reload confirmation'
 Invoke-MoonButton 'Cancel'
 Wait-MoonCondition {(Get-MoonField 'Outer gap') -eq '48'} 'Cancelling reload retains the draft after the dialog closes'
 Invoke-MoonButton 'Reload'
 Wait-MoonCondition {Find-MoonControl 'Reload settings'} 'Reload confirmation action'
 Invoke-MoonButton 'Reload settings'
 Wait-MoonCondition {(Get-MoonField 'Outer gap') -eq '32'} 'Reloaded saved settings'
 $checks.Add('Reload confirmation can retain the draft or load the external file')
 Invoke-MoonButton 'Appearance'
 Wait-MoonCondition {Find-MoonControl 'Focused color'} 'Appearance controls'
 $handle=Get-OwnedSettingsHandle
 $bounds=[MoonSettingsValidation+Rect]::new()
 if($handle -eq [IntPtr]::Zero -or ![MoonSettingsValidation]::GetWindowRect($handle,[ref]$bounds)){throw 'The visible owned Settings frame is unavailable for capture.'}
 $bitmap=[Drawing.Bitmap]::new($bounds.Right-$bounds.Left,$bounds.Bottom-$bounds.Top);$graphics=[Drawing.Graphics]::FromImage($bitmap)
 try {$dc=$graphics.GetHdc();try{[void][MoonSettingsValidation]::PrintWindow($handle,$dc,2)}finally{$graphics.ReleaseHdc($dc)};$bitmap.Save((Join-Path $OutputDirectory 'appearance.png'),[Drawing.Imaging.ImageFormat]::Png)}finally{$graphics.Dispose();$bitmap.Dispose()}
 $checks.Add('Appearance controls and preview are available in the real Windows host')
 if($MoonBinary){
  Close-OwnedSettings
  $otherConfig=Join-Path $configDirectory ('different-'+[guid]::NewGuid().ToString('N')+'.toml')
  [IO.File]::WriteAllText($otherConfig,$original,[Text.UTF8Encoding]::new($false))
  Start-OwnedSettings $otherConfig
  $before=Assert-OwnedDaemon
  Set-MoonField 'Inner gap' '28'
  Invoke-MoonButton 'Save changes'
  Wait-MoonCondition {Find-MoonControl 'Saved. The running Moon instance is using a different configuration.'} 'Different configuration warning'
  if((Read-SettingsSource $otherConfig) -notmatch 'inner\s*=\s*28'){throw 'The different owned document did not save.'}
  $after=Assert-OwnedDaemon
  $actual=Invoke-MoonRpc -Method query -Query config -DevelopmentPath $MoonBinary
  if($actual.gaps.inner -ne 24 -or $after.revision -ne $before.revision){throw 'Saving another configuration reloaded or changed the running daemon.'}
  $checks.Add('Saving a different owned file warns accurately and leaves the daemon configuration and revision unchanged')
 }
}catch{
 $failure=$_
 try {
  $names=@()
  if($process -and !$process.HasExited){
   $window=Get-OwnedSettingsWindow
   if($window){$names=@($window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)|ForEach-Object {$_.Current.Name}|Select-Object -Unique)}
  }
  [IO.File]::WriteAllText((Join-Path $OutputDirectory 'failure.json'),([pscustomobject]@{error=$failure.ToString();checks=$checks.ToArray();controls=$names}|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
 }catch{Write-Warning 'Could not capture Settings failure controls.'}
}finally{
 try {Close-OwnedSettings}catch{$cleanupErrors.Add($_.Exception.Message);if(!$failure){$failure=$_}}
 finally {try{Stop-OwnedDaemon}catch{$cleanupErrors.Add($_.Exception.Message);if(!$failure){$failure=$_}}}
}
$report=[pscustomobject]@{
 passed=($null -eq $failure);binary=$Binary;sha256=(Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash
 config=$config;checks=$checks.ToArray();user_configuration_changed=$false;draft_undo=$undoEvidence;live_help=$liveHelpEvidence
 moon=if($MoonBinary){[pscustomobject]@{binary=$MoonBinary;sha256=(Get-FileHash -LiteralPath $MoonBinary -Algorithm SHA256).Hash;filter=$filter;live_save=$liveEvidence}}else{$null}
 different_config=$otherConfig;cleanup_errors=$cleanupErrors.ToArray();error=if($failure){$failure.Exception.Message}else{$null}
}
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'report.json'),($report|ConvertTo-Json -Depth 7),[Text.UTF8Encoding]::new($false))
$report|ConvertTo-Json -Depth 7
if($failure){throw $failure}
