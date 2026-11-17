# Private-fixture Settings checks. No daemon lifecycle or personal configuration writes.
# Run with ordinary Explorer-launched Windows PowerShell 5.1 after coordinator review.
[CmdletBinding()]
param(
 [string]$Binary,
 [ValidatePattern('^[A-Fa-f0-9]{64}$')][string]$ExpectedSha256,
 [string]$OutputDirectory,
 [string]$ProtectedConfig,
 [switch]$CompileOnly,
 [switch]$VerifyStartupToggle
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'install-common.ps1')
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
public static class MoonUsabilityNative {
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

    [StructLayout(LayoutKind.Sequential)] struct MouseInput {public int X,Y;public uint Data,Flags,Time;public UIntPtr Extra;}
    [StructLayout(LayoutKind.Sequential)] struct KeyInput {public ushort Vk,Scan;public uint Flags,Time;public UIntPtr Extra;}
    [StructLayout(LayoutKind.Explicit)] struct InputData {[FieldOffset(0)] public MouseInput Mouse;[FieldOffset(0)] public KeyInput Key;}
    [StructLayout(LayoutKind.Sequential)] struct Input {public uint Type;public InputData Data;}
    [DllImport("user32.dll",SetLastError=true)] static extern uint SendInput(uint count,Input[] inputs,int size);
    [DllImport("user32.dll")] static extern short GetAsyncKeyState(int key);
    public static int InputSize(){return Marshal.SizeOf(typeof(Input));}
    [StructLayout(LayoutKind.Sequential)] public struct Point {public int X,Y;}
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr window,uint flags);
    [DllImport("user32.dll")] static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll",SetLastError=true)] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    static bool OwnInteractiveWindow(IntPtr window,uint process){
        uint owner;GetWindowThreadProcessId(window,out owner);
        return owner==process && GetForegroundWindow()==window && InputDesktopAvailable();
    }
    public static uint ClickOwnedPoint(IntPtr expectedWindow,uint expectedProcess,int x,int y){
        var previous=SetThreadDpiAwarenessContext(new IntPtr(-4));
        if(previous==IntPtr.Zero)throw Error();
        try{
            if(!OwnInteractiveWindow(expectedWindow,expectedProcess))throw new InvalidOperationException("Owned ordinary Settings lost foreground; no input sent.");
            foreach(var key in new[]{0x01,0x02,0x04,0x05,0x06,0x10,0x11,0x12,0xA0,0xA1,0xA2,0xA3,0xA4,0xA5,0x5B,0x5C,0x20})
                if((GetAsyncKeyState(key)&0x8000)!=0)throw new InvalidOperationException("A mouse button, modifier or Space is held; no input sent.");
            var hit=WindowFromPoint(new Point{X=x,Y=y});
            if(hit==IntPtr.Zero || GetAncestor(hit,2)!=expectedWindow)throw new InvalidOperationException("The switch point is not inside the owned Settings HWND; no input sent.");
            int left=GetSystemMetrics(76),top=GetSystemMetrics(77),width=GetSystemMetrics(78),height=GetSystemMetrics(79);
            if(width<=0 || height<=0 || x<left || y<top || (long)x>=((long)left+width) || (long)y>=((long)top+height))throw new InvalidOperationException("The physical switch point is outside the virtual screen.");
            int nx=(int)Math.Min(65535,(((long)x-left)*65536+32768)/width);
            int ny=(int)Math.Min(65535,(((long)y-top)*65536+32768)/height);
            var tag=UIntPtr.Size==8 ? new UIntPtr(0x4D4F4F4E5354494EUL) : new UIntPtr(0x5354494EU);
            var down=new Input{Type=0,Data=new InputData{Mouse=new MouseInput{X=nx,Y=ny,Flags=0xC003,Extra=tag}}};
            var up=new Input{Type=0,Data=new InputData{Mouse=new MouseInput{X=nx,Y=ny,Flags=0xC005,Extra=tag}}};
            if(!OwnInteractiveWindow(expectedWindow,expectedProcess) || GetAncestor(WindowFromPoint(new Point{X=x,Y=y}),2)!=expectedWindow)throw new InvalidOperationException("Settings ownership or hit target changed; no input sent.");
            // One pair includes the physical move, down and up. Never retry activation.
            uint sent=SendInput(2,new[]{down,up},InputSize());
            if(sent!=2){
                uint released=0;
                Point cursor;
                if(sent==1 && OwnInteractiveWindow(expectedWindow,expectedProcess) && GetCursorPos(out cursor)
                    && cursor.X==x && cursor.Y==y && GetAncestor(WindowFromPoint(cursor),2)==expectedWindow){
                    var release=new Input{Type=0,Data=new InputData{Mouse=new MouseInput{Flags=4,Extra=tag}}};
                    released=SendInput(1,new[]{release},InputSize());
                }
                throw new InvalidOperationException("Click input was incomplete (events="+sent+", guarded-own-up="+released+"); activation was not retried.");
            }
            return sent;
        }finally{SetThreadDpiAwarenessContext(previous);}
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

    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window,IntPtr dc,uint flags);
}
'@
if($CompileOnly){[pscustomobject]@{compiled=$true;native_ui_executed=$false;input_size=[MoonUsabilityNative]::InputSize()};return}
if(!$Binary -or !$ExpectedSha256 -or !$OutputDirectory -or !$ProtectedConfig){throw 'Binary, ExpectedSha256, fresh OutputDirectory and ProtectedConfig are required.'}
$Binary=(Resolve-Path -LiteralPath $Binary).Path
$ProtectedConfig=(Resolve-Path -LiteralPath $ProtectedConfig).Path
$OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
if(Test-Path -LiteralPath $OutputDirectory){throw 'Choose a fresh output directory; existing evidence is never overwritten.'}
if((Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash -cne $ExpectedSha256.ToUpperInvariant()){throw 'Settings executable hash mismatch.'}
if([MoonInstallNative]::Integrity() -ne 8192 -or [MoonInstallNative]::TokenWord(26) -ne 0 -or (Test-MoonAdministrator)){throw 'Use ordinary medium-integrity PowerShell without UIAccess.'}
if(![MoonUsabilityNative]::InputDesktopAvailable()){throw 'The ordinary input desktop is unavailable.'}
if(@(Get-Process -Name 'moon-settings' -ErrorAction SilentlyContinue).Count){throw 'Close existing Settings first; no existing process was changed.'}
$manifest=[xml][MoonInstallNative]::Manifest($Binary)
$level=$manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
if(!$level -or $level.level -ne 'asInvoker' -or $level.uiAccess -ne 'false'){throw 'Fixture requires an ordinary Settings executable.'}
[void][IO.Directory]::CreateDirectory($OutputDirectory)
$config=Join-Path $OutputDirectory ('owned-'+[guid]::NewGuid().ToString('N')+'.toml')
$protectedHash=(Get-FileHash -LiteralPath $ProtectedConfig -Algorithm SHA256).Hash
$original=@'
# Owned Settings usability fixture; never a personal document.
config-version = 1
workspaces = [{name="Work",icon="W"},{name="Play"}]
rules = [{matches=[{process={equals="moon-usability-fixture"}}],commands=["move-to-workspace Work"]}]
[bar]
widgets = [{id="logo",align="left",format="Moon",click="cheatsheet"},{id="clock",align="right",format="{clock}",max-width=240},{id="custom",align="left",format="Preserve",click="focus left",background="surface0"}]
'@
[IO.File]::WriteAllText($config,$original,[Text.UTF8Encoding]::new($false))
$process=$null;$identity=$null;$token=$null;$failure=$null;$startup=$null;$weather=$null
$startupOriginal=$null;$startupPin=$null;$startupDaemon=$null;$startupLastRequest=$null;$startupRestored=$false
$startupTransitions=[Collections.Generic.List[object]]::new()
$checks=[Collections.Generic.List[string]]::new();$cleanup=[Collections.Generic.List[string]]::new();$trees=[Collections.Generic.List[object]]::new()
$foregroundBefore=[MoonUsabilityNative]::GetForegroundWindow().ToInt64()
function Wait-For([scriptblock]$Condition,[string]$Description,[int]$Seconds=15){
 $until=[DateTime]::UtcNow.AddSeconds($Seconds)
 do{if(&$Condition){return};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $until)
 throw "Timed out: $Description"
}
function Assert-Protected {
 if((Get-FileHash -LiteralPath $ProtectedConfig -Algorithm SHA256).Hash -cne $protectedHash){throw 'The protected personal configuration changed; fixture actions stopped.'}
}
function Assert-Process {
 if(!$process -or !$identity){throw 'No verified Settings child is owned.'}
 $process.Refresh();if($process.HasExited){throw 'The owned Settings child exited.'}
 $actual=[MoonInstallNative]::ProcessIdentity([uint32]$process.Id)
 if($actual.Birth -ne $identity.Birth -or ![string]::Equals($actual.Path,$Binary,[StringComparison]::OrdinalIgnoreCase)){throw 'The retained Settings identity changed; actions refused.'}
}
function Main-Handle {
 Assert-Process
 return [MoonUsabilityNative]::FindVisibleWindow([uint32]$process.Id)
}
function Main-Root {
 $handle=Main-Handle;if($handle -eq [IntPtr]::Zero){return $null}
 $window=[Windows.Automation.AutomationElement]::FromHandle($handle)
 if($window.Current.ProcessId -ne $process.Id -or $window.Current.Name -cne 'Moon Settings'){throw 'Settings UI Automation ownership changed.'}
 return $window
}
function Assert-Action {
 Assert-Process;Assert-Protected
 if(![MoonUsabilityNative]::InputDesktopAvailable()){throw 'The ordinary input desktop is no longer available.'}
 $handle=Main-Handle
 if($handle -eq [IntPtr]::Zero -or [MoonUsabilityNative]::GetForegroundWindow() -ne $handle){throw 'Owned Settings no longer has foreground; no action sent.'}
}
function Find-Control([string]$Name,[object]$Pattern=$null,[Windows.Automation.AutomationElement]$Root=$null){
 if(!$Root){$Root=Main-Root};if(!$Root){return $null}
 $items=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$Name))
 $matches=@(foreach($item in $items){$supported=$null;if(!$Pattern -or $item.TryGetCurrentPattern($Pattern,[ref]$supported)){$item}})
 if($matches.Count -gt 1){throw "Ambiguous owned control '$Name' with the requested role/pattern."}
 if($matches.Count -eq 1){return $matches[0]};return $null
}
function Click([string]$Name,[Windows.Automation.AutomationElement]$Root=$null){
 Assert-Action;$item=Find-Control $Name ([Windows.Automation.InvokePattern]::Pattern) $Root
 if(!$item -or !$item.Current.IsEnabled){throw "Action unavailable: $Name"}
 ([Windows.Automation.InvokePattern]$item.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)).Invoke()
}
function Set-Value([string]$Name,[string]$Value){
 Assert-Action;$item=Find-Control $Name ([Windows.Automation.ValuePattern]::Pattern)
 if(!$item -or !$item.Current.IsEnabled){throw "Editable control unavailable: $Name"}
 ([Windows.Automation.ValuePattern]$item.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).SetValue($Value)
}
function Get-Value([string]$Name){
 $item=Find-Control $Name ([Windows.Automation.ValuePattern]::Pattern)
 if(!$item){return $null};return ([Windows.Automation.ValuePattern]$item.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).Current.Value
}
function Select-Option([string]$Name,[string]$Option){
 Assert-Action;$control=Find-Control $Name ([Windows.Automation.ExpandCollapsePattern]::Pattern)
 if(!$control -or $control.Current.ControlType -ne [Windows.Automation.ControlType]::ComboBox){throw "Selection control unavailable: $Name"}
 # Chromium exposes ExpandCollapse on the select; SelectionItem belongs to its
 # opened list options, not SelectionPattern on the collapsed ComboBox itself.
 $expand=[Windows.Automation.ExpandCollapsePattern]$control.GetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern)
 $expand.Expand()
 Wait-For {Find-Control $Option ([Windows.Automation.SelectionItemPattern]::Pattern)} "Owned option $Option"
 Assert-Action;$item=Find-Control $Option ([Windows.Automation.SelectionItemPattern]::Pattern)
 ([Windows.Automation.SelectionItemPattern]$item.GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern)).Select()
 try{$expand.Collapse()}catch{}
}
function Find-IconChoice([string]$Name) {
 $dialog=Find-Control 'Choose a workspace icon' ([Windows.Automation.WindowPattern]::Pattern)
 if(!$dialog){return $null}
 foreach($pattern in @([Windows.Automation.InvokePattern]::Pattern,[Windows.Automation.TogglePattern]::Pattern)){
  $item=Find-Control $Name $pattern $dialog
  if($item){return [pscustomobject]@{element=$item;pattern=$pattern}}
 }
 return $null
}
function Choose-Icon([string]$Name) {
 Assert-Action;$choice=Find-IconChoice $Name
 if(!$choice -or !$choice.element.Current.IsEnabled){throw "Included icon unavailable: $Name"}
 if($choice.pattern -eq [Windows.Automation.InvokePattern]::Pattern){([Windows.Automation.InvokePattern]$choice.element.GetCurrentPattern($choice.pattern)).Invoke()}
 else{([Windows.Automation.TogglePattern]$choice.element.GetCurrentPattern($choice.pattern)).Toggle()}
}
function Read-Source {
 $stream=[IO.File]::Open($config,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
 try{$reader=[IO.StreamReader]::new($stream,[Text.UTF8Encoding]::new($false,$true));try{return $reader.ReadToEnd()}finally{$reader.Dispose()}}finally{$stream.Dispose()}
}
function Assert-NotSaved {if((Read-Source) -cne $original){throw 'Draft-only changes wrote the fixture before Save.'};Assert-Protected}
function Save-Owned {
 Click 'Save changes'
 Wait-For {(Find-Control 'All changes saved') -and !(Find-Control 'Save changes' ([Windows.Automation.InvokePattern]::Pattern)).Current.IsEnabled} 'The native save reply and clean draft'
 Assert-Protected
}
function Screenshot([string]$Name){
 Assert-Action;$handle=Main-Handle;$bounds=[MoonUsabilityNative+Frame]::new()
 if(![MoonUsabilityNative]::GetWindowRect($handle,[ref]$bounds)){throw 'Owned screenshot frame unavailable.'}
 $bitmap=[Drawing.Bitmap]::new($bounds.Right-$bounds.Left,$bounds.Bottom-$bounds.Top);$graphics=[Drawing.Graphics]::FromImage($bitmap)
 try{$dc=$graphics.GetHdc();try{if(![MoonUsabilityNative]::PrintWindow($handle,$dc,2)){throw 'Owned PrintWindow failed.'}}finally{$graphics.ReleaseHdc($dc)};$bitmap.Save((Join-Path $OutputDirectory $Name),[Drawing.Imaging.ImageFormat]::Png)}finally{$graphics.Dispose();$bitmap.Dispose()}
}
function Save-Tree {
 $root=Main-Root;if(!$root){return}
 $nodes=[Collections.Generic.List[object]]::new();$queue=[Collections.Generic.Queue[object]]::new();$queue.Enqueue([pscustomobject]@{element=$root;depth=0})
 $walker=[Windows.Automation.TreeWalker]::ControlViewWalker
 while($queue.Count -and $nodes.Count -lt 240){
  $entry=$queue.Dequeue();$item=$entry.element;$state=$item.Current
  $nodes.Add([pscustomobject]@{depth=$entry.depth;name=$state.Name.Substring(0,[Math]::Min(120,$state.Name.Length));type=$state.ControlType.ProgrammaticName;enabled=$state.IsEnabled;offscreen=$state.IsOffscreen;focused=$state.HasKeyboardFocus;focusable=$state.IsKeyboardFocusable;bounds=if($state.BoundingRectangle.IsEmpty){$null}else{[pscustomobject]@{x=$state.BoundingRectangle.X;y=$state.BoundingRectangle.Y;width=$state.BoundingRectangle.Width;height=$state.BoundingRectangle.Height}};patterns=@($item.GetSupportedPatterns()|ForEach-Object {$_.ProgrammaticName})})
  if($entry.depth -lt 12){$child=$walker.GetFirstChild($item);while($child -and $queue.Count -lt 240){$queue.Enqueue([pscustomobject]@{element=$child;depth=$entry.depth+1});$child=$walker.GetNextSibling($child)}}
 }
 $trees.Add([pscustomobject]@{bounded=$true;controls=$nodes.ToArray()})
}
function Assert-StartupPin([switch]$SkipHealth) {
 if(!$startupPin){throw 'No installed startup task was captured.'}
 foreach($entry in $startupPin.files){if((Get-FileHash -LiteralPath $entry.path -Algorithm SHA256).Hash -cne $entry.sha256){throw 'Installed startup identity changed; toggle/restoration refused.'}}
 $status=Get-MoonStartupStatus
 if(!$status.available){throw 'The validated owned startup task is unavailable.'}
 if(!$SkipHealth){
  $live=Invoke-MoonRpc -Method query -Query stats -Installed -ExpectedPeer $startupDaemon.pipe_peer
  if($live.process_id -ne $startupDaemon.process_id -or $live.session_suspended -or $live.last_error){throw 'The captured installed daemon is no longer healthy; startup verification stopped.'}
 }
 return $status
}
function Toggle-Startup([bool]$Expected) {
 $status=Assert-StartupPin
 if($status.enabled -eq $Expected){throw 'Startup toggle would be a no-op; the expected previous state changed.'}
 Assert-Action
 $switch=Find-Control 'Launch Moon when I sign in' ([Windows.Automation.TogglePattern]::Pattern)
 if(!$switch -or !$switch.Current.IsEnabled){throw 'The confirmed startup switch is unavailable.'}
 $pattern=[Windows.Automation.TogglePattern]$switch.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern)
 if(($pattern.Current.ToggleState -eq [Windows.Automation.ToggleState]::On) -ne $status.enabled){throw 'UI and independently read startup state disagree.'}
 # The observed UIA Toggle/SetFocus paths did not produce an activation. Use one
 # actual click only after verifying the exact element at its physical point.
 $point=$switch.GetClickablePoint()
 $point=[System.Windows.Point]::new([Math]::Floor($point.X),[Math]::Floor($point.Y))
 $current=Find-Control 'Launch Moon when I sign in' ([Windows.Automation.TogglePattern]::Pattern)
 if(!$current -or ![Windows.Automation.Automation]::Compare($current,$switch) -or
    !$switch.Current.IsEnabled -or $switch.Current.IsOffscreen -or
    !$switch.Current.BoundingRectangle.Contains($point)){throw 'Startup switch identity or clickable bounds changed; no click sent.'}
 Assert-Action
 $hit=[Windows.Automation.AutomationElement]::FromPoint($point)
 if(!$hit -or ![Windows.Automation.Automation]::Compare($hit,$switch)){throw 'UIA hit test does not identify the exact startup switch; no click sent.'}
 $script:startupLastRequest=[DateTime]::UtcNow
 $inserted=[MoonUsabilityNative]::ClickOwnedPoint((Main-Handle),[uint32]$process.Id,[int]$point.X,[int]$point.Y)
 Wait-For {
  $control=Find-Control 'Launch Moon when I sign in' ([Windows.Automation.TogglePattern]::Pattern)
  $confirmed=Assert-StartupPin
  $control -and $control.Current.IsEnabled -and $confirmed.enabled -eq $Expected -and
   (([Windows.Automation.TogglePattern]$control.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern)).Current.ToggleState -eq [Windows.Automation.ToggleState]::On) -eq $Expected
 } 'Native startup toggle confirmed by UI and independently validated task'
 $startupTransitions.Add([pscustomobject]@{before_enabled=[bool]$status.enabled;after_enabled=$Expected;activation='guarded-click';inserted_events=$inserted;hit_control_verified=$true;point=[pscustomobject]@{x=$point.X;y=$point.Y};task_readback_verified=$true})
 Assert-NotSaved
}
function Restore-Startup {
 if(!$startupPin -or $null -eq $startupOriginal -or !$startupLastRequest){return}
 # The native helper has a 15-second child deadline plus a one-second output deadline.
 # Drain that bounded interval before restoring, so a late UI reply cannot reverse cleanup.
 if($startupLastRequest){$remaining=18-([DateTime]::UtcNow-$startupLastRequest).TotalSeconds;if($remaining -gt 0){Start-Sleep -Milliseconds ([int][Math]::Ceiling($remaining*1000))}}
 $status=Assert-StartupPin -SkipHealth
 if($status.enabled -ne $startupOriginal){$null=Set-MoonStartupEnabled -Enabled $startupOriginal}
 if((Assert-StartupPin -SkipHealth).enabled -ne $startupOriginal){throw 'The original owned startup preference was not restored.'}
 $script:startupRestored=$true
}
function Close-Owned {
 if(!$process){return};$process.Refresh();if($process.HasExited){return}
 Assert-Process
 # WM_CLOSE is intentionally deferred by Settings while an editor modal is open.
 # Dismiss only the exact fixture-owned icon dialog, with no global Escape key.
 $dialog=Find-Control 'Choose a workspace icon' ([Windows.Automation.WindowPattern]::Pattern)
 if($dialog){Click 'Cancel' $dialog;Wait-For {!(Find-Control 'Choose a workspace icon' ([Windows.Automation.WindowPattern]::Pattern))} 'Owned icon dialog dismissed before closing' 3}
 $handle=Main-Handle;if($handle -eq [IntPtr]::Zero){throw 'No verified Settings HWND for graceful cleanup.'}
 if(![MoonUsabilityNative]::PostMessage($handle,0x10,[IntPtr]::Zero,[IntPtr]::Zero)){throw 'Owned close message was refused.'}
 if(!$process.WaitForExit(1000)){
  # Discard is authorized only for the unique fixture created by this harness.
  $discard=Find-Control 'Discard and close' ([Windows.Automation.InvokePattern]::Pattern)
  if($discard){Click 'Discard and close'}
  if(!$process.WaitForExit(5000)){throw 'Owned Settings stayed open; no forced process termination was used.'}
 }
}
try {
 $process=Start-Process -FilePath $Binary -ArgumentList @('--config',(ConvertTo-MoonArgument $config)) -PassThru
 $null=$process.Handle
 $identity=[MoonInstallNative]::ProcessIdentity([uint32]$process.Id)
 [IO.File]::WriteAllText((Join-Path $OutputDirectory 'owned-process.json'),([pscustomobject]@{process_id=$process.Id;birth=$identity.Birth;path=$identity.Path;config=$config}|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
 Assert-Process;$token=[MoonUsabilityNative]::ReadToken($process.Handle)
 if($token.integrity_rid -ne 8192 -or $token.ui_access -or $token.elevated){throw 'The Settings child has unexpected token privileges.'}
 Wait-For {(Main-Handle) -ne [IntPtr]::Zero} 'Visible owned Settings window'
 Wait-For {Find-Control 'General' ([Windows.Automation.InvokePattern]::Pattern)} 'Loaded private Settings document'
 Wait-For {Find-Control 'Launch Moon when I sign in' ([Windows.Automation.TogglePattern]::Pattern)} 'Startup preference control'
 Wait-For {!(Find-Control ('Checking your installed startup preference'+[char]0x2026))} 'Startup status reply'
 $switch=Find-Control 'Launch Moon when I sign in' ([Windows.Automation.TogglePattern]::Pattern)
 $startup=[pscustomobject]@{available=$switch.Current.IsEnabled;toggle_state=([Windows.Automation.TogglePattern]$switch.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern)).Current.ToggleState.ToString();toggled=$false}
 if(Find-Control 'Retry startup status' ([Windows.Automation.InvokePattern]::Pattern)){throw 'Startup status returned an error rather than an available/unavailable result.'}
 if($VerifyStartupToggle){
  $startupDaemon=Invoke-MoonRpc -Method query -Query stats -Installed
  if(!$startupDaemon.token.ui_access -or $startupDaemon.token.elevated -or $startupDaemon.session_suspended -or $startupDaemon.last_error){throw 'Startup toggle verification requires the healthy installed UIAccess daemon.'}
  $directory=Get-MoonInstallDirectory
  $pins=@(foreach($name in @('installation.json','install-common.ps1','moon.exe')){$path=Join-Path $directory $name;[pscustomobject]@{path=$path;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash}})
  $startupPin=[pscustomobject]@{files=$pins}
  $initial=Assert-StartupPin;$startupOriginal=[bool]$initial.enabled
  if(!$switch.Current.IsEnabled){throw 'Installed Settings does not yet offer the startup preference; update the installation before this opt-in check.'}
  Toggle-Startup (!$startupOriginal)
  Toggle-Startup $startupOriginal
  $startup.toggled=$true;$startup | Add-Member -NotePropertyName original_enabled -NotePropertyValue $startupOriginal
  $startupRestored=$true
  $checks.Add('Opt-in native startup toggle changes and restores the exact owned task; independent COM readback confirms both values and the same daemon stays healthy')
 }else{$checks.Add('Startup status is visible; an older installation may honestly report unavailable. No toggle was invoked')}
 Click 'Bar'
 Wait-For {Find-Control 'City or town' ([Windows.Automation.ValuePattern]::Pattern)} 'Weather city search'
 if(!(Find-Control 'Use current location' ([Windows.Automation.InvokePattern]::Pattern))){throw 'Explicit current-location action is missing.'}
 if(!(Find-Control 'Open-Meteo' ([Windows.Automation.InvokePattern]::Pattern)) -or !(Find-Control 'GeoNames' ([Windows.Automation.InvokePattern]::Pattern))){throw 'Visible weather attribution is missing.'}
 Set-Value 'City or town' 'Sydney';Click 'Find city'
 Wait-For {Find-Control 'Sydney, New South Wales, Australia' ([Windows.Automation.InvokePattern]::Pattern)} 'Live, disambiguated Sydney result' 30
 Assert-NotSaved
 Click 'Sydney, New South Wales, Australia'
 Wait-For {(Find-Control 'Save changes' ([Windows.Automation.InvokePattern]::Pattern)).Current.IsEnabled} 'Explicit city selection stages a draft'
 Assert-NotSaved;Screenshot 'weather.png';Save-Owned
 $saved=Read-Source
 $latitude=[regex]::Match($saved,'\blatitude\s*=\s*(-?\d+(?:\.\d+)?)');$longitude=[regex]::Match($saved,'\blongitude\s*=\s*(-?\d+(?:\.\d+)?)')
 if(!$latitude.Success -or !$longitude.Success -or $saved -notmatch 'Sydney, New South Wales, Australia'){throw 'The actual saved fixture lacks the chosen city/coordinates.'}
 $lat=[double]::Parse($latitude.Groups[1].Value,[Globalization.CultureInfo]::InvariantCulture);$lon=[double]::Parse($longitude.Groups[1].Value,[Globalization.CultureInfo]::InvariantCulture)
 if($lat -lt -34 -or $lat -gt -33 -or $lon -lt 151 -or $lon -gt 152){throw 'Saved coordinates are not the selected Sydney region.'}
 $weather=[pscustomobject]@{label='Sydney, New South Wales, Australia';latitude=$lat;longitude=$lon;live_lookup=$true;windows_location_requested=$false}
 $checks.Add('Live Sydney lookup requires explicit city selection and Save; independent fixture bytes contain the chosen region')
 Click 'Workspaces'
 Wait-For {Find-Control 'Icon for workspace Work' ([Windows.Automation.ValuePattern]::Pattern)} 'Workspace icon field'
 $icon=Find-Control 'Icon for workspace Work' ([Windows.Automation.ValuePattern]::Pattern);$walker=[Windows.Automation.TreeWalker]::ControlViewWalker
 # WebView's control tree flattens anonymous card wrappers. Associate the picker
 # with its immediately preceding labelled input (or that input's label wrapper),
 # never with the first of the two identically named buttons on the page.
 $candidates=(Main-Root).FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,'Choose icon'))
 $associated=@(foreach($candidate in $candidates){
  $invoke=$null;if(!$candidate.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern,[ref]$invoke)){continue}
  $previous=$walker.GetPreviousSibling($candidate);if(!$previous){continue}
  $input=$null
  if([Windows.Automation.Automation]::Compare($previous,$icon)){$input=$previous}else{$input=Find-Control 'Icon for workspace Work' ([Windows.Automation.ValuePattern]::Pattern) $previous}
  if($input -and [Windows.Automation.Automation]::Compare($input,$icon)){$candidate}
 })
 if($associated.Count -ne 1){throw 'The Work workspace icon picker was not uniquely associated with its preceding labelled input.'}
 $picker=$associated[0]
 Assert-Action;([Windows.Automation.InvokePattern]$picker.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)).Invoke()
 Wait-For {Find-IconChoice 'Code'} 'Included icon choices'
 # UIA readiness is not proof that the font or compositor has presented pixels.
 Start-Sleep -Milliseconds 500
 Screenshot 'icon-picker.png';Choose-Icon 'Code'
 Wait-For {(Get-Value 'Icon for workspace Work') -ceq [string][char]0xf121} 'Chosen bundled code glyph'
 Save-Owned
 if((Read-Source) -notmatch [regex]::Escape([string][char]0xf121)){throw 'The chosen icon was not persisted to the private fixture.'}
 $checks.Add('Bundled workspace icon selection updates the field and persists the exact Unicode glyph')
 Click 'Bar'
 Wait-For {Find-Control 'Find city' ([Windows.Automation.InvokePattern]::Pattern)} 'Bar page after icon save'
 $summary=Find-Control ([string][char]0x203a+' Moon Left') ([Windows.Automation.ExpandCollapsePattern]::Pattern)
 if(!$summary){throw 'Expected one owned Moon bar-item summary.'}
 Assert-Action;([Windows.Automation.ExpandCollapsePattern]$summary.GetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern)).Expand()
 Wait-For {Find-Control 'Move later: Moon' ([Windows.Automation.InvokePattern]::Pattern)} 'Bar item ordering controls'
 Click 'Move later: Moon';Save-Owned
 $saved=Read-Source;$ids=@([regex]::Matches($saved,'\bid\s*=\s*"([^"]+)"')|ForEach-Object {$_.Groups[1].Value})
 if(($ids -join ',') -cne 'clock,logo,custom' -or $saved -notmatch 'cheatsheet' -or $saved -notmatch 'Preserve' -or $saved -notmatch 'max-width\s*=\s*240'){throw 'Saved bar order or unrelated custom attributes changed unexpectedly.'}
 Screenshot 'bar.png';$checks.Add('Move later changes actual saved bar order while preserving custom commands and styling')
 Click 'Rules';Wait-For {Find-Control 'App behavior' ([Windows.Automation.ExpandCollapsePattern]::Pattern)} 'Structured personal-rule controls'
 Select-Option 'App behavior' 'Keep floating';Save-Owned
 $saved=Read-Source
 if($saved -notmatch 'commands\s*=\s*\[\s*"set-floating"\s*\]' -or $saved -notmatch 'moon-usability-fixture'){throw 'Structured rule edit did not preserve its app match and persist the chosen action.'}
 Screenshot 'rules.png';$checks.Add('Structured behavior selection persists a floating rule without rewriting its app matcher')
 Assert-Protected
}catch{
 $failure=$_
 try{Save-Tree}catch{$cleanup.Add('Could not capture owned control tree: '+$_.Exception.Message)}
}finally{
 try{Restore-Startup}catch{$cleanup.Add($_.Exception.Message);if(!$failure){$failure=$_}}
 try{Close-Owned}catch{$cleanup.Add($_.Exception.Message);if(!$failure){$failure=$_}}
 if($process){$process.Dispose()}
 try{Assert-Protected}catch{$cleanup.Add($_.Exception.Message);if(!$failure){$failure=$_}}
}
$protectedUnchanged=$false
try{$protectedUnchanged=(Get-FileHash -LiteralPath $ProtectedConfig -Algorithm SHA256).Hash -ceq $protectedHash}catch{$cleanup.Add($_.Exception.Message);if(!$failure){$failure=$_}}
$report=[pscustomobject]@{
 passed=($null -eq $failure);binary=$Binary;sha256=$ExpectedSha256.ToUpperInvariant();config=$config
 checks=$checks.ToArray();startup=$startup;startup_transitions=$startupTransitions.ToArray();weather=$weather;token=$token;protected_config=$ProtectedConfig;protected_sha256=$protectedHash
 personal_config_unchanged=$protectedUnchanged
 daemon_lifecycle_requested=$false;startup_pointer_input_attempted=($null -ne $startupLastRequest);cursor_restoration_attempted=$false;startup_toggle_requested=[bool]$VerifyStartupToggle;startup_restored=$startupRestored;windows_location_requested=$false
 foreground_before=$foregroundBefore;foreground_after=[MoonUsabilityNative]::GetForegroundWindow().ToInt64()
 cleanup_errors=$cleanup.ToArray();error=if($failure){$failure.Exception.Message}else{$null}
}
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'control-trees.json'),($trees.ToArray()|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'report.json'),($report|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
$report|ConvertTo-Json -Depth 8
if($failure){throw $failure}
