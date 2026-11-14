# Validate native theme editing and file sharing using only an owned fixture.
[CmdletBinding()]
param(
 [Parameter(Mandatory=$true)][string]$Binary,
 [Parameter(Mandatory=$true)][ValidatePattern('^[A-Fa-f0-9]{64}$')][string]$ExpectedSha256,
 [Parameter(Mandatory=$true)][string]$OutputDirectory
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
$providerPath=Join-Path ([Runtime.InteropServices.RuntimeEnvironment]::GetRuntimeDirectory()) 'WPF\UIAutomationClientsideProviders.dll'
$providerName=[Reflection.AssemblyName]::GetAssemblyName($providerPath)
$clientName=[System.Windows.Automation.AutomationElement].Assembly.GetName()
$token=([BitConverter]::ToString($providerName.GetPublicKeyToken())).Replace('-','').ToLowerInvariant()
if($providerName.Name -cne 'UIAutomationClientsideProviders' -or $providerName.Version -ne $clientName.Version -or $token -cne '31bf3856ad364e35'){throw 'Unexpected Windows UIA provider assembly identity.'}
$providerAssembly=[Reflection.Assembly]::LoadFrom($providerPath)
try { [System.Windows.Automation.ClientSettings]::RegisterClientSideProviderAssembly($providerAssembly.GetName()) }
catch {
 $cause=$_.Exception.InnerException
 if($cause -isnot [NullReferenceException] -or $cause.StackTrace -notmatch 'MS.Internal.Automation.ProxyManager.LoadDefaultProxies'){throw}
 Write-Output 'The PowerShell default UIA provider stack walk failed; retrying the supported registration once in this client.'
 [System.Windows.Automation.ClientSettings]::RegisterClientSideProviderAssembly($providerAssembly.GetName())
}
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class MoonLibraryUiNative {
    delegate bool EnumWindowProc(IntPtr window,IntPtr parameter);
    [StructLayout(LayoutKind.Sequential)] public struct Frame {public int Left,Top,Right,Bottom;}
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowProc callback,IntPtr parameter);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window,out Frame frame);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window,StringBuilder text,int capacity);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr process,uint access,out IntPtr token);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr token,int kind,IntPtr value,uint length,out uint returned);
    [DllImport("advapi32.dll")] static extern IntPtr GetSidSubAuthorityCount(IntPtr sid);
    [DllImport("advapi32.dll")] static extern IntPtr GetSidSubAuthority(IntPtr sid,uint index);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
    [DllImport("user32.dll",SetLastError=true)] public static extern bool PostMessage(IntPtr window,uint message,IntPtr wparam,IntPtr lparam);
    public sealed class Token {public uint integrity_rid; public bool ui_access; public bool elevated;}
    public static IntPtr FindVisibleWindow(uint processId) {
        IntPtr result=IntPtr.Zero;bool ambiguous=false;
        EnumWindows((window,parameter)=>{
            uint owner;GetWindowThreadProcessId(window,out owner);
            if(owner!=processId || !IsWindowVisible(window))return true;
            var title=new StringBuilder(256);GetWindowText(window,title,title.Capacity);
            if(title.ToString()!="Moon Settings")return true;
            Frame frame;if(!GetWindowRect(window,out frame) || frame.Right-frame.Left<200 || frame.Bottom-frame.Top<120)return true;
            if(result!=IntPtr.Zero){ambiguous=true;return false;}
            result=window;return true;
        },IntPtr.Zero);
        if(ambiguous)throw new InvalidOperationException("Multiple visible Moon Settings windows belong to the owned process.");
        return result;
    }
    static Exception Error() {return new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());}
    static uint Word(IntPtr token,int kind) {
        var memory=Marshal.AllocHGlobal(4);
        try {uint length;if(!GetTokenInformation(token,kind,memory,4,out length))throw Error();return unchecked((uint)Marshal.ReadInt32(memory));}
        finally {Marshal.FreeHGlobal(memory);}
    }
    public static Token ReadToken(IntPtr process) {
        IntPtr token;if(!OpenProcessToken(process,8,out token))throw Error();
        try {
            uint length;GetTokenInformation(token,25,IntPtr.Zero,0,out length);
            if(length<IntPtr.Size || length>65536)throw new InvalidOperationException("Invalid integrity-label length");
            var memory=Marshal.AllocHGlobal((int)length);
            try {
                if(!GetTokenInformation(token,25,memory,length,out length))throw Error();
                var sid=Marshal.ReadIntPtr(memory);var count=Marshal.ReadByte(GetSidSubAuthorityCount(sid));
                if(count==0)throw new InvalidOperationException("Integrity SID has no subauthority");
                return new Token {integrity_rid=unchecked((uint)Marshal.ReadInt32(GetSidSubAuthority(sid,(uint)(count-1)))),ui_access=Word(token,26)!=0,elevated=Word(token,20)!=0};
            } finally {Marshal.FreeHGlobal(memory);}
        } finally {CloseHandle(token);}
    }

    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window,StringBuilder value,int length);
    [DllImport("user32.dll")] static extern IntPtr OpenInputDesktop(uint flags,bool inherit,uint access);
    [DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr desktop);
    [DllImport("user32.dll")] static extern IntPtr GetThreadDesktop(uint thread);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern bool GetUserObjectInformation(IntPtr handle,int index,StringBuilder value,int length,out uint needed);
    public static string Class(IntPtr window){var value=new StringBuilder(256);if(GetClassName(window,value,value.Capacity)==0)throw Error();return value.ToString();}
    public static bool InputDesktopAvailable(){
        var desktop=OpenInputDesktop(0,false,1);if(desktop==IntPtr.Zero)return false;
        try{uint needed;var name=new StringBuilder(256);var own=new StringBuilder(256);
            return GetUserObjectInformation(desktop,2,name,512,out needed) && name.ToString()=="Default"
                && GetUserObjectInformation(GetThreadDesktop(GetCurrentThreadId()),2,own,512,out needed) && own.ToString()==name.ToString();
        }finally{CloseDesktop(desktop);}
    }

    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr window,uint flags);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window,IntPtr dc,uint flags);
    public static IntPtr FindDialog(uint processId,IntPtr owner,string expectedTitle){
        IntPtr result=IntPtr.Zero; bool ambiguous=false;
        EnumWindows((window,parameter)=>{uint pid;GetWindowThreadProcessId(window,out pid);
            if(pid!=processId || !IsWindowVisible(window) || GetAncestor(window,3)!=owner)return true;
            var title=new StringBuilder(256);GetWindowText(window,title,title.Capacity);
            if(title.ToString()!=expectedTitle)return true;
            if(result!=IntPtr.Zero){ambiguous=true;return false;}result=window;return true;
        },IntPtr.Zero);
        if(ambiguous)throw new InvalidOperationException("Ambiguous owned native file dialog.");return result;
    }
}
'@
$Binary=[IO.Path]::GetFullPath($Binary)
$OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
if(Test-Path -LiteralPath $OutputDirectory){throw 'Use a fresh evidence directory; existing files will not be overwritten.'}
[void][IO.Directory]::CreateDirectory($OutputDirectory)
$config=Join-Path $OutputDirectory 'fixture.toml'
$library=$config+'.moon-library.json'
$original="# Owned native theme-library validation`r`nconfig-version = 1`r`ntheme = 'dark'`r`n[bar]`r`nenabled = false`r`n[borders]`r`nenabled = false`r`n"
[IO.File]::WriteAllText($config,$original,[Text.UTF8Encoding]::new($false))
$personal=Join-Path $env:USERPROFILE '.config\moon\config.toml'
$process=$null;$identity=$null;$failure=$null;$closed=$false;$activePicker=$null
$checks=[Collections.Generic.List[string]]::new();$cleanup=[Collections.Generic.List[string]]::new();$trees=[Collections.Generic.List[object]]::new()
$exportPath=Join-Path $OutputDirectory 'linen.moon-theme.json'
$importPath=Join-Path $OutputDirectory 'roundtrip.moon-theme.json'
$customName='Fixture Copper';$roundtripName='Fixture Roundtrip'
function Hash-OrNull([string]$Path){if(Test-Path -LiteralPath $Path){return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash};return $null}
$personalBefore=Hash-OrNull $personal;$personalLibraryBefore=Hash-OrNull ($personal+'.moon-library.json')
function Assert-Unchanged {
 if([IO.File]::ReadAllText($config) -cne $original){throw 'The fixture configuration changed although no configuration Save was requested.'}
 if((Hash-OrNull $personal) -cne $personalBefore -or (Hash-OrNull ($personal+'.moon-library.json')) -cne $personalLibraryBefore){throw 'Personal settings/library changed during the check; no further actions are allowed.'}
}
function Assert-Process {
 if(!$process -or $process.HasExited -or !$identity){throw 'The retained Settings process is unavailable.'}
 if($process.StartTime.ToFileTimeUtc() -ne $identity.birth -or $process.MainModule.FileName -ine $Binary){throw 'The retained Settings image or birth changed.'}
}
function Main-Handle {
 Assert-Process
 $handle=[MoonLibraryUiNative]::FindVisibleWindow([uint32]$process.Id)
 if($handle -ne [IntPtr]::Zero){[uint32]$owner=0;[void][MoonLibraryUiNative]::GetWindowThreadProcessId($handle,[ref]$owner);if($owner -ne $process.Id){throw 'Settings HWND ownership changed.'}}
 return $handle
}
function Main-Root {
 $handle=Main-Handle;if($handle -eq [IntPtr]::Zero){return $null}
 $element=[Windows.Automation.AutomationElement]::FromHandle($handle)
 if($element.Current.ProcessId -ne $process.Id -or $element.Current.Name -cne 'Moon Settings'){throw 'Settings UI Automation ownership changed.'}
 return $element
}
function Assert-Action([Windows.Automation.AutomationElement]$Root) {
 Assert-Process;Assert-Unchanged
 if(![MoonLibraryUiNative]::InputDesktopAvailable()){throw 'The ordinary input desktop is unavailable.'}
 $handle=[IntPtr]$Root.Current.NativeWindowHandle
 [uint32]$owner=0;[void][MoonLibraryUiNative]::GetWindowThreadProcessId($handle,[ref]$owner)
 $main=Main-Handle
 if($owner -ne $process.Id -or ($handle -ne $main -and [MoonLibraryUiNative]::GetAncestor($handle,3) -ne $main)){throw 'The UI action target is not owned by this Settings fixture.'}
 if([MoonLibraryUiNative]::GetForegroundWindow() -ne $handle){throw 'The owned action target no longer has foreground; no input was sent.'}
}
function Wait-For([scriptblock]$Condition,[string]$Description) {
 $until=[DateTime]::UtcNow.AddSeconds(15)
 do {if(&$Condition){return};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $until)
 throw "Timed out: $Description"
}
function Find-Control([Windows.Automation.AutomationElement]$Root,[string]$Name,[object]$Pattern=$null) {
 if(!$Root){return $null}
 $items=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$Name))
 $matches=@(foreach($item in $items){$p=$null;if(!$Pattern -or $item.TryGetCurrentPattern($Pattern,[ref]$p)){$item}})
 if($matches.Count -gt 1){throw "Ambiguous control '$Name' in the owned window."}
 if($matches.Count){return $matches[0]};return $null
}
function Invoke-Control([Windows.Automation.AutomationElement]$Root,[string]$Name) {
 Assert-Action $Root
 $item=Find-Control $Root $Name ([Windows.Automation.InvokePattern]::Pattern)
 if(!$item -or !$item.Current.IsEnabled){throw "Action unavailable: $Name"}
 ([Windows.Automation.InvokePattern]$item.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)).Invoke()
}
function Click([string]$Name){Invoke-Control (Main-Root) $Name}
function Set-Value([Windows.Automation.AutomationElement]$Root,[string]$Name,[string]$Value) {
 Assert-Action $Root
 $item=Find-Control $Root $Name ([Windows.Automation.ValuePattern]::Pattern)
 if(!$item -or !$item.Current.IsEnabled){throw "Editable field unavailable: $Name"}
 ([Windows.Automation.ValuePattern]$item.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).SetValue($Value)
}
function Is-Dirty {
 $button=Find-Control (Main-Root) 'Save changes' ([Windows.Automation.InvokePattern]::Pattern)
 return $button -and $button.Current.IsEnabled
}
function Save-Tree([Windows.Automation.AutomationElement]$Root,[string]$Label) {
 # Bounded owned tree only: metadata, never text-field values or unrelated apps.
 $queue=[Collections.Generic.Queue[object]]::new();$queue.Enqueue([pscustomobject]@{element=$Root;depth=0});$nodes=[Collections.Generic.List[object]]::new()
 $walker=[Windows.Automation.TreeWalker]::ControlViewWalker
 while($queue.Count -and $nodes.Count -lt 200){
  $entry=$queue.Dequeue();$element=$entry.element;$current=$element.Current
  $nodes.Add([pscustomobject]@{depth=$entry.depth;name=$current.Name.Substring(0,[Math]::Min(100,$current.Name.Length));id=$current.AutomationId;type=$current.ControlType.ProgrammaticName;enabled=$current.IsEnabled;offscreen=$current.IsOffscreen;patterns=@($element.GetSupportedPatterns()|ForEach-Object {$_.ProgrammaticName})})
  if($entry.depth -lt 7){$child=$walker.GetFirstChild($element);$siblings=0;while($child -and $queue.Count -lt 200 -and $siblings -lt 80){$queue.Enqueue([pscustomobject]@{element=$child;depth=$entry.depth+1});$child=$walker.GetNextSibling($child);$siblings++}}
 }
 $trees.Add([pscustomobject]@{label=$Label;nodes=$nodes.ToArray();bounded=$true})
}
function Card-Export([string]$Name) {
 $root=Main-Root;Assert-Action $root
 $apply=Find-Control $root ('Apply '+$Name) ([Windows.Automation.InvokePattern]::Pattern)
 if(!$apply){throw "Theme card missing: $Name"}
 $parent=$apply;$walker=[Windows.Automation.TreeWalker]::ControlViewWalker
 for($i=0;$i -lt 8 -and $parent;$i++){
  $parent=$walker.GetParent($parent);if(!$parent){break}
  $exports=$parent.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,'Export'))
  if($exports.Count -eq 1){$button=$exports[0];if(!$button.Current.IsEnabled){throw 'Theme export is disabled.'};([Windows.Automation.InvokePattern]$button.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)).Invoke();return}
  if($exports.Count -gt 1){break}
 }
 throw "Could not uniquely locate Export within the $Name card."
}
function Picker-NameControl([Windows.Automation.AutomationElement]$Root) {
 $items=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Edit))
 $matches=@(foreach($item in $items){$p=$null;$name=$item.Current.Name.Replace('&','').Trim().TrimEnd(':');if($name -ceq 'File name' -and $item.Current.IsEnabled -and $item.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern,[ref]$p)){$item}})
 if($matches.Count -eq 1){return $matches[0]};return $null
}
function Wait-PickerReady([string]$Title,[string]$Mode) {
 $script:readyPicker=$null
 Wait-For {
  $hwnd=[MoonLibraryUiNative]::FindDialog([uint32]$process.Id,(Main-Handle),$Title)
  if($hwnd -eq [IntPtr]::Zero -or [MoonLibraryUiNative]::GetForegroundWindow() -ne $hwnd){return $false}
  $root=[Windows.Automation.AutomationElement]::FromHandle($hwnd)
  $cancel=Find-Control $root 'Cancel' ([Windows.Automation.InvokePattern]::Pattern)
  $accept=Find-Control $root $(if($Mode -eq 'save'){'Save'}else{'Open'}) ([Windows.Automation.InvokePattern]::Pattern)
  $name=Picker-NameControl $root
  # Open may be disabled until a file name is supplied. Its pattern must exist;
  # Cancel and the file-name input must already be usable.
  if($cancel -and $cancel.Current.IsEnabled -and $accept -and $name){$script:readyPicker=$root;return $true}
  return $false
 } "Owned native $Mode dialog controls expose Invoke/Value patterns"
 return $script:readyPicker
}
function Picker([string]$Mode) {
 $title=$(if($Mode -eq 'save'){'Export a Moon theme or profile'}else{'Import a Moon theme or profile'})
 $script:activePicker=$title
 $root=Wait-PickerReady $title $Mode
 Assert-Action $root
 Save-Tree $root ($Mode+'-ready-dialog-'+$trees.Count)
 return $root
}
function Picker-Path([Windows.Automation.AutomationElement]$Root,[string]$Path) {
 Assert-Action $Root
 $field=Picker-NameControl $Root
 if(!$field){throw 'The inspected native dialog has no unique editable File name control.'}
 ([Windows.Automation.ValuePattern]$field.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).SetValue($Path)
}
function Wait-PickerClosed {
 $title=$script:activePicker
 Wait-For {([MoonLibraryUiNative]::FindDialog([uint32]$process.Id,(Main-Handle),$title)) -eq [IntPtr]::Zero} 'Native picker closed'
 $script:activePicker=$null
 Wait-For {$button=Find-Control (Main-Root) 'Import theme' ([Windows.Automation.InvokePattern]::Pattern);$button -and $button.Current.IsEnabled} 'Native reply unlocks the library'
}
try {
 if((Hash-OrNull $Binary) -cne $ExpectedSha256){throw 'The exact reviewed Settings binary is required.'}
 if(![MoonLibraryUiNative]::InputDesktopAvailable()){throw 'The ordinary input desktop is unavailable.'}
 if(@(Get-Process -Name moon-settings -ErrorAction SilentlyContinue).Count){throw 'A Settings process already exists; it will not be touched.'}
 $self=[Diagnostics.Process]::GetCurrentProcess();try{$token=[MoonLibraryUiNative]::ReadToken($self.Handle)}finally{$self.Dispose()}
 if($token.integrity_rid -ne 8192 -or $token.ui_access -or $token.elevated){throw 'Run from ordinary medium-integrity Explorer.'}
 $start=[Diagnostics.ProcessStartInfo]::new();$start.FileName=$Binary;$start.UseShellExecute=$false;$start.CreateNoWindow=$true;$start.Arguments='--config "'+$config+'"'
 $process=[Diagnostics.Process]::Start($start);$null=$process.Handle
 $identity=[pscustomobject]@{process_id=$process.Id;birth=$process.StartTime.ToFileTimeUtc();path=$process.MainModule.FileName;sha256=$ExpectedSha256;token=[MoonLibraryUiNative]::ReadToken($process.Handle);config=$config}
 [IO.File]::WriteAllText((Join-Path $OutputDirectory 'owned-process.json'),($identity|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
 Assert-Process
 if($identity.token.integrity_rid -ne 8192 -or $identity.token.ui_access -or $identity.token.elevated){throw 'Settings did not retain ordinary permissions.'}
 Wait-For {(Main-Handle) -ne [IntPtr]::Zero} 'Visible owned Settings window'
 Wait-For {Find-Control (Main-Root) 'Appearance' ([Windows.Automation.InvokePattern]::Pattern)} 'Loaded Settings page'
 Click 'Appearance'
 Wait-For {(Find-Control (Main-Root) 'Apply Linen' ([Windows.Automation.InvokePattern]::Pattern)) -and (Find-Control (Main-Root) 'Apply Slate' ([Windows.Automation.InvokePattern]::Pattern)) -and (Find-Control (Main-Root) 'Apply Forest' ([Windows.Automation.InvokePattern]::Pattern))} 'Three real curated themes'
 $checks.Add('Real native Theme gallery exposes Linen, Slate and Forest')
 Click 'Apply Slate';Wait-For {Is-Dirty} 'Theme changes the draft'
 Assert-Unchanged;if(Test-Path -LiteralPath $library){throw 'Applying a builtin theme unexpectedly wrote a library.'}
 Click 'Undo last change';Wait-For {!(Is-Dirty)} 'Undo restores the clean draft'
 Assert-Unchanged;$checks.Add('Theme Apply and draft Undo leave fixture and personal configuration bytes unchanged')
 Click 'Save your theme';Wait-For {Find-Control (Main-Root) 'Saved item name' ([Windows.Automation.ValuePattern]::Pattern)} 'Named theme dialog'
 Set-Value (Main-Root) 'Saved item name' $customName;Click 'Save to library'
 Wait-For {(Test-Path -LiteralPath $library) -and (Find-Control (Main-Root) ('Apply '+$customName) ([Windows.Automation.InvokePattern]::Pattern))} 'Custom theme saved to fixture sidecar'
 $savedLibrary=Get-Content -Raw -LiteralPath $library|ConvertFrom-Json
 if(@($savedLibrary.entries|Where-Object {$_.name -ceq $customName -and $_.kind -ceq 'theme'}).Count -ne 1){throw 'Fixture sidecar does not contain the saved custom theme.'}
 Assert-Unchanged;$sidecarBefore=Hash-OrNull $library
 $checks.Add('Saving a named theme writes only the fixture library sidecar')
 Card-Export 'Linen';$picker=Picker 'save';Invoke-Control $picker 'Cancel';Wait-PickerClosed
 if((Hash-OrNull $library) -cne $sidecarBefore -or (Test-Path -LiteralPath $exportPath)){throw 'Canceling Save changed the fixture files.'}
 Click 'Import theme';$picker=Picker 'open';Invoke-Control $picker 'Cancel';Wait-PickerClosed
 if((Hash-OrNull $library) -cne $sidecarBefore){throw 'Canceling Open changed the fixture library.'}
 $checks.Add('Owned native Save and Open cancellation leave the library and configuration unchanged')
 Card-Export 'Linen';$picker=Picker 'save';Picker-Path $picker $exportPath;Invoke-Control $picker 'Save';Wait-PickerClosed
 Wait-For {Test-Path -LiteralPath $exportPath} 'Native exported file'
 $exported=Get-Content -Raw -LiteralPath $exportPath|ConvertFrom-Json
 if($exported.version -ne 1 -or $exported.kind -cne 'theme' -or $exported.name -cne 'Linen' -or (Get-Item -LiteralPath $exportPath).Length -gt 65536){throw 'The native export does not contain the selected bounded theme document.'}
 $exported.name=$roundtripName
 [IO.File]::WriteAllText($importPath,($exported|ConvertTo-Json -Depth 30),[Text.UTF8Encoding]::new($false))
 Click 'Import theme';$picker=Picker 'open';Picker-Path $picker $importPath;Invoke-Control $picker 'Open'
 Wait-For {([MoonLibraryUiNative]::FindDialog([uint32]$process.Id,(Main-Handle),'Import a Moon theme or profile')) -eq [IntPtr]::Zero} 'Import picker closed'
 $activePicker=$null
 Wait-For {Find-Control (Main-Root) 'Moon import data' ([Windows.Automation.ValuePattern]::Pattern)} 'Import preview before confirmation'
 $preview=Find-Control (Main-Root) 'Moon import data' ([Windows.Automation.ValuePattern]::Pattern)
 $previewData=([Windows.Automation.ValuePattern]$preview.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).Current.Value|ConvertFrom-Json
 if($previewData.name -cne $roundtripName -or (Hash-OrNull $library) -cne $sidecarBefore){throw 'Import skipped preview or changed the sidecar before confirmation.'}
 Click 'Import to library'
 Wait-For {Find-Control (Main-Root) ('Apply '+$roundtripName) ([Windows.Automation.InvokePattern]::Pattern)} 'Confirmed imported theme'
 $savedLibrary=Get-Content -Raw -LiteralPath $library|ConvertFrom-Json
 $roundtrip=@($savedLibrary.entries|Where-Object {$_.name -ceq $roundtripName -and $_.kind -ceq 'theme'})
 if($roundtrip.Count -ne 1 -or ($roundtrip[0].values|ConvertTo-Json -Depth 30 -Compress) -cne ($exported.values|ConvertTo-Json -Depth 30 -Compress)){throw 'The native file roundtrip did not preserve the exported theme values.'}
 if(Is-Dirty){throw 'Library import unexpectedly changed the Settings draft.'}
 Assert-Unchanged;$checks.Add('Native file Export/Open roundtrip previews before confirmation and preserves exact theme values in the fixture sidecar')
 $handle=Main-Handle;Assert-Action (Main-Root);$bounds=[MoonLibraryUiNative+Frame]::new()
 if(![MoonLibraryUiNative]::GetWindowRect($handle,[ref]$bounds)){throw 'Owned frame unavailable for capture.'}
 $bitmap=[Drawing.Bitmap]::new($bounds.Right-$bounds.Left,$bounds.Bottom-$bounds.Top);$graphics=[Drawing.Graphics]::FromImage($bitmap)
 try{$dc=$graphics.GetHdc();try{if(![MoonLibraryUiNative]::PrintWindow($handle,$dc,2)){throw 'PrintWindow failed for the owned Settings frame.'}}finally{$graphics.ReleaseHdc($dc)};$bitmap.Save((Join-Path $OutputDirectory 'appearance.png'),[Drawing.Imaging.ImageFormat]::Png)}finally{$graphics.Dispose();$bitmap.Dispose()}
}catch{$failure=$_;try{if($process -and !$process.HasExited){Save-Tree (Main-Root) 'failure-settings';if($activePicker){$h=[MoonLibraryUiNative]::FindDialog([uint32]$process.Id,(Main-Handle),$activePicker);if($h -ne [IntPtr]::Zero){Save-Tree ([Windows.Automation.AutomationElement]::FromHandle($h)) 'failure-picker'}}}}catch{$cleanup.Add('Failure tree: '+$_.Exception.Message)}}
finally{
 if($process){try{
  if(!$process.HasExited){
   if($activePicker){$h=[MoonLibraryUiNative]::FindDialog([uint32]$process.Id,(Main-Handle),$activePicker);if($h -ne [IntPtr]::Zero){$mode=$(if($activePicker -ceq 'Export a Moon theme or profile'){'save'}else{'open'});$ready=Wait-PickerReady $activePicker $mode;Invoke-Control $ready 'Cancel';Wait-PickerClosed}}
   $root=Main-Root;Assert-Action $root
   $cancel=Find-Control $root 'Cancel' ([Windows.Automation.InvokePattern]::Pattern)
   if($cancel){Invoke-Control $root 'Cancel'}
   $root=Main-Root;Assert-Action $root
   if(![MoonLibraryUiNative]::PostMessage([IntPtr]$root.Current.NativeWindowHandle,0x10,[IntPtr]::Zero,[IntPtr]::Zero)){throw 'Owned Settings close message failed.'}
   if(!$process.WaitForExit(1000)){
    Wait-For {$process.HasExited -or (Find-Control (Main-Root) 'Discard and close' ([Windows.Automation.InvokePattern]::Pattern))} 'Owned fixture close confirmation'
    if(!$process.HasExited){Click 'Discard and close';if(!$process.WaitForExit(5000)){throw 'Owned Settings did not close; no forced termination was used.'}}
   }
  }
  $closed=$true
 }catch{$cleanup.Add($_.Exception.Message);if(!$failure){$failure=$_}}finally{$process.Dispose()}}
 try{Assert-Unchanged}catch{if(!$failure){$failure=$_}}
 $report=[ordered]@{passed=($null -eq $failure);recorded_at=[DateTime]::UtcNow.ToString('o');identity=$identity;fixture_config=$config;fixture_library=$library;checks=$checks.ToArray();owned_settings_closed=$closed;personal_config_unchanged=((Hash-OrNull $personal) -ceq $personalBefore);personal_library_unchanged=((Hash-OrNull ($personal+'.moon-library.json')) -ceq $personalLibraryBefore);config_sha256=(Hash-OrNull $config);library_sha256=(Hash-OrNull $library);export_sha256=(Hash-OrNull $exportPath);import_sha256=(Hash-OrNull $importPath);daemon_actions_sent=$false;cleanup_errors=$cleanup.ToArray();error=$(if($failure){$failure|Out-String}else{$null})}
 [IO.File]::WriteAllText((Join-Path $OutputDirectory 'report.json'),($report|ConvertTo-Json -Depth 20),[Text.UTF8Encoding]::new($false))
 [IO.File]::WriteAllText((Join-Path $OutputDirectory 'control-trees.json'),($trees.ToArray()|ConvertTo-Json -Depth 15),[Text.UTF8Encoding]::new($false))
}
if($failure){throw $failure}
