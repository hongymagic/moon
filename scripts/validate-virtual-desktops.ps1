# Interactive acceptance: run only in an exclusively coordinated desktop slot.
# Public contracts: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager
# Shortcuts: https://learn.microsoft.com/en-us/accessibility/windows/keyboard-shortcuts-in-windows
[CmdletBinding(DefaultParameterSetName='Run')]
param(
    [Parameter(Mandatory,ParameterSetName='Run')][string]$TestwinPath,
    [Parameter(Mandatory,ParameterSetName='Run')][ValidateRange(1,2147483647)][int]$ExpectedDaemonId,
    [Parameter(Mandatory,ParameterSetName='Run')][string]$ExpectedDaemonPath,
    [Parameter(Mandatory,ParameterSetName='Run')][ValidatePattern('^[A-Fa-f0-9]{64}$')][string]$ExpectedDaemonSha256,
    [string]$ReportPath,
    [Parameter(Mandatory,ParameterSetName='Compile')][switch]$CompileOnly
)
. (Join-Path $PSScriptRoot 'install-common.ps1')
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
public static class MoonDesktopNative {
 [ComImport,Guid("A5CD92FF-29BE-454C-8D04-D82879FB3F1B"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
 interface IDesktopManager {
  [PreserveSig]int IsWindowOnCurrentVirtualDesktop(IntPtr window,[MarshalAs(UnmanagedType.Bool)]out bool current);
  [PreserveSig]int GetWindowDesktopId(IntPtr window,out Guid desktop);
  [PreserveSig]int MoveWindowToDesktop(IntPtr window,ref Guid desktop);
 }
 [StructLayout(LayoutKind.Sequential)]public struct Rect {public int Left,Top,Right,Bottom;}
 [StructLayout(LayoutKind.Sequential)]struct Mouse {public int X,Y;public uint Data,Flags,Time;public UIntPtr Extra;}
 [StructLayout(LayoutKind.Sequential)]struct Key {public ushort Vk,Scan;public uint Flags,Time;public UIntPtr Extra;}
 [StructLayout(LayoutKind.Explicit)]struct InputUnion {[FieldOffset(0)]public Mouse Mouse;[FieldOffset(0)]public Key Key;}
 [StructLayout(LayoutKind.Sequential)]struct Input {public uint Type;public InputUnion Value;}
 [StructLayout(LayoutKind.Sequential)]struct KeyboardHook {public uint Vk,Scan,Flags,Time;public UIntPtr Extra;}
 [StructLayout(LayoutKind.Sequential)]struct Message {public IntPtr Window;public uint Id;public UIntPtr W;public IntPtr L;public uint Time;public int X,Y;public uint Private;}
 public sealed class Window {public long Id;public uint ProcessId;public string Class,Desktop,Error,ErrorStage;public int DesktopHResult,ErrorHResult;public bool Visible,Current;public uint Cloak;public Rect Outer,Frame;}
 delegate bool EnumProc(IntPtr window,IntPtr data);
 delegate IntPtr HookProc(int code,UIntPtr w,IntPtr l);
 static IDesktopManager manager;
 static readonly HookProc keyboardHook=KeyboardEvent,mouseHook=MouseEvent;
 static readonly ManualResetEventSlim ready=new ManualResetEventSlim(false),heartbeat=new ManualResetEventSlim(false),ownAck=new ManualResetEventSlim(false);
 static readonly long ownTag=BitConverter.ToInt64(Guid.NewGuid().ToByteArray(),0);
 static Thread guardThread;static uint guardThreadId;static volatile string guardFailure;
 static long foreignEvents,ownEvents;static int pendingFinalKey;
 const uint Ping=0x8065;
 [DllImport("user32.dll")]static extern bool EnumWindows(EnumProc callback,IntPtr data);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int GetClassName(IntPtr window,StringBuilder name,int size);
 [DllImport("user32.dll")]static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
 [DllImport("user32.dll")]static extern bool IsWindowVisible(IntPtr window);
 [DllImport("user32.dll",SetLastError=true)]static extern bool GetWindowRect(IntPtr window,out Rect rect);
 [DllImport("user32.dll")]public static extern bool IsWindow(IntPtr window);
 [DllImport("user32.dll")]public static extern IntPtr GetShellWindow();
 [DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")]public static extern bool SetForegroundWindow(IntPtr window);
 [DllImport("user32.dll")]public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
 [DllImport("user32.dll",SetLastError=true)]public static extern bool PostMessage(IntPtr window,uint message,IntPtr w,IntPtr l);
 [DllImport("user32.dll")]static extern short GetAsyncKeyState(int key);
 [DllImport("user32.dll",SetLastError=true)]static extern uint SendInput(uint count,Input[] inputs,int size);
 [DllImport("user32.dll",EntryPoint="SetWindowsHookExW",SetLastError=true)]static extern IntPtr SetWindowsHookEx(int kind,HookProc callback,IntPtr module,uint thread);
 [DllImport("user32.dll",SetLastError=true)]static extern bool UnhookWindowsHookEx(IntPtr hook);
 [DllImport("user32.dll")]static extern IntPtr CallNextHookEx(IntPtr hook,int code,UIntPtr w,IntPtr l);
 [DllImport("user32.dll",EntryPoint="GetMessageW",SetLastError=true)]static extern int GetMessage(out Message message,IntPtr window,uint first,uint last);
 [DllImport("user32.dll",EntryPoint="PeekMessageW")]static extern bool PeekMessage(out Message message,IntPtr window,uint first,uint last,uint remove);
 [DllImport("user32.dll",EntryPoint="PostThreadMessageW",SetLastError=true)]static extern bool PostThreadMessage(uint thread,uint message,UIntPtr w,IntPtr l);
 [DllImport("kernel32.dll")]static extern uint GetCurrentThreadId();
 [DllImport("kernel32.dll",EntryPoint="GetModuleHandleW",CharSet=CharSet.Unicode)]static extern IntPtr GetModuleHandle(string name);
 [DllImport("dwmapi.dll")]static extern int DwmGetWindowAttribute(IntPtr window,uint attribute,out uint value,uint size);
 [DllImport("dwmapi.dll",EntryPoint="DwmGetWindowAttribute")]static extern int GetFrame(IntPtr window,uint attribute,out Rect value,uint size);
 [DllImport("kernel32.dll",SetLastError=true)]public static extern bool TerminateProcess(IntPtr process,uint exitCode);
 public static int InputSize(){return Marshal.SizeOf(typeof(Input));}
 public static void Initialize(){manager=(IDesktopManager)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("AA509086-5CA9-4C25-8F95-589D3C07B48A")));}
 public static void Release(){if(manager!=null){Marshal.FinalReleaseComObject(manager);manager=null;}}
 public static uint Owner(long window){uint process;GetWindowThreadProcessId(new IntPtr(window),out process);return process;}
 static uint Modifier(uint key){return key==0xa0||key==0xa1?16u:key==0xa2||key==0xa3?17u:key==0xa4||key==0xa5?18u:key;}
 static IntPtr KeyboardEvent(int code,UIntPtr w,IntPtr l){
  if(code>=0)try{
   var input=(KeyboardHook)Marshal.PtrToStructure(l,typeof(KeyboardHook));
   bool injected=(input.Flags&0x10)!=0,lower=(input.Flags&2)!=0,up=(input.Flags&0x80)!=0;
   if(injected&&!lower&&input.Extra.ToUInt64()==unchecked((ulong)ownTag)){
    Interlocked.Increment(ref ownEvents);if(up&&Modifier(input.Vk)==Volatile.Read(ref pendingFinalKey))ownAck.Set();
   }else if(!(injected&&!lower&&up&&input.Extra.ToUInt64()==0x4d4f4f4eUL&&input.Vk==0xe8&&input.Scan==0))Interlocked.Increment(ref foreignEvents);
  }catch(Exception error){guardFailure=error.Message;}
  return CallNextHookEx(IntPtr.Zero,code,w,l);
 }
 static IntPtr MouseEvent(int code,UIntPtr w,IntPtr l){if(code>=0)Interlocked.Increment(ref foreignEvents);return CallNextHookEx(IntPtr.Zero,code,w,l);}
 public static void StartGuard(){
  if(guardThread!=null)throw new InvalidOperationException("Input guard already started.");
  guardThread=new Thread(()=>{
   IntPtr keyboard=IntPtr.Zero,mouse=IntPtr.Zero;
   try{
    guardThreadId=GetCurrentThreadId();Message message;PeekMessage(out message,IntPtr.Zero,0,0,0);
    var module=GetModuleHandle(null);keyboard=SetWindowsHookEx(13,keyboardHook,module,0);if(keyboard==IntPtr.Zero)throw new System.ComponentModel.Win32Exception();
    mouse=SetWindowsHookEx(14,mouseHook,module,0);if(mouse==IntPtr.Zero)throw new System.ComponentModel.Win32Exception();ready.Set();
    for(;;){int result=GetMessage(out message,IntPtr.Zero,0,0);if(result<0)throw new System.ComponentModel.Win32Exception();if(result==0)break;if(message.Id==Ping)heartbeat.Set();}
   }catch(Exception error){guardFailure=error.Message;}
   finally{if(mouse!=IntPtr.Zero&&!UnhookWindowsHookEx(mouse))guardFailure="Mouse hook removal failed.";if(keyboard!=IntPtr.Zero&&!UnhookWindowsHookEx(keyboard))guardFailure="Keyboard hook removal failed.";ready.Set();}
  });
  guardThread.IsBackground=true;guardThread.Start();if(!ready.Wait(2000))throw new InvalidOperationException("Input guard startup timed out.");CheckGuard(0);
 }
 public static void CheckGuard(long expectedGeneration){
  if(guardThread==null||!guardThread.IsAlive||guardFailure!=null)throw new InvalidOperationException("Input guard unavailable: "+guardFailure);
  heartbeat.Reset();if(!PostThreadMessage(guardThreadId,Ping,UIntPtr.Zero,IntPtr.Zero)||!heartbeat.Wait(500))throw new InvalidOperationException("Input guard pump failed.");
  if(guardFailure!=null||!guardThread.IsAlive||Interlocked.Read(ref foreignEvents)!=expectedGeneration)throw new InvalidOperationException("Foreign input or input guard failure detected.");
 }
 public static long ForeignEvents(){return Interlocked.Read(ref foreignEvents);}
 public static long OwnEvents(){return Interlocked.Read(ref ownEvents);}
 public static void StopGuard(){
  if(guardThread==null)return;
  if(guardThread.IsAlive&&(!PostThreadMessage(guardThreadId,0x12,UIntPtr.Zero,IntPtr.Zero)||!guardThread.Join(2000)))throw new InvalidOperationException("Input guard shutdown failed.");
  if(guardFailure!=null)throw new InvalidOperationException(guardFailure);
 }
 public static Window Read(long window){
  var handle=new IntPtr(window);if(!IsWindow(handle))throw new InvalidOperationException("Window no longer exists.");
  var value=new Window{Id=window,ProcessId=Owner(window),Visible=IsWindowVisible(handle)};
  var name=new StringBuilder(256);GetClassName(handle,name,name.Capacity);value.Class=name.ToString();
  string stage="GetWindowDesktopId";
  try {Guid desktop;bool current;value.DesktopHResult=manager.GetWindowDesktopId(handle,out desktop);Marshal.ThrowExceptionForHR(value.DesktopHResult);value.Desktop=desktop.ToString();
   stage="IsWindowOnCurrentVirtualDesktop";Marshal.ThrowExceptionForHR(manager.IsWindowOnCurrentVirtualDesktop(handle,out current));value.Current=current;
   stage="DwmGetCloak";Marshal.ThrowExceptionForHR(DwmGetWindowAttribute(handle,14,out value.Cloak,4));
   stage="GetWindowRect";if(!GetWindowRect(handle,out value.Outer))throw new System.ComponentModel.Win32Exception();
   stage="DwmGetFrame";Marshal.ThrowExceptionForHR(GetFrame(handle,9,out value.Frame,16));
  }catch(Exception error){value.Error=error.Message;value.ErrorStage=stage;value.ErrorHResult=error.HResult;}
  return value;
 }
 public static Window[] Windows(){
  var result=new List<Window>();string failure=null;
  if(!EnumWindows((h,p)=>{try{if(result.Count>=2048)throw new InvalidOperationException("Window inventory exceeded its bound.");result.Add(Read(h.ToInt64()));return true;}catch(Exception error){failure=error.Message;return false;}},IntPtr.Zero))throw new InvalidOperationException(failure??"Window enumeration failed.");
  return result.ToArray();
 }
 public static long Chord(ushort[] keys,long generation,long expectedForeground){
  if(keys.Length<2||keys.Length>5)throw new InvalidOperationException("Invalid bounded chord.");
  CheckGuard(generation);
  if(expectedForeground!=0&&GetForegroundWindow().ToInt64()!=expectedForeground)throw new InvalidOperationException("The owned helper lost foreground; no shortcut was sent.");
  for(int key=1;key<256;key++)if((GetAsyncKeyState(key)&0x8000)!=0)throw new InvalidOperationException("An input is held; no shortcut was sent.");
  var inputs=new Input[keys.Length*2];
  var tag=new UIntPtr(unchecked((ulong)ownTag));
  for(int i=0;i<keys.Length;i++){inputs[i]=new Input{Type=1,Value=new InputUnion{Key=new Key{Vk=keys[i],Extra=tag}}};inputs[keys.Length+i]=new Input{Type=1,Value=new InputUnion{Key=new Key{Vk=keys[keys.Length-1-i],Flags=2,Extra=tag}}};}
  ownAck.Reset();Volatile.Write(ref pendingFinalKey,keys[0]);
  uint sent=SendInput((uint)inputs.Length,inputs,InputSize());
  if(sent!=inputs.Length){
   // Release only keys whose downs were actually submitted by this call.
   var pending=new List<Input>();for(int i=0;i<keys.Length&&i<sent;i++)if(sent<=keys.Length+(keys.Length-1-i))pending.Add(new Input{Type=1,Value=new InputUnion{Key=new Key{Vk=keys[i],Flags=2,Extra=tag}}});
   if(pending.Count>0)SendInput((uint)pending.Count,pending.ToArray(),InputSize());
   guardFailure="Windows did not accept the complete shortcut.";throw new InvalidOperationException(guardFailure);
  }
  if(!ownAck.Wait(1000)){guardFailure="The input guard did not acknowledge the final owned release.";throw new InvalidOperationException(guardFailure);}
  Volatile.Write(ref pendingFinalKey,0);CheckGuard(generation);return generation;
 }
}
'@
if($CompileOnly){if([MoonDesktopNative]::InputSize() -ne 40){throw 'Expected x64 INPUT size40.'};[pscustomobject]@{compiled=$true;input_size=40;desktop_changed=$false}|ConvertTo-Json;return}
if(!$ReportPath){$ReportPath=Join-Path $env:USERPROFILE 'Downloads\Moon-validation\virtual-desktops.json'}
$ReportPath=[IO.Path]::GetFullPath($ReportPath)
if(Test-Path -LiteralPath $ReportPath){throw 'Choose a new report path; previous recovery evidence is never overwritten.'}
if((Test-MoonAdministrator) -or [MoonInstallNative]::Integrity() -ne 8192 -or [MoonInstallNative]::TokenWord(26)){throw 'Use ordinary medium-integrity PowerShell without UIAccess, launched through the existing Explorer shell.'}
$TestwinPath=(Resolve-Path -LiteralPath $TestwinPath).ProviderPath
$ExpectedDaemonPath=(Resolve-Path -LiteralPath $ExpectedDaemonPath).ProviderPath
if([IO.Path]::GetFileName($TestwinPath) -ine 'testwin.exe'){throw 'Provide the dedicated testwin.exe fixture.'}
if((Get-FileHash -LiteralPath $ExpectedDaemonPath -Algorithm SHA256).Hash -ne $ExpectedDaemonSha256){throw 'The expected daemon file hash differs.'}
if(@(Get-Process testwin -ErrorAction SilentlyContinue).Count){throw 'A testwin fixture is already running; it was left untouched.'}
$daemonIdentity=[MoonInstallNative]::ProcessIdentity([uint32]$ExpectedDaemonId)
$baselineStats=Invoke-MoonRpc query stats -DevelopmentPath $ExpectedDaemonPath
if($baselineStats.process_id -ne $ExpectedDaemonId -or $baselineStats.executable -ine $ExpectedDaemonPath -or $daemonIdentity.Path -ine $ExpectedDaemonPath -or $baselineStats.token.ui_access -or $baselineStats.token.elevated -or $baselineStats.token.integrity_rid -ne 8192 -or $baselineStats.token.session_id -ne [Diagnostics.Process]::GetCurrentProcess().SessionId){throw 'The live daemon does not match the explicitly pinned ordinary instance.'}
$baselineWorld=Invoke-MoonRpc query world -DevelopmentPath $ExpectedDaemonPath
if($baselineWorld.paused -or $baselineWorld.mode -ne 'default' -or $baselineWorld.native_fullscreen -or $baselineStats.last_error){throw 'The current Moon session is not ready for an isolated desktop scenario.'}
$baselinePlacements=@(Invoke-MoonRpc query windows -DevelopmentPath $ExpectedDaemonPath)
$baselineFingerprint=Get-MoonLayoutFingerprint -World $baselineWorld -Placements $baselinePlacements
$configExists=Test-Path -LiteralPath $baselineStats.config_path
$configHash=if($configExists){(Get-FileHash -LiteralPath $baselineStats.config_path -Algorithm SHA256).Hash}else{$null}
# Select an explicitly configured, single reversible command. No aliases or modes.
$hookKeys=$null;$hookBinding=$null
foreach($binding in $baselineWorld.config.bindings.PSObject.Properties){
 if(@($binding.Value).Count -ne 1 -or $binding.Value[0] -ne 'toggle-floating'){continue}
 $parts=@($binding.Name.ToLowerInvariant().Split('+')|ForEach-Object{$_.Trim()});$keys=[System.Collections.Generic.List[System.UInt16]]::new();$valid=$parts.Count -ge 2
 for($i=0;$i -lt $parts.Count;$i++){
  $key=switch($parts[$i]){'alt'{18};'ctrl'{17};'control'{17};'shift'{16};'space'{32};default{if($_ -match '^[a-z0-9]$'){[int][char]$_.ToUpperInvariant()}else{0}}}
  if(!$key -or ($i -lt $parts.Count-1 -and $key -notin @(16,17,18)) -or ($i -eq $parts.Count-1 -and $key -in @(16,17,18))){$valid=$false;break};$keys.Add([uint16]$key)
 }
 if($valid -and $keys.Count -le 4){$hookKeys=$keys.ToArray();$hookBinding=$binding.Name;break}
}
if(!$hookKeys){throw 'No supported single-command toggle-floating binding exists; configuration was not changed.'}
# The four OS shortcuts must reach Windows rather than a configured Moon action.
foreach($binding in $baselineWorld.config.bindings.PSObject.Properties){if($binding.Name -match '(?i)(win|super|meta)' -and $binding.Name -match '(?i)(ctrl|control)'){throw 'A configured Win+Ctrl binding could intercept desktop shortcuts; configuration was not changed.'}}
$owned=[Collections.Generic.List[object]]::new();$checks=[Collections.Generic.List[object]]::new();$cleanupErrors=[Collections.Generic.List[string]]::new()
$createdDesktop=$null;$desktopClosed=$false;$creationAttempted=$false;$intervened=$false;$failure=$null;$a=$null;$b=$null;$originalWindows=@();$baselineNative=@();$baselineUnassigned=@();$previous=$null;$shell=$null
$lastInput=0L
$report=[ordered]@{version=1;started_at=[DateTime]::UtcNow.ToString('o');passed=$false;daemon=[pscustomobject]@{id=$ExpectedDaemonId;birth=$daemonIdentity.Birth;path=$ExpectedDaemonPath;sha256=$ExpectedDaemonSha256};fixture_sha256=(Get-FileHash -LiteralPath $TestwinPath -Algorithm SHA256).Hash;configuration_sha256=$configHash;baseline_fingerprint=$baselineFingerprint;hook_binding=$hookBinding;maximum_navigation_hops=16;observations=[Collections.Generic.List[object]]::new()}
function Save-Report {
 $report.checks=@($checks.ToArray());$report.owned=@($owned|ForEach-Object{[pscustomobject]@{id=$_.id;process_id=$_.process.Id;birth=$_.birth;path=$TestwinPath;desktop=$_.desktop}})
 $report.created_desktop=$createdDesktop;$report.created_desktop_closed=$desktopClosed;$report.creation_attempted=$creationAttempted;$report.intervention_detected=$intervened;$report.cleanup_errors=@($cleanupErrors.ToArray())
 [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($ReportPath));$temp=$ReportPath+'.'+[guid]::NewGuid().ToString('N')+'.tmp'
 try{[IO.File]::WriteAllText($temp,($report|ConvertTo-Json -Depth 100),[Text.UTF8Encoding]::new($false));if(Test-Path -LiteralPath $ReportPath){[IO.File]::Replace($temp,$ReportPath,$null)}else{[IO.File]::Move($temp,$ReportPath)}}finally{if(Test-Path -LiteralPath $temp){Remove-Item -LiteralPath $temp -Force}}
}
function Assert-Input {
 try{[MoonDesktopNative]::CheckGuard($script:lastInput)}catch{$script:intervened=$true;throw 'Foreign input or input-guard failure detected; further switching and focus restoration were canceled.'}
}
function Assert-Daemon {
 $identity=[MoonInstallNative]::ProcessIdentity([uint32]$ExpectedDaemonId);$stats=Invoke-MoonRpc query stats -DevelopmentPath $ExpectedDaemonPath
 if($identity.Birth -ne $daemonIdentity.Birth -or $identity.Path -ine $ExpectedDaemonPath -or $stats.process_id -ne $ExpectedDaemonId -or $stats.executable -ine $ExpectedDaemonPath -or $stats.last_error){throw 'The original daemon changed or reported an error.'}
 return $stats
}
function Assert-Window($Window){
 if(![MoonDesktopNative]::IsWindow([IntPtr][long]$Window.id) -or [MoonDesktopNative]::Owner($Window.id) -ne $Window.process_id){throw 'A saved window identity changed.'}
 $identity=[MoonInstallNative]::ProcessIdentity([uint32]$Window.process_id)
 if($identity.Birth -ne $Window.birth -or $identity.Path -ine $Window.path){throw 'A saved process identity changed.'}
}
function Assert-Owned($Helper){
 $Helper.process.Refresh();if($Helper.process.HasExited){throw 'The owned helper exited unexpectedly.'}
 Assert-Window ([pscustomobject]@{id=$Helper.id;process_id=$Helper.process.Id;birth=$Helper.birth;path=$TestwinPath})
 $native=[MoonDesktopNative]::Read($Helper.id);if($native.Error -or !$native.Desktop -or $native.Desktop -eq [guid]::Empty.ToString() -or $native.Class -ne 'MoonTestWindow'){throw 'The owned helper native state is unavailable.'};return $native
}
function Test-Infrastructure($Window){
 if($Window.ProcessId -eq $ExpectedDaemonId -and $Window.Class -in @('moon.surface','moon.notification','moon.carousel','MoonSystemNotifications','tooltips_class32')){return $true}
 return $shell -and $Window.ProcessId -eq $shell.process_id -and $Window.Class -in @('Progman','WorkerW','Shell_TrayWnd','Shell_SecondaryTrayWnd')
}
function Test-NativeAccessDenied($Record){
 $exception=$Record.Exception
 for($depth=0;$exception -and $depth -lt 8;$depth++){
  if($exception -is [ComponentModel.Win32Exception]){return $exception.NativeErrorCode -eq 5}
  $exception=$exception.InnerException
 }
 return $false
}
function Get-CimBirth([uint32]$ProcessId){
 # CreationDate is fixed/read-only; path availability is a separate privilege.
 # https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-process
 $rows=@(Get-CimInstance -ClassName Win32_Process -Filter "ProcessId=$ProcessId" -Property ProcessId,Name,CreationDate,SessionId -OperationTimeoutSec 3 -ErrorAction Stop)
 if($rows.Count -ne 1 -or $rows[0].ProcessId -ne $ProcessId -or $rows[0].CreationDate -isnot [datetime] -or [string]::IsNullOrWhiteSpace($rows[0].Name) -or $null -eq $rows[0].SessionId){throw 'CIM did not provide one complete process birth identity.'}
 $birth=$rows[0].CreationDate.ToFileTimeUtc()
 if($birth -le 0){throw 'CIM process creation time is invalid.'}
 return [pscustomobject]@{name=[string]$rows[0].Name;birth_filetime=$birth;session=[uint32]$rows[0].SessionId}
}
function Get-Inventory {
 param([switch]$Baseline)
 $result=[Collections.Generic.List[object]]::new();$identities=@{};$cimIdentities=@{}
 foreach($window in [MoonDesktopNative]::Windows()){
  if(Test-Infrastructure $window){continue}
  # TYPE_E_ELEMENTNOTFOUND is observed for Windows' non-application HWNDs.
  # GUID_NULL with successful queries is separately retained (e.g. pseudoconsole).
  $unassigned=($window.ErrorStage -eq 'GetWindowDesktopId' -and $window.DesktopHResult -eq -2147319765 -and $window.ErrorHResult -eq -2147319765) -or (!$window.Error -and $window.DesktopHResult -eq 0 -and $window.Desktop -eq [guid]::Empty.ToString())
  if(!$unassigned -and ($window.Error -or !$window.Desktop -or $window.Desktop -eq [guid]::Empty.ToString())){throw "Cannot establish native desktop ownership for HWND $($window.Id), stage $($window.ErrorStage), HRESULT $($window.ErrorHResult); no desktop may be closed."}
  if(!$identities.ContainsKey($window.ProcessId)){
   # Never exclude a RunAs/foreign-user window. Only access denied has a fallback.
   try{$identities[$window.ProcessId]=[MoonInstallNative]::ProcessIdentity($window.ProcessId)}catch{if(!(Test-NativeAccessDenied $_)){throw};$identities[$window.ProcessId]=$null}
  }
  $identity=$identities[$window.ProcessId]
  $source='native';$birth=$null;$path=$null;$cimName=$null;$cimBirth=$null;$cimSession=$null;$identityError=0
  if($identity){$birth=$identity.Birth;$path=$identity.Path}else{
   if(!$unassigned){throw 'An assigned foreign window has an inaccessible process identity; desktop deletion is refused.'}
   $identityError=5
   $known=@($baselineUnassigned|Where-Object{$_.id -eq $window.Id -and $_.process_id -eq $window.ProcessId -and $_.class -ceq $window.Class -and $_.identity_source -eq 'cim-unassigned'})
   if(!$Baseline -and $known.Count -ne 1){throw 'A new inaccessible unassigned window prevents desktop deletion.'}
   if(!$cimIdentities.ContainsKey($window.ProcessId)){$cimIdentities[$window.ProcessId]=Get-CimBirth $window.ProcessId}
   $cim=$cimIdentities[$window.ProcessId];$source='cim-unassigned';$cimName=$cim.name;$cimBirth=$cim.birth_filetime;$cimSession=$cim.session
  }
  if(![MoonDesktopNative]::IsWindow([IntPtr]$window.Id) -or [MoonDesktopNative]::Owner($window.Id) -ne $window.ProcessId){throw 'A window owner changed during process identity lookup.'}
  $result.Add([pscustomobject]@{id=$window.Id;process_id=$window.ProcessId;birth=$birth;path=$path;identity_source=$source;native_identity_error=$identityError;cim_name=$cimName;cim_birth_filetime=$cimBirth;cim_session=$cimSession;class=$window.Class;desktop=$window.Desktop;unassigned=$unassigned;desktop_hresult=$window.DesktopHResult;error_stage=$window.ErrorStage;error_hresult=$window.ErrorHResult;error=$window.Error;current=$window.Current;visible=$window.Visible;cloak=$window.Cloak;outer=$window.Outer;frame=$window.Frame})
 }
 return $result.ToArray()
}
function Assert-UnassignedBaseline([object[]]$Inventory){
 foreach($window in @($Inventory|Where-Object unassigned)){
  $helper=@($owned|Where-Object{$_.process.Id -eq $window.process_id -and $_.birth -eq $window.birth})
  if($helper.Count -eq 1 -and $window.path -ieq $TestwinPath){
   $helper[0].process.Refresh();if($helper[0].process.HasExited){throw 'An owned helper exited during the unassigned-window check.'}
   $identity=[MoonInstallNative]::ProcessIdentity([uint32]$window.process_id)
   if($identity.Birth -ne $helper[0].birth -or $identity.Path -ine $TestwinPath){throw 'The owned helper process identity changed.'};continue
  }
  $before=@($baselineUnassigned|Where-Object{$_.id -eq $window.id})
  if($before.Count -ne 1 -or $before[0].process_id -ne $window.process_id -or $before[0].birth -ne $window.birth -or $before[0].path -ine $window.path -or $before[0].class -cne $window.class -or $before[0].visible -ne $window.visible -or $before[0].desktop -ne $window.desktop -or $before[0].desktop_hresult -ne $window.desktop_hresult -or $before[0].error_stage -cne $window.error_stage -or $before[0].error_hresult -ne $window.error_hresult){throw "A new or changed unassigned foreign HWND $($window.id) prevents desktop deletion."}
  if($before[0].identity_source -cne $window.identity_source -or $before[0].native_identity_error -ne $window.native_identity_error -or $before[0].cim_name -cne $window.cim_name -or $before[0].cim_birth_filetime -ne $window.cim_birth_filetime -or $before[0].cim_session -ne $window.cim_session){throw 'An unassigned process birth identity changed; desktop retained.'}
  if(!$window.error -and ($before[0].current -ne $window.current -or $before[0].cloak -ne $window.cloak -or (($before[0].outer|ConvertTo-Json -Compress) -ne ($window.outer|ConvertTo-Json -Compress)) -or (($before[0].frame|ConvertTo-Json -Compress) -ne ($window.frame|ConvertTo-Json -Compress)))){throw 'A baseline GUID_NULL window changed its observed native state; desktop retained.'}
 }
}
function New-Helper {
 Assert-Input;$null=Assert-Daemon
 $start=[Diagnostics.ProcessStartInfo]::new();$start.FileName=$TestwinPath;$start.Arguments='1';$start.UseShellExecute=$false;$start.CreateNoWindow=$true
 $process=[Diagnostics.Process]::Start($start);$null=$process.Handle
 # Retain ownership before any further fallible operation, including HWND discovery.
 $helper=[pscustomobject]@{process=$process;id=0L;birth=$process.StartTime.ToFileTimeUtc();desktop=$null};$owned.Add($helper);Save-Report
 Wait-MoonCondition -Seconds 5 -Description 'one owned fixture window' -Condition {@([MoonDesktopNative]::Windows()|Where-Object{$_.ProcessId -eq $process.Id -and $_.Class -eq 'MoonTestWindow'}).Count -eq 1}
 $window=@([MoonDesktopNative]::Windows()|Where-Object{$_.ProcessId -eq $process.Id -and $_.Class -eq 'MoonTestWindow'})[0];$helper.id=$window.Id
 $native=Assert-Owned $helper;if(!$native.Current -or $native.Cloak){throw 'The new helper is not visible on the current native desktop.'};$helper.desktop=$native.Desktop
 Wait-MoonCondition -Seconds 5 -Description 'the helper to join Moon' -Condition {$world=Invoke-MoonRpc query world -DevelopmentPath $ExpectedDaemonPath;$item=$world.windows.PSObject.Properties[[string]$helper.id];return $item -and !$item.Value.suspended}
 Save-Report;return $helper
}
function Send-DesktopKey([uint16]$Key,[long]$Foreground=0){
 Assert-Input;$null=Assert-Daemon
 $script:lastInput=[MoonDesktopNative]::Chord([uint16[]]@(91,17,$Key),$script:lastInput,$Foreground)
}
function Focus-Owned($Helper){
 Assert-Input;$native=Assert-Owned $Helper;if(!$native.Current){throw 'Never focus an off-desktop window.'}
 if(![MoonDesktopNative]::SetForegroundWindow([IntPtr][long]$Helper.id)){throw 'Windows refused foreground for the owned helper.'}
 Wait-MoonCondition -Seconds 5 -Description 'owned native and Moon focus' -Condition {Assert-Input;$world=Invoke-MoonRpc query world -DevelopmentPath $ExpectedDaemonPath;return [MoonDesktopNative]::GetForegroundWindow().ToInt64() -eq $Helper.id -and $world.focused -eq $Helper.id}
}
function Wait-Current($Helper,[bool]$Current){
 Wait-MoonCondition -Seconds 5 -Description "native current=$Current for owned helper" -Condition {Assert-Input;$native=Assert-Owned $Helper;return $native.Current -eq $Current}
}
function Observe-Desktop($Current,$Away,[string]$Name){
 Wait-MoonCondition -Seconds 8 -Description 'native-desktop suspension and visible placement' -Condition {
  Assert-Input;$null=Assert-Daemon;$here=Assert-Owned $Current;$awayNative=Assert-Owned $Away;$world=Invoke-MoonRpc query world -DevelopmentPath $ExpectedDaemonPath;$placements=@(Invoke-MoonRpc query windows -DevelopmentPath $ExpectedDaemonPath)
  $hereState=$world.windows.PSObject.Properties[[string]$Current.id];$awayState=$world.windows.PSObject.Properties[[string]$Away.id]
  $visible=@($placements|Where-Object{$_.id -eq $Current.id -and $_.visible})
  $exact=$visible.Count -eq 1
  if($exact){$rect=$visible[0].rect;$exact=[Math]::Abs($here.Frame.Left-$rect.x) -le 1 -and [Math]::Abs($here.Frame.Top-$rect.y) -le 1 -and [Math]::Abs(($here.Frame.Right-$here.Frame.Left)-$rect.width) -le 1 -and [Math]::Abs(($here.Frame.Bottom-$here.Frame.Top)-$rect.height) -le 1}
  return $here.Current -and !$awayNative.Current -and !$here.Cloak -and $awayNative.Cloak -ne 0 -and $hereState -and !$hereState.Value.suspended -and $awayState -and $awayState.Value.suspended -and @($placements|Where-Object{$_.id -eq $Away.id}).Count -eq 0 -and $exact
 }
 $outer=(Assert-Owned $Away).Outer|ConvertTo-Json -Compress;$samples=[Collections.Generic.List[object]]::new()
 for($i=0;$i -lt 8;$i++){
  Assert-Input;$null=Assert-Daemon;$native=Assert-Owned $Away;$here=Assert-Owned $Current;$world=Invoke-MoonRpc query world -DevelopmentPath $ExpectedDaemonPath
  if($native.Current -or !$native.Cloak -or !$here.Current -or (($native.Outer|ConvertTo-Json -Compress) -ne $outer) -or !$world.windows.PSObject.Properties[[string]$Away.id].Value.suspended){throw 'The inactive native-desktop helper changed or became active.'}
  $samples.Add([pscustomobject]@{sample=$i;away=$native;current=$here.Id;suspended=$true});Start-Sleep -Milliseconds 250
 }
 $report.observations.Add([pscustomobject]@{name=$Name;samples=$samples.ToArray()});$checks.Add([pscustomobject]@{check=$Name;passed=$true});Save-Report
}
function Return-ToOriginal {
 for($hop=0;$hop -le 16;$hop++){
  Assert-Input;$native=Assert-Owned $a;if($native.Current){return $hop}
  if($hop -eq 16){throw 'Original desktop not reached within sixteen left hops.'}
  Send-DesktopKey 37;Start-Sleep -Milliseconds 500
 }
}
function Go-ToCreated([int]$Hops){
 for($hop=0;$hop -lt $Hops;$hop++){Send-DesktopKey 39;Start-Sleep -Milliseconds 500}
 Wait-Current $b $true
 if((Assert-Owned $b).Desktop -ne $createdDesktop){throw 'The created desktop sentinel changed.'}
}
function Close-CreatedDesktop {
 Assert-Input;$null=Assert-Daemon;$native=Assert-Owned $b
 if(!$createdDesktop -or !$native.Current -or $native.Desktop -ne $createdDesktop -or (Assert-Owned $a).Current){throw 'The current desktop is not the identified created desktop.'}
 Assert-Window $shell
 $inventory=@(Get-Inventory);$foreign=@($inventory|Where-Object{!$_.unassigned -and $_.desktop -eq $createdDesktop -and $_.id -ne $b.id})
 $report.before_desktop_close=$inventory;Save-Report
 Assert-UnassignedBaseline $inventory
 if($foreign.Count){throw 'A foreign application appeared on the created desktop; the desktop was retained.'}
 # Recheck current identity immediately before the non-atomic Windows shortcut.
 $native=Assert-Owned $b;if(!$native.Current -or $native.Desktop -ne $createdDesktop){throw 'The current native desktop changed before close.'}
 Send-DesktopKey 115
 Wait-MoonCondition -Seconds 5 -Description 'the owned sentinel to leave the closed desktop' -Condition {Assert-Input;return (Assert-Owned $b).Desktop -ne $createdDesktop}
 $script:desktopClosed=$true;Save-Report
}
function Close-Helper($Helper){
 $Helper.process.Refresh();if($Helper.process.HasExited){return}
 if(!$Helper.id){
  $identity=[MoonInstallNative]::ProcessIdentity([uint32]$Helper.process.Id)
  if($identity.Birth -ne $Helper.birth -or $identity.Path -ine $TestwinPath){throw 'The HWND-less helper identity changed; it was not terminated.'}
  # This handle has been retained since Process.Start; never terminate by PID alone.
  if(![MoonDesktopNative]::TerminateProcess($Helper.process.Handle,1) -or !$Helper.process.WaitForExit(5000)){throw 'The exact owned HWND-less helper could not be terminated.'}
  $checks.Add([pscustomobject]@{check='HWND-less owned fixture terminated by retained handle';process_id=$Helper.process.Id;birth=$Helper.birth;passed=$true})
  return
 }
 $null=Assert-Owned $Helper
 if(![MoonDesktopNative]::PostMessage([IntPtr][long]$Helper.id,16,[IntPtr]::Zero,[IntPtr]::Zero)){throw 'Could not request normal closure of the owned helper.'}
 if(!$Helper.process.WaitForExit(5000)){throw 'The owned helper did not close; it was retained for recovery.'}
}
try{
 [MoonDesktopNative]::Initialize();[void][MoonDesktopNative]::SetThreadDpiAwarenessContext([IntPtr](-4));[MoonDesktopNative]::StartGuard()
 $shellWindow=[MoonDesktopNative]::GetShellWindow().ToInt64();$shellPid=[MoonDesktopNative]::Owner($shellWindow);$shellIdentity=[MoonInstallNative]::ProcessIdentity($shellPid)
 if($shellIdentity.Path -ine (Join-Path $env:SystemRoot 'explorer.exe')){throw 'The current desktop shell is not the expected Explorer image.'}
 $shell=[pscustomobject]@{id=$shellWindow;process_id=$shellPid;birth=$shellIdentity.Birth;path=$shellIdentity.Path}
 $inventory=@(Get-Inventory -Baseline);$baselineNative=@($inventory|Where-Object{!$_.unassigned});$baselineUnassigned=@($inventory|Where-Object unassigned)
 $report.baseline_native=$baselineNative;$report.baseline_unassigned=$baselineUnassigned
 foreach($entry in $baselineWorld.windows.PSObject.Properties){
  $window=@($baselineNative|Where-Object{$_.id -eq [long]$entry.Name -and $_.process_id -eq $entry.Value.info.process_id})
  if($window.Count -ne 1 -or $window[0].identity_source -ne 'native'){throw 'A managed window lacks an unambiguous native identity.'};$originalWindows+= $window[0]
 }
 $foreground=[MoonDesktopNative]::GetForegroundWindow().ToInt64();$previous=@($baselineNative|Where-Object{$_.id -eq $foreground})
 if($previous.Count -ne 1 -or !$previous[0].current -or $previous[0].identity_source -ne 'native'){throw 'The original foreground cannot be safely restored.'};$previous=$previous[0];$report.original_foreground=$previous
 Save-Report
 $a=New-Helper;Focus-Owned $a
 $report.original_desktop=$a.desktop;$creationAttempted=$true;Save-Report
 Send-DesktopKey 68 $a.id;Wait-Current $a $false
 $b=New-Helper
 if($b.desktop -eq $a.desktop -or @($baselineNative|Where-Object{$_.desktop -eq $b.desktop}).Count){throw 'The resulting desktop is not a distinct, previously unoccupied desktop identity.'}
 $createdDesktop=$b.desktop
 Save-Report;Observe-Desktop $b $a 'original helper suspended on the new native desktop'
 $hops=Return-ToOriginal;$report.return_hops=$hops;Wait-Current $b $false
 Observe-Desktop $a $b 'new helper suspended after returning to the original desktop'
 Go-ToCreated $hops;Observe-Desktop $b $a 'second visit preserves both memberships'
 $null=Return-ToOriginal;Focus-Owned $a
 $beforeHooks=(Assert-Daemon).hooks.callbacks
 $initialFloating=(Invoke-MoonRpc query world -DevelopmentPath $ExpectedDaemonPath).windows.PSObject.Properties[[string]$a.id].Value.floating
 foreach($expected in @((-not $initialFloating),[bool]$initialFloating)){
  Focus-Owned $a;Assert-Input;$script:lastInput=[MoonDesktopNative]::Chord($hookKeys,$script:lastInput,$a.id)
  Wait-MoonCondition -Seconds 5 -Description 'owned reversible hook command' -Condition {Assert-Input;return (Invoke-MoonRpc query world -DevelopmentPath $ExpectedDaemonPath).windows.PSObject.Properties[[string]$a.id].Value.floating -eq $expected}
 }
 if((Assert-Daemon).hooks.callbacks -le $beforeHooks){throw 'No live keyboard-hook callback was recorded.'}
 $checks.Add([pscustomobject]@{check='configured hotkey works after native desktop return';passed=$true});Save-Report
 Go-ToCreated $hops;Close-CreatedDesktop;$null=Return-ToOriginal
 $checks.Add([pscustomobject]@{check='only the identified empty owned desktop closed';passed=$true})
}catch{
 $failure=$_.Exception.Message;$report.failure=$failure
 try{$report.failure_world=Invoke-MoonRpc query world -DevelopmentPath $ExpectedDaemonPath;$report.failure_stats=Invoke-MoonRpc query stats -DevelopmentPath $ExpectedDaemonPath}catch{$report.failure_query_error=$_.Exception.Message}
}
finally{
 # Never guess a desktop identity after a failed creation or interrupted switch.
 if(!$intervened -and $a -and $b -and $createdDesktop -and !$desktopClosed){
  try{Assert-Input;if(!(Assert-Owned $b).Current){$null=Return-ToOriginal;if(!$report.Contains('return_hops')){throw 'No complete navigation route was recorded; created desktop retained.'};Go-ToCreated $report.return_hops};Close-CreatedDesktop}catch{$cleanupErrors.Add($_.Exception.Message)}
 }
 if(!$intervened -and $a){try{$null=Return-ToOriginal}catch{$cleanupErrors.Add($_.Exception.Message)}}
 foreach($helper in $owned){try{Close-Helper $helper}catch{$cleanupErrors.Add($_.Exception.Message)}}
 if(!$intervened -and $previous){
  try{
   Assert-Input;$null=Assert-Daemon;foreach($window in $originalWindows){Assert-Window $window}
   $focus=@($originalWindows|Where-Object{$_.id -eq $baselineWorld.focused})
   if($focus.Count -eq 1 -and ([MoonDesktopNative]::Read($focus[0].id)).Current){[void][MoonDesktopNative]::SetForegroundWindow([IntPtr][long]$focus[0].id);Start-Sleep -Milliseconds 200}
   Assert-Window $previous;if(!([MoonDesktopNative]::Read($previous.id)).Current){throw 'Original foreground is not on the current desktop; it was not activated.'}
   if(![MoonDesktopNative]::SetForegroundWindow([IntPtr][long]$previous.id)){throw 'Windows refused original foreground restoration.'}
   Wait-MoonCondition -Seconds 8 -Description 'the exact original semantic layout and foreground' -Condition {Assert-Input;return (Get-MoonLayoutFingerprint -DevelopmentPath $ExpectedDaemonPath) -eq $baselineFingerprint -and [MoonDesktopNative]::GetForegroundWindow().ToInt64() -eq $previous.id}
   $finalNative=@(Get-Inventory)
   foreach($window in $baselineNative){
    Assert-Window $window
    $after=@($finalNative|Where-Object{$_.id -eq $window.id -and $_.class -ceq $window.class})
    if($after.Count -ne 1 -or $after[0].desktop -ne $window.desktop -or $after[0].current -ne $window.current -or $after[0].cloak -ne $window.cloak -or (($after[0].outer|ConvertTo-Json -Compress) -ne ($window.outer|ConvertTo-Json -Compress))){throw 'An original native window state differs after cleanup.'}
   }
   $report.restored_native=$finalNative;$report.restored_fingerprint=Get-MoonLayoutFingerprint -DevelopmentPath $ExpectedDaemonPath;$report.final_stats=Assert-Daemon
  }catch{$cleanupErrors.Add($_.Exception.Message)}
 }
 $sameConfig=$false
 try{
  $sameConfig=(Test-Path -LiteralPath $baselineStats.config_path) -eq $configExists
  if($sameConfig -and $configExists){$sameConfig=(Get-FileHash -LiteralPath $baselineStats.config_path -Algorithm SHA256).Hash -eq $configHash}
 }catch{$sameConfig=$false;$cleanupErrors.Add('Configuration verification failed: '+$_.Exception.Message)}
 $report.configuration_unchanged=$sameConfig;if(!$sameConfig){$cleanupErrors.Add('Personal configuration changed during the scenario; it was not overwritten.')}
 try{[MoonDesktopNative]::StopGuard()}catch{$cleanupErrors.Add($_.Exception.Message)}
 $report.input_guard=[pscustomobject]@{foreign_events=[MoonDesktopNative]::ForeignEvents();owned_events=[MoonDesktopNative]::OwnEvents();raw_input_recorded=$false}
 $report.remaining_owned=@($owned|Where-Object{!$_.process.HasExited}|ForEach-Object{[pscustomobject]@{process_id=$_.process.Id;birth=$_.birth;id=$_.id;desktop=$_.desktop}})
 $report.recovery_required=($creationAttempted -and !$desktopClosed) -or $report.remaining_owned.Count -gt 0 -or $intervened -or $cleanupErrors.Count -gt 0
 $report.passed=!$failure -and !$report.recovery_required;$report.finished_at=[DateTime]::UtcNow.ToString('o')
 try{Save-Report}finally{foreach($helper in $owned){$helper.process.Dispose()};[MoonDesktopNative]::Release()}
}
if(!$report.passed){throw "Native desktop validation did not pass; preserve the recovery report at $ReportPath. $failure"}
[pscustomobject]@{passed=$true;checks=$checks.Count;report=$ReportPath;configuration_unchanged=$true;owned_processes=0}|ConvertTo-Json
