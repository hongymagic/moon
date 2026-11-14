# Shared local install, uninstall and S8 verification helpers. No action runs on import.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (!('MoonInstallNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using Microsoft.Win32;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
public static class MoonInstallNative {
    [StructLayout(LayoutKind.Sequential)] struct Label { public IntPtr Sid; public uint Attributes; }
    [StructLayout(LayoutKind.Sequential)] struct LogonGroups { public uint Count; public Label Group; }
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr token,int kind,IntPtr value,uint length,out uint returned);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr process,uint access,out IntPtr token);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool SetTokenInformation(IntPtr token,int kind,ref Label value,uint length);
    [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool ConvertStringSidToSid(string value,out IntPtr sid);
    [DllImport("advapi32.dll")] static extern IntPtr GetSidSubAuthorityCount(IntPtr sid);
    [DllImport("advapi32.dll")] static extern IntPtr GetSidSubAuthority(IntPtr sid,uint index);
    [DllImport("advapi32.dll")] static extern uint GetLengthSid(IntPtr sid);
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,uint processId);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool QueryFullProcessImageName(IntPtr process,uint flags,StringBuilder path,ref uint length);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetProcessTimes(IntPtr process,out long created,out long exited,out long kernel,out long user);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetNamedPipeServerProcessId(Microsoft.Win32.SafeHandles.SafePipeHandle pipe,out uint processId);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetNamedPipeInfo(Microsoft.Win32.SafeHandles.SafePipeHandle pipe,IntPtr flags,IntPtr outBuffer,IntPtr inBuffer,out uint maximumInstances);
    [DllImport("kernel32.dll",SetLastError=true)] static extern uint WaitForSingleObject(IntPtr handle,uint milliseconds);
    [DllImport("shell32.dll")] static extern int SHGetKnownFolderPath(ref Guid folder,uint flags,IntPtr token,out IntPtr path);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr memory);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr LoadLibraryEx(string path,IntPtr file,uint flags);
    [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr FindResource(IntPtr module,IntPtr name,IntPtr type);
    [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr LoadResource(IntPtr module,IntPtr resource);
    [DllImport("kernel32.dll")] static extern IntPtr LockResource(IntPtr resource);
    [DllImport("kernel32.dll")] static extern uint SizeofResource(IntPtr module,IntPtr resource);
    [DllImport("kernel32.dll")] static extern bool FreeLibrary(IntPtr module);
    [DllImport("user32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr SendMessageTimeout(IntPtr window,uint message,IntPtr wparam,string lparam,uint flags,uint timeout,out UIntPtr result);
    static Exception Error() { return new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error()); }
    public static string ProgramFilesX64() {
        if(!Environment.Is64BitProcess) {
            using(var machine=RegistryKey.OpenBaseKey(RegistryHive.LocalMachine,RegistryView.Registry64))
            using(var windows=machine.OpenSubKey(@"SOFTWARE\Microsoft\Windows\CurrentVersion")) {
                var directory=windows==null ? null : windows.GetValue("ProgramFilesDir") as string;
                if(String.IsNullOrEmpty(directory))throw new InvalidOperationException("Windows did not provide the 64-bit Program Files directory.");
                return directory;
            }
        }
        var folder=new Guid("6d809377-6af0-444b-8957-a3773f02200e");
        IntPtr path;Marshal.ThrowExceptionForHR(SHGetKnownFolderPath(ref folder,0,IntPtr.Zero,out path));
        try{return Marshal.PtrToStringUni(path);}finally{Marshal.FreeCoTaskMem(path);}
    }
    public sealed class ProcessIdentityInfo {public string Path; public long Birth; public uint ProcessId; public bool UiAccess;}
    static long ProcessBirth(IntPtr process) {
        long created,exited,kernel,user;
        if(!GetProcessTimes(process,out created,out exited,out kernel,out user))throw Error();
        return created;
    }
    static ProcessIdentityInfo ReadProcessIdentity(IntPtr process,uint processId) {
        var path=new StringBuilder(32768);uint length=(uint)path.Capacity;
        if(!QueryFullProcessImageName(process,0,path,ref length))throw Error();
        return new ProcessIdentityInfo{Path=path.ToString(),Birth=ProcessBirth(process),ProcessId=processId};
    }
    public static ProcessIdentityInfo ProcessIdentity(uint processId) {
        var process=OpenProcess(0x1000,false,processId);if(process==IntPtr.Zero)throw Error();
        try{return ReadProcessIdentity(process,processId);}finally{CloseHandle(process);}
    }
    public static bool ProcessStillRunning(ProcessIdentityInfo identity) {
        var process=OpenProcess(0x101000,false,identity.ProcessId);
        if(process==IntPtr.Zero) {
            var error=Marshal.GetLastWin32Error();
            if(error==87)return false;
            throw new System.ComponentModel.Win32Exception(error);
        }
        try {
            if(ProcessBirth(process)!=identity.Birth)return false;
            var state=WaitForSingleObject(process,0);
            if(state==0)return false;
            if(state==258)return true;
            throw Error();
        } finally {CloseHandle(process);}
    }
    public static ProcessIdentityInfo VerifyPipeServer(Microsoft.Win32.SafeHandles.SafePipeHandle pipe,string expectedPath,ProcessIdentityInfo previous,string developmentPath,string installedPath,bool exactDevelopmentPath) {
        return VerifyPipeServer(pipe,expectedPath,previous,developmentPath,installedPath,exactDevelopmentPath,null,null);
    }
    public static ProcessIdentityInfo VerifyPipeServer(Microsoft.Win32.SafeHandles.SafePipeHandle pipe,string expectedPath,ProcessIdentityInfo previous,string developmentPath,string installedPath,bool exactDevelopmentPath,ProcessIdentityInfo portable,string portableSha256) {
        uint maximum;
        if(!GetNamedPipeInfo(pipe,IntPtr.Zero,IntPtr.Zero,IntPtr.Zero,out maximum))throw Error();
        if(maximum!=1)throw new InvalidOperationException("Moon IPC requires one server instance per pipe name.");
        uint pid;if(!GetNamedPipeServerProcessId(pipe,out pid))throw Error();
        var process=OpenProcess(0x101000,false,pid);if(process==IntPtr.Zero)throw Error();
        try {
            var peer=ReadProcessIdentity(process,pid);
            IntPtr token;if(!OpenProcessToken(process,8,out token))throw Error();
            try {
                using(var current=WindowsIdentity.GetCurrent()) {
                    if(PipeName(token)!=PipeName(current.Token) || TokenWord(token,12)!=TokenWord(current.Token,12) || TokenWord(token,29)!=0)
                        throw new InvalidOperationException("The Moon pipe server belongs to a different Windows logon.");
                    peer.UiAccess=TokenWord(token,26)!=0;
                    if(portable!=null) {
                        if(previous!=null || !String.IsNullOrEmpty(expectedPath) || exactDevelopmentPath || portable.UiAccess
                            || peer.UiAccess || TokenWord(token,20)!=0 || TokenIntegrity(token)!=8192
                            || portable.ProcessId!=pid || portable.Birth!=peer.Birth
                            || !String.Equals(portable.Path,peer.Path,StringComparison.OrdinalIgnoreCase)
                            || portableSha256==null || !System.Text.RegularExpressions.Regex.IsMatch(portableSha256,@"\A[0-9a-fA-F]{64}\z"))
                            throw new InvalidOperationException("The portable Moon process no longer matches its approved identity.");
                        using(var image=System.IO.File.Open(peer.Path,System.IO.FileMode.Open,System.IO.FileAccess.Read,System.IO.FileShare.Read|System.IO.FileShare.Delete))
                        using(var sha=System.Security.Cryptography.SHA256.Create()) {
                            var actual=BitConverter.ToString(sha.ComputeHash(image)).Replace("-","");
                            if(!String.Equals(actual,portableSha256,StringComparison.OrdinalIgnoreCase))
                                throw new InvalidOperationException("The portable Moon executable changed after it was selected.");
                        }
                    } else if(!String.IsNullOrEmpty(expectedPath)) {
                        var matchesPrevious=previous!=null && previous.ProcessId==pid && previous.Birth==peer.Birth;
                        if(!peer.UiAccess || (previous!=null ? !matchesPrevious : !String.Equals(peer.Path,expectedPath,StringComparison.OrdinalIgnoreCase)))
                            throw new InvalidOperationException("The Moon pipe server is not the expected installed daemon.");
                    } else {
                        var installed=String.Equals(peer.Path,installedPath,StringComparison.OrdinalIgnoreCase);
                        var development=String.Equals(peer.Path,developmentPath,StringComparison.OrdinalIgnoreCase);
                        if((exactDevelopmentPath && !development) || (installed ? !peer.UiAccess : !development
                            || (TokenWord(current.Token,20)!=0 && TokenWord(token,20)==0)))
                            throw new InvalidOperationException("The Moon pipe server is not a daemon at the expected privilege level.");
                    }
                }
                if(WaitForSingleObject(process,0)!=258)throw new InvalidOperationException("The Moon pipe server exited during verification.");
                return peer;
            } finally {CloseHandle(token);}
        } finally {CloseHandle(process);}
    }
    public static string PipeName() { using(var identity=WindowsIdentity.GetCurrent()) { return PipeName(identity.Token); } }
    public static string PipeName(IntPtr token) {
        uint length;GetTokenInformation(token,28,IntPtr.Zero,0,out length);
        if(length<Marshal.SizeOf(typeof(LogonGroups)) || length>65536)throw new InvalidOperationException("Invalid Windows logon identity size");
        var memory=Marshal.AllocHGlobal((int)length);
        try {
            if(!GetTokenInformation(token,28,memory,length,out length))throw Error();
            var groups=(LogonGroups)Marshal.PtrToStructure(memory,typeof(LogonGroups));
            if(groups.Count!=1 || groups.Group.Sid==IntPtr.Zero)throw new InvalidOperationException("Expected one Windows logon SID");
            return "moon-v1-"+new SecurityIdentifier(groups.Group.Sid).Value;
        } finally {Marshal.FreeHGlobal(memory);}
    }
    static uint TokenWord(IntPtr token,int kind) {var memory=Marshal.AllocHGlobal(4); try {uint length; if(!GetTokenInformation(token,kind,memory,4,out length)) throw Error(); return unchecked((uint)Marshal.ReadInt32(memory));} finally {Marshal.FreeHGlobal(memory);} }
    public static uint TokenWord(int kind) { using(var identity=WindowsIdentity.GetCurrent()) {return TokenWord(identity.Token,kind);} }
    static uint TokenIntegrity(IntPtr token) {uint length;GetTokenInformation(token,25,IntPtr.Zero,0,out length);var memory=Marshal.AllocHGlobal((int)length);try {if(!GetTokenInformation(token,25,memory,length,out length))throw Error();var sid=Marshal.ReadIntPtr(memory);var count=Marshal.ReadByte(GetSidSubAuthorityCount(sid));return unchecked((uint)Marshal.ReadInt32(GetSidSubAuthority(sid,(uint)(count-1))));}finally{Marshal.FreeHGlobal(memory);}}
    public static uint Integrity() {using(var identity=WindowsIdentity.GetCurrent()){return TokenIntegrity(identity.Token);}}
    public static void LowerToLowIntegrity() {IntPtr token,sid;if(!OpenProcessToken(GetCurrentProcess(),0x88,out token))throw Error();try{if(!ConvertStringSidToSid("S-1-16-4096",out sid))throw Error();try{var label=new Label{Sid=sid,Attributes=0x20};if(!SetTokenInformation(token,25,ref label,(uint)Marshal.SizeOf(typeof(Label))+GetLengthSid(sid)))throw Error();}finally{LocalFree(sid);}}finally{CloseHandle(token);}}
    public static string Manifest(string path) {var module=LoadLibraryEx(path,IntPtr.Zero,0x22);if(module==IntPtr.Zero)throw Error();try{var resource=FindResource(module,new IntPtr(1),new IntPtr(24));if(resource==IntPtr.Zero)throw Error();var loaded=LoadResource(module,resource);if(loaded==IntPtr.Zero)throw Error();uint length=SizeofResource(module,resource);if(length==0||length>1048576)throw new InvalidOperationException("Invalid manifest size");var bytes=new byte[length];Marshal.Copy(LockResource(loaded),bytes,0,(int)length);return Encoding.UTF8.GetString(bytes).TrimEnd('\0');}finally{FreeLibrary(module);}}
    public static void EnvironmentChanged() {UIntPtr result;SendMessageTimeout(new IntPtr(0xffff),0x1a,IntPtr.Zero,"Environment",2,200,out result);}
}
'@
}

function Get-MoonInstallDirectory { Join-Path ([MoonInstallNative]::ProgramFilesX64()) 'moon' }
function Get-MoonStartupLinks {
    param([string[]]$Programs,[string]$Directory=([Environment]::GetFolderPath('Startup')))
    if (!(Test-Path -LiteralPath $Directory)) {return}
    $shell=New-Object -ComObject WScript.Shell
    try {
        foreach ($file in @(Get-ChildItem -LiteralPath $Directory -Filter '*.lnk')) {
            $hash=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
            $shortcut=$shell.CreateShortcut($file.FullName)
            try {
                if (!$shortcut.TargetPath) {continue}
                $target=[Environment]::ExpandEnvironmentVariables($shortcut.TargetPath)
                $program=[IO.Path]::GetFileName($target).ToLowerInvariant()
                if ($Programs -notcontains $program) {continue}
                $target=[IO.Path]::GetFullPath($target)
                if ((Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash -ne $hash) {throw 'A startup shortcut changed while its launch information was read.'}
                [pscustomobject]@{original=$file.FullName;program=$program;target=$target;arguments=[string]$shortcut.Arguments;working_directory=[Environment]::ExpandEnvironmentVariables([string]$shortcut.WorkingDirectory);sha256=$hash}
            } finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shortcut)}
        }
    } finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)}
}
function Get-MoonProcessLaunch {
    param($Process,[object[]]$StartupLinks=@())
    # Limited-query access remains available when WMI withholds UIAccess paths.
    # Capture path and creation time from one process handle to bind the PID.
    $identity=[MoonInstallNative]::ProcessIdentity([uint32]$Process.ProcessId)
    $path=[IO.Path]::GetFullPath($identity.Path)
    if ([IO.Path]::GetFileName($path) -ine $Process.Name -or ($Process.ExecutablePath -and ![string]::Equals([IO.Path]::GetFullPath($Process.ExecutablePath),$path,[StringComparison]::OrdinalIgnoreCase))) {throw 'The previous desktop process identity changed during handover preflight.'}
    $source='process_command_line'; $shortcutPath=$null
    $workingDirectory=[IO.Path]::GetDirectoryName($path)
    if ($Process.CommandLine) {
        if ($Process.CommandLine -notmatch '^(?:"[^"]*"|\S+)\s*(.*)$') {throw "Cannot safely preserve launch arguments for $($Process.Name)."}
        $arguments=$Matches[1]
    } else {
        $matched=@($StartupLinks | Where-Object {[string]::Equals($_.target,$path,[StringComparison]::OrdinalIgnoreCase)})
        if ($matched.Count -ne 1) {throw "Cannot safely preserve launch arguments for $($Process.Name): expected one startup shortcut with the exact running image path."}
        $shortcut=$matched[0]
        if ((Get-FileHash -LiteralPath $shortcut.original -Algorithm SHA256).Hash -ne $shortcut.sha256) {throw 'The matching startup shortcut changed after preflight.'}
        $source='startup_shortcut'; $shortcutPath=$shortcut.original
        $arguments=$shortcut.arguments
        if ($shortcut.working_directory) {$workingDirectory=$shortcut.working_directory}
    }
    [pscustomobject]@{name=$Process.Name.ToLowerInvariant();path=$path;arguments=$arguments;working_directory=$workingDirectory;process_id=$Process.ProcessId;process_birth=$identity.Birth;launch_source=$source;startup_shortcut=$shortcutPath}
}
function Assert-MoonProcessIdentity($Expected) {
    $identity=[MoonInstallNative]::ProcessIdentity([uint32]$Expected.process_id)
    if ($identity.Birth -ne $Expected.process_birth -or ![string]::Equals($identity.Path,$Expected.path,[StringComparison]::OrdinalIgnoreCase)) {throw 'A captured desktop process changed identity; it was not stopped.'}
}
function Read-MoonPathValue([Microsoft.Win32.RegistryKey]$Key) {
    $names=@()
    if ($Key) {$names=@($Key.GetValueNames() | Where-Object {$_ -ieq 'Path'} | Select-Object -First 1)}
    if ($names.Count -eq 0) {return [pscustomobject]@{exists=$false;name='Path';value='';kind='ExpandString'}}
    $name=$names[0]
    $kind=$Key.GetValueKind($name).ToString()
    if ($kind -notin @('String','ExpandString')) {throw 'The existing user PATH has an unsupported registry value kind; it was not changed.'}
    [pscustomobject]@{exists=$true;name=$name;value=[string]$Key.GetValue($name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames);kind=$kind}
}
function Get-MoonPathAddition($Current,[string]$Entry,$Ownership) {
    $expected=$Entry.TrimEnd('\','/')
    foreach ($part in $Current.value.Split(';')) {
        $candidate=$part.Trim().Trim('"')
        if ($Current.kind -eq 'ExpandString') {$candidate=[Environment]::ExpandEnvironmentVariables($candidate)}
        if ([string]::Equals($candidate.TrimEnd('\','/'),$expected,[StringComparison]::OrdinalIgnoreCase)) {
            if (!$Ownership) {$Ownership=[pscustomobject]@{owned=$false;entry=$Entry}}
            return [pscustomobject]@{changed=$false;before=$Current;after=$Current;ownership=$Ownership}
        }
    }
    $separator=$Current.value.Length -gt 0 -and !$Current.value.EndsWith(';')
    $prefix=$Current.value+$(if($separator){';'}else{''})
    $value=$prefix+$Entry
    if ($value.Length -gt 32766) {throw 'Adding moon would exceed the Windows environment-variable limit; PATH was not changed.'}
    $after=[pscustomobject]@{exists=$true;name=$Current.name;value=$value;kind=$Current.kind}
    $owned=[pscustomobject]@{owned=$true;entry=$Entry;separator_added=$separator;value_existed=$Current.exists;kind=$Current.kind}
    [pscustomobject]@{changed=$true;before=$Current;after=$after;ownership=$owned}
}
function Get-MoonPathRemoval($Current,$Ownership) {
    if (!$Ownership -or !$Ownership.owned -or !$Current.exists) {return [pscustomobject]@{changed=$false;before=$Current;after=$Current}}
    $offset=0; $start=-1
    foreach($part in $Current.value.Split(';')) {
        # Remove one exact installed token. A reformatted or expanded user edit
        # is no longer our literal entry and is preserved.
        if ([string]::Equals($part,$Ownership.entry,[StringComparison]::Ordinal)) {$start=$offset;break}
        $offset+=$part.Length+1
    }
    if ($start -lt 0) {return [pscustomobject]@{changed=$false;before=$Current;after=$Current}}
    $length=$Ownership.entry.Length; $end=$start+$length
    if ($end -lt $Current.value.Length -and $Current.value[$end] -eq ';') {$length++}
    elseif ($start -gt 0 -and $Ownership.separator_added) {$start--; $length++}
    $value=$Current.value.Remove($start,$length)
    $exists=!(!$value -and !$Ownership.value_existed -and $Current.kind -eq $Ownership.kind)
    $after=[pscustomobject]@{exists=$exists;name=$Current.name;value=$value;kind=$Current.kind}
    [pscustomobject]@{changed=$true;before=$Current;after=$after}
}
function Set-MoonPathChange([Microsoft.Win32.RegistryKey]$Key,$Change) {
    if (!$Change.changed) {return}
    $current=Read-MoonPathValue $Key
    if ($current.exists -ne $Change.before.exists -or $current.kind -ne $Change.before.kind -or ![string]::Equals($current.value,$Change.before.value,[StringComparison]::Ordinal)) {
        throw 'User PATH changed while the installer was preparing its update; the newer value was not overwritten.'
    }
    if ($Change.after.exists) {$Key.SetValue($Change.after.name,$Change.after.value,[Microsoft.Win32.RegistryValueKind]$Change.after.kind)}
    else {$Key.DeleteValue($current.name,$false)}
}
function Remove-MoonOwnedUserPath($Ownership) {
    if (!$Ownership -or !$Ownership.owned) {return}
    if (![string]::Equals($Ownership.entry,(Get-MoonInstallDirectory),[StringComparison]::Ordinal)) {throw 'The recorded PATH entry is not this installation directory.'}
    $key=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment',$true)
    if (!$key) {return}
    try {
        $change=Get-MoonPathRemoval (Read-MoonPathValue $key) $Ownership
        Set-MoonPathChange $key $change
        if ($change.changed) {[MoonInstallNative]::EnvironmentChanged()}
    } finally {$key.Dispose()}
}
function Test-MoonAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try { ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) }
    finally { $identity.Dispose() }
}
function New-MoonInstallMutexSecurity {
    $security=[Security.AccessControl.MutexSecurity]::new()
    $security.SetAccessRuleProtection($true,$false)
    $security.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
    foreach ($sid in @('S-1-5-32-544','S-1-5-18')) {
        $rule=[Security.AccessControl.MutexAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid),[Security.AccessControl.MutexRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow)
        [void]$security.AddAccessRule($rule)
    }
    $security
}
function Enter-MoonInstallLock {
    param([string]$Name='Global\moon-install-v1',[Security.AccessControl.MutexSecurity]$Security=(New-MoonInstallMutexSecurity))
    # One installation is shared by every account and Windows session. Acquire
    # only after elevation, before reading state; keep it through recovery.
    $created=$false
    $mutex=[Threading.Mutex]::new($false,$Name,[ref]$created,$Security)
    try {
        try {$acquired=$mutex.WaitOne(0)} catch [Threading.AbandonedMutexException] {$acquired=$true}
        if (!$acquired) {throw 'Another Moon setup or removal is already running. Wait for it to finish, then try again.'}
        return $mutex
    } catch {$mutex.Dispose();throw}
}
function Exit-MoonInstallLock([Threading.Mutex]$Mutex) {
    if ($null -ne $Mutex) {try {$Mutex.ReleaseMutex()} finally {$Mutex.Dispose()}}
}
function New-MoonHandoverMutexSecurity {
    $security=New-MoonInstallMutexSecurity
    $user=[Security.Principal.WindowsIdentity]::GetCurrent().User
    $security.SetOwner($user)
    [void]$security.AddAccessRule([Security.AccessControl.MutexAccessRule]::new($user,[Security.AccessControl.MutexRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow))
    $security
}
function Test-MoonHandoverInstallation($Saved,$State) {
    if (!$State -or !$Saved.PSObject.Properties['handover_id'] -or $Saved.handover_id -notmatch '^[a-f0-9]{32}$' -or !$State.PSObject.Properties['handover_id'] -or $State.handover_id -cne $Saved.handover_id) {return $false}
    if ($Saved.PSObject.Properties['installation_id'] -and $Saved.installation_id -and $State.install_id -cne $Saved.installation_id) {return $false}
    return $true
}
function Assert-MoonExpectedInstallation($State,[string]$InstallId,[string]$HandoverId) {
    if ($InstallId -and $State.install_id -cne $InstallId) {throw 'The installation changed before removal. The newer installation was left unchanged.'}
    if ($HandoverId -and (!$State.PSObject.Properties['handover_id'] -or $State.handover_id -cne $HandoverId)) {throw 'A different setup operation owns this installation. It was left unchanged.'}
}
function ConvertTo-MoonArgument([string]$Value) {
    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') { return $Value }
    $result = [Text.StringBuilder]::new(); [void]$result.Append('"'); $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') { $slashes++; continue }
        if ($character -eq '"') { [void]$result.Append(('\' * (2*$slashes+1))); [void]$result.Append('"') }
        else { [void]$result.Append(('\' * $slashes)); [void]$result.Append($character) }
        $slashes = 0
    }
    [void]$result.Append(('\' * (2*$slashes))); [void]$result.Append('"'); $result.ToString()
}
function Invoke-MoonNative {
    param([string]$Path,[string[]]$Arguments=@(),[int]$TimeoutSeconds=15,[switch]$AllowFailure)
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Path
    $start.Arguments = (@($Arguments | ForEach-Object { ConvertTo-MoonArgument $_ }) -join ' ')
    $start.UseShellExecute = $false; $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new(); $process.StartInfo = $start
    try {
        [void]$process.Start()
        $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
        if ($TimeoutSeconds -le 0) { $process.WaitForExit() }
        elseif (!$process.WaitForExit($TimeoutSeconds * 1000)) { $process.Kill(); throw "Process timed out: $Path" }
        $result = [pscustomobject]@{ ExitCode=$process.ExitCode; Output=$stdout.Result; Error=$stderr.Result }
        if (!$AllowFailure -and $result.ExitCode -ne 0) { throw "$Path failed ($($result.ExitCode)): $($result.Error)" }
        return $result
    } finally { $process.Dispose() }
}
function Invoke-MoonShellProcess {
    param([string]$Path,[string[]]$Arguments=@(),[switch]$Elevate,[ValidateRange(0,2147483)][int]$TimeoutSeconds=0)
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=$Path
    $start.Arguments=(@($Arguments | ForEach-Object {ConvertTo-MoonArgument $_}) -join ' ')
    $start.UseShellExecute=$true
    $start.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden
    if ($Elevate) {$start.Verb='runas'}
    $process=[Diagnostics.Process]::new();$process.StartInfo=$start
    try {
        # Start-Process discards the native exception when ShellExecute fails.
        # Keep it intact so a declined Windows prompt retains error 1223.
        if (!$process.Start()) {throw 'Windows did not return a setup process.'}
        # Installer/remover approval and recovery waits remain unlimited by
        # default. Only the short, already-approved config validator opts in.
        if ($TimeoutSeconds -eq 0) {$process.WaitForExit()}
        elseif (!$process.WaitForExit($TimeoutSeconds * 1000)) {
            if (!$process.HasExited) {
                try {$process.Kill()} catch {if (!$process.HasExited) {throw}}
            }
            if (!$process.WaitForExit(5000)) {throw "Process did not exit after its timeout: $Path"}
            throw "Process timed out: $Path"
        }
        return $process.ExitCode
    } finally {$process.Dispose()}
}
function Connect-MoonRpc([string]$Name,[string]$Expected,[MoonInstallNative+ProcessIdentityInfo]$ExpectedPeer,[string]$Development,[string]$InstalledPath,[bool]$ExactDevelopment,[MoonInstallNative+ProcessIdentityInfo]$ExpectedPortablePeer,[string]$ExpectedPortableSha256) {
    $until=[DateTime]::UtcNow.AddMilliseconds(1500)
    $lastError=$null
    do {
        for ($slot=0; $slot -lt 16; $slot++) {
            $endpoint=if($slot){$Name+'-'+$slot}else{$Name}
            $pipe=[IO.Pipes.NamedPipeClientStream]::new('.',$endpoint,[IO.Pipes.PipeDirection]::InOut,[IO.Pipes.PipeOptions]::Asynchronous,[Security.Principal.TokenImpersonationLevel]::Identification)
            try {
                $pipe.Connect(25)
                $peer=[MoonInstallNative]::VerifyPipeServer($pipe.SafePipeHandle,$Expected,$ExpectedPeer,$Development,$InstalledPath,$ExactDevelopment,$ExpectedPortablePeer,$ExpectedPortableSha256)
                return [pscustomobject]@{pipe=$pipe;peer=$peer}
            } catch {
                $pipe.Dispose()
                if ($_.Exception -isnot [TimeoutException]) {$lastError=$_}
            }
        }
    } while ([DateTime]::UtcNow -lt $until)
    if ($lastError) {throw $lastError}
    throw 'Moon IPC connection timed out.'
}
function Invoke-MoonRpc {
    param([ValidateSet('query','command','lifecycle')][string]$Method,[string]$Query,[string[]]$Commands,[long]$ExpectedFocus,
        [switch]$Installed,[MoonInstallNative+ProcessIdentityInfo]$ExpectedPeer,[string]$DevelopmentPath,[ValidateSet('restart','exit')][string]$LifecycleAction,
        [MoonInstallNative+ProcessIdentityInfo]$ExpectedPortablePeer,[string]$ExpectedPortableSha256)
    if ($ExpectedPortablePeer -or $ExpectedPortableSha256) {
        if (!$ExpectedPortablePeer -or $ExpectedPortableSha256 -notmatch '^[A-Fa-f0-9]{64}$' -or $Installed -or $ExpectedPeer -or $DevelopmentPath) {throw 'Portable peer verification requires one explicit, complete identity.'}
        if (!(($Method -eq 'query' -and $Query -in @('stats','world','windows','restart-config')) -or ($Method -eq 'lifecycle' -and $LifecycleAction -eq 'exit'))) {throw 'Portable migration only permits reading its checkpoint and a normal exit.'}
    }
    $installedPath=Join-Path (Get-MoonInstallDirectory) 'moon.exe'
    $expected=if($Installed -or $ExpectedPeer){$installedPath}else{$null}
    $development=if($DevelopmentPath){[IO.Path]::GetFullPath($DevelopmentPath)}else{Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'moon-dev\moon.exe'}
    $connection=Connect-MoonRpc ([MoonInstallNative]::PipeName()) $expected $ExpectedPeer $development $installedPath ([bool]$DevelopmentPath) $ExpectedPortablePeer $ExpectedPortableSha256
    $pipe=$connection.pipe
    $peer=$connection.peer
    try {
        $encoding = [Text.UTF8Encoding]::new($false,$true)
        $reader = [IO.StreamReader]::new($pipe,$encoding,$false,4096,$true)
        $writer = [IO.StreamWriter]::new($pipe,$encoding,4096,$true)
        try {
            $request = @{version=1;id='installation';method=$Method}
            switch ($Method) {
                'query' {$request.query=$Query}
                'command' {$request.commands=$Commands}
                'lifecycle' {$request.action=$LifecycleAction}
            }
            if ($PSBoundParameters.ContainsKey('ExpectedFocus')) {
                if ($Method -ne 'command') {throw 'ExpectedFocus applies only to command requests.'}
                $request.expected_focus=$ExpectedFocus
            }
            $writer.AutoFlush=$true; $writer.NewLine="`n"; $writer.WriteLine(($request | ConvertTo-Json -Compress))
            $read=$reader.ReadLineAsync()
            if (!$read.Wait(10000)) { throw 'moon IPC response timed out' }
            if ($null -eq $read.Result -or $read.Result.Length -gt 8388608) { throw 'Invalid moon IPC response' }
            $response=$read.Result | ConvertFrom-Json
            if (!$response.ok) { throw "moon IPC: $($response.error)" }
            if ($Method -eq 'query' -and $Query -eq 'stats') {
                if ($response.result.process_id -ne $peer.ProcessId -or [bool]$response.result.token.ui_access -ne $peer.UiAccess) {throw 'Moon IPC reported the wrong process identity.'}
                $response.result.executable=$peer.Path
                $response.result | Add-Member -NotePropertyName pipe_peer -NotePropertyValue $peer
            }
            return $response.result
        } finally { $reader.Dispose(); try { $writer.Dispose() } catch [IO.IOException] {} }
    } finally { $pipe.Dispose() }
}
function Get-MoonDaemon {
    param([switch]$Installed,[MoonInstallNative+ProcessIdentityInfo]$ExpectedPeer,[string]$DevelopmentPath,[MoonInstallNative+ProcessIdentityInfo]$ExpectedPortablePeer,[string]$ExpectedPortableSha256)
    try {Invoke-MoonRpc query stats -Installed:$Installed -ExpectedPeer $ExpectedPeer -DevelopmentPath $DevelopmentPath -ExpectedPortablePeer $ExpectedPortablePeer -ExpectedPortableSha256 $ExpectedPortableSha256} catch {return $null}
}
function Assert-MoonPortableActive($Daemon) {
    if (!$Daemon.PSObject.Properties['paused'] -or $Daemon.paused -isnot [bool] -or $Daemon.paused) {throw 'Resume tiling in portable Moon before setup; the current desktop was left unchanged.'}
    if (!$Daemon.PSObject.Properties['session_suspended'] -or $Daemon.session_suspended -isnot [bool] -or $Daemon.session_suspended) {throw 'Return to the unlocked desktop before switching Moon installations; the current desktop was left unchanged.'}
}
function Get-MoonPortableSource([string]$Path) {
    if ($Path) {
        $Path=(Resolve-Path -LiteralPath $Path).ProviderPath
        $running=Get-MoonDaemon -DevelopmentPath $Path
        if (!$running) {throw 'The selected portable Moon could not be verified. Older versions must exit from their tray menu once before setup.'}
    } else {
        # The reviewed development location is the only implicit candidate.
        $running=Get-MoonDaemon
        if (!$running -or $running.token.ui_access) {return $null}
    }
    if ($running.token.ui_access) {throw 'Portable migration requires an ordinary Moon instance.'}
    $sha=(Get-FileHash -LiteralPath $running.executable -Algorithm SHA256).Hash
    $verified=Invoke-MoonRpc query stats -ExpectedPortablePeer $running.pipe_peer -ExpectedPortableSha256 $sha
    Assert-MoonPortableActive $verified
    [void](Get-MoonSelectedConfigPath $verified)
    [pscustomobject]@{daemon=$verified;peer=$verified.pipe_peer;sha256=$sha}
}
function Find-MoonPortableDaemon([string]$Path,[string]$Sha256) {
    # Recovery may have a new PID, but never a new image, logon or privilege.
    # Inspect only the explicitly selected image; no arbitrary Moon discovery.
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ne $Sha256) {throw 'The saved portable executable changed; it was not launched or trusted.'}
    $session=[Diagnostics.Process]::GetCurrentProcess().SessionId
    foreach ($process in @(Get-CimInstance Win32_Process | Where-Object {$_.SessionId -eq $session -and $_.Name -ieq [IO.Path]::GetFileName($Path)})) {
        try {
            $peer=[MoonInstallNative]::ProcessIdentity([uint32]$process.ProcessId)
            if (![string]::Equals($peer.Path,$Path,[StringComparison]::OrdinalIgnoreCase)) {continue}
            $running=Get-MoonDaemon -ExpectedPortablePeer $peer -ExpectedPortableSha256 $Sha256
            if ($running) {return $running}
        } catch {Write-Verbose 'A portable candidate exited or could not be verified.'}
    }
    return $null
}
function Wait-MoonMigrationDrained([string]$PortablePath) {
    $session=[Diagnostics.Process]::GetCurrentProcess().SessionId
    $names=@('moon.exe',[IO.Path]::GetFileName($PortablePath))
    Wait-MoonCondition -Description 'Moon and its recovery watchdogs to finish restoring windows' -Condition {
        @(Get-CimInstance Win32_Process | Where-Object {$_.SessionId -eq $session -and $names -contains $_.Name}).Count -eq 0
    }
}
function Get-MoonConfigFingerprint($Value) {
    if ($null -eq $Value) {return 'null'}
    # PowerShell can wrap JSON arrays/scalars in PSObject. Test their native
    # shapes first; inspecting wrapper properties would discard actual values.
    if ($Value -is [Array]) {
        $parts=foreach($item in $Value) {Get-MoonConfigFingerprint $item}
        return '['+(@($parts) -join ',')+']'
    }
    if ($Value -is [string] -or $Value -is [ValueType]) {return (ConvertTo-Json -InputObject $Value -Compress)}
    if ($Value -is [pscustomobject]) {
        $parts=foreach($property in @($Value.PSObject.Properties | Sort-Object Name)) {(ConvertTo-Json -InputObject $property.Name -Compress)+':'+(Get-MoonConfigFingerprint $property.Value)}
        return '{'+(@($parts) -join ',')+'}'
    }
    throw 'The configuration checkpoint has an unsupported value.'
}
function Assert-MoonRestartConfig {
    param($Snapshot,$ConfigIdentity,[hashtable]$PeerArguments)
    $state=[pscustomobject]@{config_path=$ConfigIdentity.config_path}
    Assert-MoonConfigIdentity $ConfigIdentity $state
    # Parsing happens in the existing daemon under its own token. A failed
    # reload may leave a working desktop whose current file cannot be restarted.
    $restart=Invoke-MoonRpc query restart-config @PeerArguments
    $record=([Text.UTF8Encoding]::new($false,$true)).GetString($Snapshot.journal) | ConvertFrom-Json
    if (!$record.world.PSObject.Properties['paused'] -or $record.world.paused -isnot [bool] -or $record.world.paused) {throw 'Resume tiling before setup; a paused desktop checkpoint cannot preserve native positions through a normal exit.'}
    if (!$record.world.PSObject.Properties['config'] -or $restart -isnot [pscustomobject] -or (Get-MoonConfigFingerprint $record.world.config) -cne (Get-MoonConfigFingerprint $restart)) {
        throw 'Saved settings do not match the running desktop. Apply or correct them before setup; Moon was left running.'
    }
    Assert-MoonConfigIdentity $ConfigIdentity $state
}
function Test-MoonCandidateConfig {
    param([string]$Target,$State,$ConfigIdentity)
    $arguments=@('config','validate')
    $selected=Get-MoonConfiguredPath $State
    if ($selected) {$arguments+=@('--config',$selected)}
    if ($ConfigIdentity) {Assert-MoonConfigIdentity $ConfigIdentity ([pscustomobject]@{config_path=$ConfigIdentity.config_path})}
    # Target is the signed executable already published in protected Program
    # Files. Never execute the user-writable portable image from this process.
    # A signed UIAccess image requires the shell's AppInfo launch broker;
    # direct CreateProcess can fail with ERROR_ELEVATION_REQUIRED (740).
    $exitCode=Invoke-MoonShellProcess $Target $arguments -TimeoutSeconds 20
    if ($ConfigIdentity) {Assert-MoonConfigIdentity $ConfigIdentity ([pscustomobject]@{config_path=$ConfigIdentity.config_path})}
    if ($exitCode -ne 0) {throw "The selected settings failed validation with the installed Moon candidate (exit code $exitCode)."}
}
function Save-MoonPortableHandoff {
    param([MoonInstallNative+ProcessIdentityInfo]$Peer,[string]$Sha256,$ConfigIdentity,[string]$JournalPath,[string]$Directory)
    $pins=@{ExpectedPortablePeer=$Peer;ExpectedPortableSha256=$Sha256}
    $current=Invoke-MoonRpc query stats @pins
    Assert-MoonPortableActive $current
    $identity=Get-MoonDesktopIdentity $current
    if ($identity.sha256 -ne $Sha256) {throw 'The portable executable changed during setup.'}
    Assert-MoonConfigIdentity $ConfigIdentity $current
    $snapshot=Get-MoonUpgradeSnapshot -JournalPath $JournalPath -ProcessId $Peer.ProcessId @pins
    Assert-MoonRestartConfig $snapshot $ConfigIdentity $pins
    Save-MoonDesktopCheckpoint $snapshot $identity $Directory
}
function Stop-MoonPortableCheckpoint {
    param($Checkpoint,[MoonInstallNative+ProcessIdentityInfo]$Peer,[ref]$ShutdownRequested)
    $pins=@{ExpectedPortablePeer=$Peer;ExpectedPortableSha256=$Checkpoint.identity.sha256}
    $running=Invoke-MoonRpc query stats @pins
    Assert-MoonPortableActive $running
    Assert-MoonDesktopIdentity $Checkpoint.identity $running
    if ((Get-MoonLayoutFingerprint @pins) -ne $Checkpoint.layout) {throw 'Your window layout changed before the switch. Moon was left running; retry setup.'}
    $record=([Text.UTF8Encoding]::new($false,$true)).GetString((Read-MoonDesktopCheckpoint $Checkpoint)) | ConvertFrom-Json
    if (!$record.world.PSObject.Properties['paused'] -or $record.world.paused -isnot [bool] -or $record.world.paused) {throw 'Resume tiling before setup; the paused checkpoint was not transferred.'}
    $Checkpoint.status='stopping'
    Write-MoonJsonFile $Checkpoint.receipt_path $Checkpoint
    # A reply can be lost after exit was accepted. Own recovery before sending,
    # and decide whether exit finished from native process identity and drainage.
    $ShutdownRequested.Value=$true
    try {Invoke-MoonRpc lifecycle -LifecycleAction exit @pins | Out-Null} catch {Write-Verbose $_}
    Wait-MoonCondition -Description 'the captured portable Moon to exit' -Condition {![MoonInstallNative]::ProcessStillRunning($Peer)}
    Wait-MoonMigrationDrained $Checkpoint.identity.path
}
function Resume-MoonPortableCheckpoint {
    param($Checkpoint,[string]$JournalPath,$State,[MoonInstallNative+ProcessIdentityInfo]$OriginalPeer)
    $identity=$Checkpoint.identity
    Assert-MoonConfigIdentity $identity ([pscustomobject]@{config_path=$identity.config_path})
    $running=if([MoonInstallNative]::ProcessStillRunning($OriginalPeer)) {
        Invoke-MoonRpc query stats -ExpectedPortablePeer $OriginalPeer -ExpectedPortableSha256 $identity.sha256
    } else {Find-MoonPortableDaemon $identity.path $identity.sha256}
    if (!$running) {
        Wait-MoonMigrationDrained $identity.path
        [void](Read-MoonDesktopCheckpoint $Checkpoint)
        if (Test-Path -LiteralPath $JournalPath) {
            Assert-MoonNoReparse $JournalPath
            if ((Get-FileHash -LiteralPath $JournalPath -Algorithm SHA256).Hash -ne $Checkpoint.journal_sha256) {throw 'Another recovery journal remains. The original checkpoint was retained for recovery.'}
        } else {Publish-MoonDesktopCheckpoint $Checkpoint $JournalPath}
        $launch=[pscustomobject]@{owner_sid=$State.owner_sid;install_id=$State.install_id;config_path=$identity.launch_config_path}
        Start-MoonLimitedOnce $identity.path (Get-MoonLaunchArguments $launch -Resume) $launch -ExpectedPortableSha256 $identity.sha256
        $running=Find-MoonPortableDaemon $identity.path $identity.sha256
    }
    if (!$running -or (Get-MoonSelectedConfigPath $running) -ne $identity.launch_config_path) {throw 'The portable Moon recovery did not retain its original settings selection.'}
    $pins=@{ExpectedPortablePeer=$running.pipe_peer;ExpectedPortableSha256=$identity.sha256}
    Wait-MoonCondition -Description 'the original portable desktop layout' -Condition {(Get-MoonLayoutFingerprint @pins) -eq $Checkpoint.layout}
    $Checkpoint.status='restored'
    try {Write-MoonJsonFile $Checkpoint.receipt_path $Checkpoint} catch {Write-Warning 'Moon recovered, but its checkpoint receipt could not be updated.'}
    return $running
}
function Wait-MoonCondition([scriptblock]$Condition,[string]$Description,[int]$Seconds=20) {
    $until=[DateTime]::UtcNow.AddSeconds($Seconds); $last=''
    do { try { if (& $Condition) { return } } catch { $last=$_.Exception.Message }; Start-Sleep -Milliseconds 150 } while ([DateTime]::UtcNow -lt $until)
    throw "Timed out waiting for $Description. $last"
}
function Assert-MoonNoReparse([string]$Path) {
    $pathNow=[IO.Path]::GetFullPath($Path)
    while ($pathNow) {
        if (Test-Path -LiteralPath $pathNow) {
            if ((Get-Item -Force -LiteralPath $pathNow).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point is not allowed for installation: $pathNow" }
        }
        $parent=[IO.Path]::GetDirectoryName($pathNow.TrimEnd('\'))
        if (!$parent -or $parent -eq $pathNow) { break }; $pathNow=$parent
    }
}
function Assert-MoonSecurePath([string]$Path) {
    Assert-MoonNoReparse $Path
    $acl=Get-Acl -LiteralPath $Path
    $trusted=@('S-1-5-18','S-1-5-32-544','S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464')
    $owner=([Security.Principal.NTAccount]::new($acl.Owner)).Translate([Security.Principal.SecurityIdentifier]).Value
    if ($trusted -notcontains $owner) { throw "Installation path owner is not an administrative identity: $Path" }
    $write=[Security.AccessControl.FileSystemRights]::Write -bor [Security.AccessControl.FileSystemRights]::Delete -bor [Security.AccessControl.FileSystemRights]::ChangePermissions -bor [Security.AccessControl.FileSystemRights]::TakeOwnership
    foreach ($rule in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
        if ($rule.AccessControlType -eq 'Allow' -and ($rule.FileSystemRights -band $write) -ne 0 -and $trusted -notcontains $rule.IdentityReference.Value) { throw "Installation path grants write access outside administrators: $Path" }
    }
}
function Protect-MoonDirectory([string]$Path) {
    $acl=[Security.AccessControl.DirectorySecurity]::new(); $acl.SetAccessRuleProtection($true,$false)
    $administrators=[Security.Principal.SecurityIdentifier]::new('S-1-5-32-544')
    $acl.SetOwner($administrators)
    foreach ($entry in @(@('S-1-5-32-544','FullControl'),@('S-1-5-18','FullControl'),@('S-1-5-32-545','ReadAndExecute'))) {
        $sid=[Security.Principal.SecurityIdentifier]::new($entry[0])
        $rule=[Security.AccessControl.FileSystemAccessRule]::new($sid,[Security.AccessControl.FileSystemRights]$entry[1],([Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'),[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow)
        [void]$acl.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
    Assert-MoonSecurePath $Path
}
function Read-MoonInstallState([string]$Directory) {
    Assert-MoonSecurePath $Directory
    $path=Join-Path $Directory 'installation.json'; Assert-MoonNoReparse $path
    Assert-MoonSecurePath $path
    $state=Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
    $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    if ($state.version -ne 1 -or $state.owner_sid -ne $sid -or $state.install_id -notmatch '^[a-f0-9]{32}$') { throw 'Installation ownership or metadata version does not match this user.' }
    if ($state.task_name -ne ('moon-' + $sid)) { throw 'Unexpected startup task identity in installation metadata.' }
    [void](Get-MoonConfiguredPath $state)
    return $state
}
function Get-MoonConfiguredPath($State) {
    if (!$State -or !$State.PSObject.Properties['config_path'] -or $null -eq $State.config_path) {return $null}
    if ($State.config_path -isnot [string] -or [string]::IsNullOrWhiteSpace($State.config_path) -or $State.config_path -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\/]+[\\/][^\\/]+(?:[\\/]|$))') {throw 'Moon configuration path must be an absolute path.'}
    [IO.Path]::GetFullPath($State.config_path)
}
function Get-MoonSelectedConfigPath($Daemon) {
    # The resolved path is also reported for built-in defaults, even when no
    # file exists. Only an explicit selection may become a --config argument.
    # Do not infer this from file existence or from a default-looking path.
    if (!$Daemon -or !$Daemon.PSObject.Properties['config_explicit'] -or $Daemon.config_explicit -isnot [bool]) {
        throw 'Cannot preserve how this Moon instance selected its settings. Update or stop it before setup.'
    }
    $path=Get-MoonConfiguredPath $Daemon
    if (!$path) {throw 'The running Moon instance did not identify its configuration path.'}
    if ($Daemon.config_explicit) {return $path}
    return $null
}
function Get-MoonLaunchArguments {
    param($State,[switch]$Resume,[switch]$Settings)
    $arguments=if($Settings){@()}else{@('start','--background')}
    if ($Resume) {if($Settings){throw 'Settings cannot resume a desktop checkpoint.'};$arguments+='--resume'}
    $config=Get-MoonConfiguredPath $State
    if ($config) {$arguments+=@('--config',$config)}
    (@($arguments | ForEach-Object {ConvertTo-MoonArgument $_}) -join ' ')
}
function Get-MoonConfigIdentity($State) {
    $path=Get-MoonConfiguredPath $State
    if (!$path) {throw 'The running Moon instance did not identify its configuration path.'}
    $exists=Test-Path -LiteralPath $path -PathType Leaf
    if (!$exists -and (Test-Path -LiteralPath $path)) {throw 'The Moon configuration path is not a file.'}
    [pscustomobject]@{config_path=$path;config_exists=$exists;config_sha256=$(if($exists){(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash}else{$null})}
}
function Assert-MoonConfigIdentity($Expected,$State) {
    $actual=Get-MoonConfigIdentity $State
    if (![string]::Equals($actual.config_path,$Expected.config_path,[StringComparison]::OrdinalIgnoreCase) -or $actual.config_exists -ne $Expected.config_exists -or $actual.config_sha256 -ne $Expected.config_sha256) {
        throw 'Your Moon configuration changed during setup. It was left unchanged; retry with the current settings.'
    }
}
function Get-MoonDesktopIdentity($Daemon) {
    $native=[MoonInstallNative]::ProcessIdentity([uint32]$Daemon.process_id)
    if (![string]::Equals($native.Path,$Daemon.executable,[StringComparison]::OrdinalIgnoreCase)) {throw 'Moon changed executable identity during setup.'}
    $selected=Get-MoonSelectedConfigPath $Daemon
    $config=Get-MoonConfigIdentity $Daemon
    [pscustomobject]@{process_id=[uint32]$Daemon.process_id;process_birth=$native.Birth;path=$native.Path;sha256=(Get-FileHash -LiteralPath $native.Path -Algorithm SHA256).Hash;launch_config_path=$selected;config_path=$config.config_path;config_exists=$config.config_exists;config_sha256=$config.config_sha256}
}
function Assert-MoonDesktopIdentity($Expected,$Daemon) {
    if (!$Daemon -or $Daemon.process_id -ne $Expected.process_id) {throw 'The captured Moon process is no longer running; no replacement was stopped.'}
    $actual=Get-MoonDesktopIdentity $Daemon
    if ($actual.process_birth -ne $Expected.process_birth -or ![string]::Equals($actual.path,$Expected.path,[StringComparison]::OrdinalIgnoreCase) -or $actual.sha256 -ne $Expected.sha256 -or $actual.launch_config_path -ne $Expected.launch_config_path) {throw 'The captured Moon executable or launch settings changed; it was not stopped.'}
    Assert-MoonConfigIdentity $Expected $Daemon
}
function Save-MoonDesktopCheckpoint {
    param($Snapshot,$Identity,[string]$Directory)
    # The caller supplies an already authenticated live peer and a consistent
    # snapshot. This function persists evidence; it never contacts or stops Moon.
    if (!$Snapshot -or !$Snapshot.journal -or $Snapshot.journal.Length -gt 16777216) {throw 'Invalid desktop checkpoint size.'}
    $record=([Text.UTF8Encoding]::new($false,$true)).GetString($Snapshot.journal) | ConvertFrom-Json
    if ($record.version -ne 1 -or $record.owner -ne $Identity.process_id -or $record.owner_birth -ne $Identity.process_birth -or $record.world.revision -ne $Snapshot.revision) {throw 'The desktop checkpoint does not belong to the captured Moon process.'}
    if (Test-Path -LiteralPath $Directory) {throw 'The recovery checkpoint directory is already occupied.'}
    $directory=[IO.Path]::GetFullPath($Directory)
    Assert-MoonNoReparse ([IO.Path]::GetDirectoryName($directory))
    [void][IO.Directory]::CreateDirectory($directory)
    $checkpoint=[pscustomobject]@{version=1;status='saved';identity=$Identity;layout=$Snapshot.layout;revision=$Snapshot.revision;journal_path=(Join-Path $directory 'journal.json');journal_sha256=(Get-MoonBytesHash $Snapshot.journal);receipt_path=(Join-Path $directory 'checkpoint.json')}
    $stream=[IO.FileStream]::new($checkpoint.journal_path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try {$stream.Write($Snapshot.journal,0,$Snapshot.journal.Length);$stream.Flush($true)} finally {$stream.Dispose()}
    Write-MoonJsonFile $checkpoint.receipt_path $checkpoint
    return $checkpoint
}
function Read-MoonDesktopCheckpoint($Checkpoint) {
    Assert-MoonNoReparse $Checkpoint.journal_path
    $stream=[IO.FileStream]::new($Checkpoint.journal_path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    $memory=[IO.MemoryStream]::new()
    try {
        if ($stream.Length -gt 16777216) {throw 'Recovery checkpoint exceeds 16 MiB.'}
        $stream.CopyTo($memory);$bytes=$memory.ToArray()
    } finally {$memory.Dispose();$stream.Dispose()}
    if ((Get-MoonBytesHash $bytes) -ne $Checkpoint.journal_sha256) {throw 'The retained desktop checkpoint changed; it was not replayed.'}
    return ,$bytes
}
function Publish-MoonDesktopCheckpoint {
    param($Checkpoint,[string]$JournalPath)
    # The caller must drain its verified daemon/watchdog before publishing.
    # Never replace an existing journal: a new owner or interrupted recovery
    # must be handled explicitly instead of losing its native-window state.
    $bytes=Read-MoonDesktopCheckpoint $Checkpoint
    Assert-MoonNoReparse $JournalPath
    if (Test-Path -LiteralPath $JournalPath) {throw 'A live recovery journal already exists; the saved checkpoint was retained.'}
    $temporary=$JournalPath+'.install-'+[guid]::NewGuid().ToString('N')+'.tmp'
    try {
        $stream=[IO.FileStream]::new($temporary,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try {$stream.Write($bytes,0,$bytes.Length);$stream.Flush($true)} finally {$stream.Dispose()}
        [IO.File]::Move($temporary,$JournalPath)
    } finally {if(Test-Path -LiteralPath $temporary){Remove-Item -LiteralPath $temporary -Force}}
}
function Write-MoonJsonFile([string]$Path,$Value) {
    $directory=[IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($Path))
    $temporary=Join-Path $directory ('moon-write.'+[guid]::NewGuid().ToString('N')+'.tmp')
    try {
    [IO.File]::WriteAllText($temporary,($Value | ConvertTo-Json -Depth 20),[Text.UTF8Encoding]::new($false))
    # The directory is Administrators-owned in a real install. Set the same
    # owner explicitly so a creator's token default cannot leave user-owned
    # security metadata with the owner's implicit permission to change its ACL.
    $security=Get-Acl -LiteralPath $temporary
    $security.SetOwner((Get-Acl -LiteralPath $directory).GetOwner([Security.Principal.SecurityIdentifier]))
    Set-Acl -LiteralPath $temporary -AclObject $security
    if (Test-Path -LiteralPath $Path) { [IO.File]::Replace($temporary,$Path,[NullString]::Value) } else { [IO.File]::Move($temporary,$Path) }
    } finally {if (Test-Path -LiteralPath $temporary) {Remove-Item -LiteralPath $temporary -Force -ErrorAction SilentlyContinue}}
}
function Write-MoonInstallState([string]$Directory,$State) { Write-MoonJsonFile (Join-Path $Directory 'installation.json') $State }
function Assert-MoonCertificate($Certificate,[string]$InstallId) {
    if (!$Certificate -or !$Certificate.HasPrivateKey -or $Certificate.Subject -ne "CN=moon code signing $InstallId") { throw 'The installation signing certificate is missing or does not belong to this installation.' }
    $eku=@($Certificate.Extensions | Where-Object {$_.Oid.Value -eq '2.5.29.37'} | ForEach-Object {$_.EnhancedKeyUsages} | ForEach-Object {$_.Value})
    if ($eku.Count -ne 1 -or $eku[0] -ne '1.3.6.1.5.5.7.3.3') { throw 'The signing certificate must be restricted to code signing.' }
    $key=[Security.Cryptography.X509Certificates.RSACertificateExtensions]::GetRSAPrivateKey($Certificate)
    try { if ($key -isnot [Security.Cryptography.RSACng] -or $key.Key.ExportPolicy -ne [Security.Cryptography.CngExportPolicies]::None) { throw 'The signing private key must be a non-exportable CNG key.' } } finally { if ($key) {$key.Dispose()} }
}
function Assert-MoonOwnedTask($Task,$State,[string]$Target) {
    if (!$Task) { return }
    if ($Task.Description -ne "moon installation $($State.install_id); owner $($State.owner_sid)") { throw 'The existing startup task is not owned by this moon installation.' }
    if ($Task.Principal.RunLevel -ne 'Limited' -or @($Task.Actions).Count -ne 1) { throw 'Unexpected privilege or action configuration on the owned startup task.' }
    $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    if (![string]::Equals($Task.Actions[0].Execute,$powershell,[StringComparison]::OrdinalIgnoreCase) -or $Task.Actions[0].Arguments -notmatch '^-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand ([A-Za-z0-9+/=]+)$') { throw 'Unexpected startup task executable or arguments.' }
    $launch=[Text.Encoding]::Unicode.GetString([Convert]::FromBase64String($Matches[1]))
    $legacy="Start-Process -FilePath '$($Target.Replace("'","''"))' -ArgumentList 'start --background' -WindowStyle Hidden"
    $expected=Get-MoonTaskLaunchScript $Target (Get-MoonLaunchArguments $State)
    if ($launch -ne $expected -and ((Get-MoonConfiguredPath $State) -or $launch -ne $legacy)) { throw 'The startup task points outside the expected moon launch operation.' }
    $principal=$Task.Principal.UserId
    if ($principal -notmatch '^S-1-') { $principal=([Security.Principal.NTAccount]::new($principal)).Translate([Security.Principal.SecurityIdentifier]).Value }
    if ($principal -ne $State.owner_sid) { throw 'Unexpected startup task user.' }
}
function Get-MoonLayoutFingerprint {
    param($World,[object[]]$Placements,[switch]$Installed,[MoonInstallNative+ProcessIdentityInfo]$ExpectedPeer,[string]$DevelopmentPath,[MoonInstallNative+ProcessIdentityInfo]$ExpectedPortablePeer,[string]$ExpectedPortableSha256)
    if (!$PSBoundParameters.ContainsKey('World')) {$World=Invoke-MoonRpc query world -Installed:$Installed -ExpectedPeer $ExpectedPeer -DevelopmentPath $DevelopmentPath -ExpectedPortablePeer $ExpectedPortablePeer -ExpectedPortableSha256 $ExpectedPortableSha256}
    if (!$PSBoundParameters.ContainsKey('Placements')) {$Placements=@(Invoke-MoonRpc query windows -Installed:$Installed -ExpectedPeer $ExpectedPeer -DevelopmentPath $DevelopmentPath -ExpectedPortablePeer $ExpectedPortablePeer -ExpectedPortableSha256 $ExpectedPortableSha256)}
    # Preserve tree child order, but normalize unordered maps and window lists.
    # Revisions, process lifetimes and changing titles are not layout semantics.
    $state=[ordered]@{
        focused=$World.focused
        active_monitor=$World.active_monitor
        active=@($World.active.PSObject.Properties | Sort-Object Name | ForEach-Object {[ordered]@{monitor=$_.Name;workspace=$_.Value}})
        workspaces=@($World.workspaces | Sort-Object id | ForEach-Object {[ordered]@{id=$_.id;monitor=$_.monitor;tree=$_.tree;focused=$_.focused}})
        windows=@($World.windows.PSObject.Properties | Sort-Object Name | ForEach-Object {[ordered]@{id=$_.Name;workspace=$_.Value.workspace;floating=$_.Value.floating;fullscreen=$_.Value.fullscreen;suspended=$_.Value.suspended;floating_rect=$_.Value.floating_rect}})
        placements=@($Placements | Sort-Object id | ForEach-Object {[ordered]@{id=$_.id;visible=$_.visible;rect=$_.rect;floating=$_.floating;fullscreen=$_.fullscreen}})
    }
    $state | ConvertTo-Json -Depth 100 -Compress
}
function Get-MoonUpgradeSnapshot {
    param([string]$JournalPath,[uint32]$ProcessId,[int]$TimeoutMilliseconds=5000,[MoonInstallNative+ProcessIdentityInfo]$ExpectedPeer,[MoonInstallNative+ProcessIdentityInfo]$ExpectedPortablePeer,[string]$ExpectedPortableSha256)
    $pins=@{ExpectedPeer=$ExpectedPeer;ExpectedPortablePeer=$ExpectedPortablePeer;ExpectedPortableSha256=$ExpectedPortableSha256}
    $identity=if($ExpectedPortablePeer){$ExpectedPortablePeer}elseif($ExpectedPeer){$ExpectedPeer}else{[MoonInstallNative]::ProcessIdentity($ProcessId)}
    $until=[DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    $last='The journal checkpoint has not caught up with the current layout.'
    do {
        try {
            $before=Invoke-MoonRpc query world @pins
            $layout=Get-MoonLayoutFingerprint -World $before @pins
            # The writer publishes by atomic rename; preserve delete-sharing
            # while reading one immutable opened version of the journal.
            $stream=[IO.FileStream]::new($JournalPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
            $memory=[IO.MemoryStream]::new()
            try {
                if ($stream.Length -gt 16777216) {throw 'Recovery journal exceeds 16 MiB.'}
                $stream.CopyTo($memory); $bytes=$memory.ToArray()
            } finally {$memory.Dispose(); $stream.Dispose()}
            $journal=([Text.UTF8Encoding]::new($false,$true)).GetString($bytes) | ConvertFrom-Json
            $after=Invoke-MoonRpc query world @pins
            if ($journal.version -eq 1 -and $journal.owner -eq $ProcessId -and $journal.owner_birth -eq $identity.Birth -and $before.revision -eq $after.revision -and $journal.world.revision -eq $after.revision) {
                return [pscustomobject]@{layout=$layout;journal=$bytes;revision=$after.revision}
            }
        } catch {$last=$_.Exception.Message}
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $until)
    throw "Cannot capture a consistent upgrade recovery checkpoint; the executable was not replaced. $last"
}

function Get-MoonTaskLaunchScript([string]$Target,[string]$Arguments='start --background') {
    $prefix="`$target='$($Target.Replace("'","''"))'`n`$arguments='$($Arguments.Replace("'","''"))'`n"
    $prefix+@'
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$directory=Join-Path $env:LOCALAPPDATA 'moon'
$path=Join-Path $directory ('startup-session'+[Diagnostics.Process]::GetCurrentProcess().SessionId+'.json')
$record=[ordered]@{version=1;attempt=[guid]::NewGuid().ToString('N');started_at=[DateTime]::UtcNow.ToString('o');updated_at='';worker_pid=$PID;target=$target;arguments=$arguments;phase='launching';child_pid=$null;exit_code=$null;error=$null;native_error_code=$null;environment=[ordered]@{user_profile=$env:USERPROFILE;local_app_data=$env:LOCALAPPDATA;path_extensions=$env:PATHEXT;current_directory=[Environment]::CurrentDirectory;session_id=[Diagnostics.Process]::GetCurrentProcess().SessionId}}
function Write-StartupRecord {
    $temporary=$null
    try {
        $record.updated_at=[DateTime]::UtcNow.ToString('o')
        [void][IO.Directory]::CreateDirectory($directory)
        $temporary=Join-Path $directory ('startup-'+[guid]::NewGuid().ToString('N')+'.tmp')
        $text=$record|ConvertTo-Json -Depth 4
        if([Text.Encoding]::UTF8.GetByteCount($text) -gt 65536){throw 'Startup diagnostic exceeded its size limit.'}
        [IO.File]::WriteAllText($temporary,$text,[Text.UTF8Encoding]::new($false))
        if(Test-Path -LiteralPath $path){[IO.File]::Replace($temporary,$path,[NullString]::Value)}else{[IO.File]::Move($temporary,$path)}
    }catch{Write-Warning ('Moon startup diagnostic could not be written: '+$_.Exception.Message) -WarningAction Continue}
    finally{if($temporary -and (Test-Path -LiteralPath $temporary)){Remove-Item -LiteralPath $temporary -Force -ErrorAction SilentlyContinue}}
}
$child=$null
try {
    Write-StartupRecord
    $child=Start-Process -FilePath $target -ArgumentList $arguments -WindowStyle Hidden -PassThru
    $record.child_pid=$child.Id;$record.phase='started';Write-StartupRecord
    # Process.WaitForExit waits only for this launcher, unlike Start-Process
    # -Wait, which can wait for its long-lived daemon descendants as well.
    if(!$child.WaitForExit(15000)){
        $record.phase='timed-out';$record.error='The launch process is still running after 15 seconds; it was not terminated.'
        $record.exit_code=1460;Write-StartupRecord;exit 1460
    }
    $record.phase='exited';$record.exit_code=$child.ExitCode;Write-StartupRecord
    exit $child.ExitCode
}catch{
    $failure=$_;$record.phase='failed'
    $record.error=$failure.Exception.ToString()
    if($record.error.Length -gt 8192){$record.error=$record.error.Substring(0,8192)}
    $exception=$failure.Exception
    while($exception){if($exception -is [ComponentModel.Win32Exception]){$record.native_error_code=$exception.NativeErrorCode;break};$exception=$exception.InnerException}
    $record.exit_code=1;Write-StartupRecord
    Write-Error -ErrorRecord $failure -ErrorAction Continue
    exit 1
}finally{if($child){$child.Dispose()}}
'@
}
function Get-MoonTaskStartupDiagnostic([string]$Target,[datetime]$Since) {
    $path=Join-Path $env:LOCALAPPDATA ('moon\startup-session'+[Diagnostics.Process]::GetCurrentProcess().SessionId+'.json')
    try {
        if(!(Test-Path -LiteralPath $path)){return "Startup task did not write a diagnostic record: $path"}
        $stream=[IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
        try {
            if($stream.Length -gt 65536){throw 'Startup diagnostic exceeded its size limit.'}
            $reader=[IO.StreamReader]::new($stream,[Text.UTF8Encoding]::new($false,$true))
            try{$record=$reader.ReadToEnd()|ConvertFrom-Json}finally{$reader.Dispose()}
        }finally{$stream.Dispose()}
        if($record.version -ne 1 -or ![string]::Equals($record.target,$Target,[StringComparison]::OrdinalIgnoreCase) -or [datetime]::Parse($record.started_at).ToUniversalTime() -lt $Since.ToUniversalTime()){
            return "Startup task has no matching diagnostic from this attempt: $path"
        }
        return "Startup task diagnostic ($path):`n$($record|ConvertTo-Json -Depth 4)"
    }catch{return "Startup task diagnostic could not be read ($path): $($_.Exception.Message)"}
}
function New-MoonTaskAction([string]$Target,[string]$Arguments='start --background') {
    $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $launch=Get-MoonTaskLaunchScript $Target $Arguments
    $encoded=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($launch))
    New-ScheduledTaskAction -Execute $powershell -Argument ('-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand '+$encoded)
}
function Start-MoonLimitedOnce([string]$Target,[string]$Arguments,$State,[string]$ExpectedPortableSha256) {
    if ($ExpectedPortableSha256 -and (Get-FileHash -LiteralPath $Target -Algorithm SHA256).Hash -ne $ExpectedPortableSha256) {throw 'The portable recovery executable changed before launch.'}
    $name='moon-install-'+[guid]::NewGuid().ToString('N')
    $principal=New-ScheduledTaskPrincipal -UserId $State.owner_sid -LogonType Interactive -RunLevel Limited
    $settings=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::FromMinutes(1))
    Register-ScheduledTask -TaskName $name -Description "moon temporary installation $($State.install_id)" -Action (New-MoonTaskAction $Target $Arguments) -Principal $principal -Settings $settings | Out-Null
    try {
        $launchAfter=[DateTime]::UtcNow
        Start-ScheduledTask -TaskName $name
        try {
            Wait-MoonCondition -Description 'the restored Moon desktop' -Condition {
                if ($ExpectedPortableSha256) {return [bool](Find-MoonPortableDaemon $Target $ExpectedPortableSha256)}
                $running=Get-MoonDaemon -Installed
                $running -and [string]::Equals($running.executable,$Target,[StringComparison]::OrdinalIgnoreCase) -and $running.token.ui_access
            }
        }catch{throw "$($_.Exception.Message)`n$(Get-MoonTaskStartupDiagnostic $Target $launchAfter)"}
    } finally {Unregister-ScheduledTask -TaskName $name -Confirm:$false}
}

# Shell integration uses a private product key and one recorded shortcut. The
# optional roots let regression checks exercise real APIs in isolated locations.
function Get-MoonShellPaths($State,[string]$ProgramsDirectory=([Environment]::GetFolderPath('Programs'))) {
    if ($State.install_id -notmatch '^[a-f0-9]{32}$') {throw 'Invalid Moon installation identity.'}
    [pscustomobject]@{shortcut=(Join-Path $ProgramsDirectory 'Moon.lnk');registry=('Software\Microsoft\Windows\CurrentVersion\Uninstall\moon-'+$State.install_id)}
}
function Read-MoonShellRegistry([Microsoft.Win32.RegistryKey]$Root,[string]$Path) {
    $key=$Root.OpenSubKey($Path)
    if (!$key) {return [pscustomobject]@{exists=$false;values=@()}}
    try {
        if ($key.GetSubKeyNames().Count) {throw 'Unexpected nested application registration; it was left unchanged.'}
        $values=@($key.GetValueNames() | Sort-Object | ForEach-Object {
            [pscustomobject]@{name=$_;kind=$key.GetValueKind($_).ToString();value=$key.GetValue($_,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)}
        })
        [pscustomobject]@{exists=$true;values=$values}
    } finally {$key.Dispose()}
}
function Assert-MoonShellRegistryOwner($Snapshot,$State) {
    if (!$Snapshot.exists) {return}
    $id=@($Snapshot.values | Where-Object name -eq 'MoonInstallId')
    $owner=@($Snapshot.values | Where-Object name -eq 'MoonOwnerSid')
    if ($id.Count -ne 1 -or $owner.Count -ne 1 -or $id[0].value -ne $State.install_id -or $owner[0].value -ne $State.owner_sid) {throw 'An unrelated application owns this installed-apps entry; it was left unchanged.'}
}
function Get-MoonBytesHash([byte[]]$Bytes) {
    $sha=[Security.Cryptography.SHA256]::Create()
    try {([BitConverter]::ToString($sha.ComputeHash($Bytes))).Replace('-','')} finally {$sha.Dispose()}
}
function New-MoonShellPlan {
    param([string]$Directory,$State,[string]$Version,[string]$ShortcutArguments,[string]$UninstallCommand,
        [string]$ProgramsDirectory=([Environment]::GetFolderPath('Programs')),
        [Microsoft.Win32.RegistryKey]$RegistryRoot=[Microsoft.Win32.Registry]::CurrentUser)
    $paths=Get-MoonShellPaths $State $ProgramsDirectory
    Assert-MoonNoReparse $paths.shortcut
    $beforeRegistry=Read-MoonShellRegistry $RegistryRoot $paths.registry
    Assert-MoonShellRegistryOwner $beforeRegistry $State
    $prior=if ($State.PSObject.Properties['shell']) {$State.shell} else {$null}
    if ($beforeRegistry.exists -and !$prior) {throw 'Existing Moon application registration has no installation ownership record.'}
    $beforeShortcut=$null
    if (Test-Path -LiteralPath $paths.shortcut) {
        $beforeShortcut=[IO.File]::ReadAllBytes($paths.shortcut)
        if (!$prior -or $prior.shortcut_path -ne $paths.shortcut -or (Get-MoonBytesHash $beforeShortcut) -ne $prior.shortcut_sha256) {throw 'A different or edited Moon Start menu shortcut already exists; it was left unchanged.'}
    }
    $target=Join-Path $Directory 'moon.exe'
    $temporary=Join-Path ([IO.Path]::GetTempPath()) ('moon-shortcut-'+[guid]::NewGuid().ToString('N')+'.lnk')
    $shell=New-Object -ComObject WScript.Shell
    try {
        $shortcut=$shell.CreateShortcut($temporary)
        try {
        $shortcut.TargetPath=(Join-Path $Directory 'moon-settings.exe'); $shortcut.Arguments=$ShortcutArguments
            $shortcut.WorkingDirectory=$Directory; $shortcut.Description='A calmer place for your windows.'
            $shortcut.IconLocation=$target+',0'; $shortcut.WindowStyle=1; $shortcut.Save()
            $bytes=[IO.File]::ReadAllBytes($temporary)
        } finally {[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shortcut)}
    } finally {
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)
        if (Test-Path -LiteralPath $temporary) {Remove-Item -LiteralPath $temporary -Force}
    }
    $fields=[ordered]@{
        DisplayName='Moon';DisplayVersion=$Version;Publisher='Moon';DisplayIcon=($target+',0')
        InstallLocation=$Directory;InstallDate=[DateTime]::UtcNow.ToString('yyyyMMdd')
        UninstallString=$UninstallCommand;NoModify=1;NoRepair=1
        MoonInstallId=$State.install_id;MoonOwnerSid=$State.owner_sid
    }
    $size=0L
    foreach ($file in $State.files) { $path=Join-Path $Directory $file; if (Test-Path -LiteralPath $path -PathType Leaf) {$size+=(Get-Item -LiteralPath $path).Length} }
    $fields.EstimatedSize=[int][Math]::Min([int]::MaxValue,[Math]::Ceiling($size/1024.0))
    $values=@($fields.GetEnumerator() | Sort-Object Key | ForEach-Object {[pscustomobject]@{name=$_.Key;value=$_.Value;kind=if ($_.Value -is [int]) {'DWord'} else {'String'}}})
    [pscustomobject]@{
        paths=$paths;before_registry=$beforeRegistry;before_shortcut=$beforeShortcut
        registry=[pscustomobject]@{exists=$true;values=$values};shortcut=$bytes
        ownership=[pscustomobject]@{shortcut_path=$paths.shortcut;shortcut_sha256=(Get-MoonBytesHash $bytes);uninstall_key=$paths.registry}
    }
}
function Set-MoonShellRegistry([Microsoft.Win32.RegistryKey]$Root,[string]$Path,$Snapshot) {
    if (!$Snapshot.exists) {$Root.DeleteSubKeyTree($Path,$false);return}
    $key=$Root.CreateSubKey($Path)
    try {
        foreach ($name in @($key.GetValueNames())) {if ($Snapshot.values.name -notcontains $name) {$key.DeleteValue($name,$false)}}
        foreach ($entry in @($Snapshot.values|Sort-Object @{Expression={if($_.name -in @('MoonInstallId','MoonOwnerSid')){0}else{1}}},name)) {$key.SetValue($entry.name,$entry.value,[Microsoft.Win32.RegistryValueKind]$entry.kind)}
    } finally {$key.Dispose()}
}
function Publish-MoonShellPlan($Plan,$State,[Microsoft.Win32.RegistryKey]$RegistryRoot=[Microsoft.Win32.Registry]::CurrentUser,[ref]$Started) {
    $current=Read-MoonShellRegistry $RegistryRoot $Plan.paths.registry
    if (($current | ConvertTo-Json -Depth 10 -Compress) -cne ($Plan.before_registry | ConvertTo-Json -Depth 10 -Compress)) {throw 'The installed-apps entry changed during setup; the newer entry was preserved.'}
    Assert-MoonNoReparse $Plan.paths.shortcut
    if (Test-Path -LiteralPath $Plan.paths.shortcut) {
        if ($null -eq $Plan.before_shortcut -or (Get-FileHash -LiteralPath $Plan.paths.shortcut -Algorithm SHA256).Hash -ne (Get-MoonBytesHash $Plan.before_shortcut)) {throw 'The Start menu shortcut changed during setup; the newer shortcut was preserved.'}
    } elseif ($null -ne $Plan.before_shortcut) {throw 'The Start menu shortcut was removed during setup; it was not recreated.'}
    # All callers persist Plan.ownership in protected metadata before publication.
    if ($Started) {$Started.Value=$true}
    Set-MoonShellRegistry $RegistryRoot $Plan.paths.registry $Plan.registry
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Plan.paths.shortcut))
    [IO.File]::WriteAllBytes($Plan.paths.shortcut,$Plan.shortcut)
}
function Undo-MoonShellPlan($Plan,$State,[Microsoft.Win32.RegistryKey]$RegistryRoot=[Microsoft.Win32.Registry]::CurrentUser) {
    $current=Read-MoonShellRegistry $RegistryRoot $Plan.paths.registry
    if ($current.exists) {Assert-MoonShellRegistryOwner $current $State}
    Set-MoonShellRegistry $RegistryRoot $Plan.paths.registry $Plan.before_registry
    Assert-MoonNoReparse $Plan.paths.shortcut
    if (Test-Path -LiteralPath $Plan.paths.shortcut) {
        $hash=(Get-FileHash -LiteralPath $Plan.paths.shortcut -Algorithm SHA256).Hash
        if ($hash -ne $Plan.ownership.shortcut_sha256) {
            if ($null -ne $Plan.before_shortcut -and $hash -eq (Get-MoonBytesHash $Plan.before_shortcut)) {return}
            Write-Warning 'A changed Start menu shortcut was preserved during recovery.';return
        }
    }
    if ($null -ne $Plan.before_shortcut) {[IO.File]::WriteAllBytes($Plan.paths.shortcut,$Plan.before_shortcut)}
    elseif (Test-Path -LiteralPath $Plan.paths.shortcut) {Remove-Item -LiteralPath $Plan.paths.shortcut -Force}
}
function Remove-MoonShellIntegration($State,[string]$ProgramsDirectory=([Environment]::GetFolderPath('Programs')),[Microsoft.Win32.RegistryKey]$RegistryRoot=[Microsoft.Win32.Registry]::CurrentUser) {
    if (!$State.PSObject.Properties['shell'] -or !$State.shell) {return}
    $paths=Get-MoonShellPaths $State $ProgramsDirectory
    if ($State.shell.uninstall_key -ne $paths.registry -or $State.shell.shortcut_path -ne $paths.shortcut) {throw 'The saved Moon application registration points outside its owned locations.'}
    $current=Read-MoonShellRegistry $RegistryRoot $paths.registry
    Assert-MoonShellRegistryOwner $current $State
    Assert-MoonNoReparse $paths.shortcut
    if (Test-Path -LiteralPath $paths.shortcut) {
        if ((Get-FileHash -LiteralPath $paths.shortcut -Algorithm SHA256).Hash -eq $State.shell.shortcut_sha256) {Remove-Item -LiteralPath $paths.shortcut -Force}
        else {Write-Warning 'Your edited Moon Start menu shortcut was preserved.'}
    }
    if ($current.exists) {$RegistryRoot.DeleteSubKeyTree($paths.registry,$false)}
}

function Get-MoonSetupFiles([string]$OperationId) {
    if ($OperationId -notmatch '^[a-f0-9]{32}$') {throw 'Invalid setup operation identity.'}
    $directory=Join-Path $env:LOCALAPPDATA 'moon\setup'
    Assert-MoonNoReparse $directory
    [pscustomobject]@{directory=$directory;status=(Join-Path $directory ($OperationId+'.json'));cancel=(Join-Path $directory ($OperationId+'.cancel'))}
}
function Test-MoonApprovalCancelled($ErrorRecord) {
    $exception=$ErrorRecord.Exception
    while ($exception) {
        if ($exception -is [ComponentModel.Win32Exception] -and $exception.NativeErrorCode -eq 1223) {return $true}
        $exception=$exception.InnerException
    }
    return $false
}
function Write-MoonSetupStatus([string]$OperationId,[string]$Status,[string]$Message,[string]$Detail='') {
    if (!$OperationId) {return}
    try {
    $files=Get-MoonSetupFiles $OperationId
    [void][IO.Directory]::CreateDirectory($files.directory)
    Assert-MoonNoReparse $files.status
    Write-MoonJsonFile $files.status ([pscustomobject]@{version=1;operation=$OperationId;status=$Status;message=$Message;detail=$Detail;updated_at=[DateTime]::UtcNow.ToString('o')})
    } catch {
        # Progress is advisory. A locked file, full disk, or closed UI must never
        # stop installation recovery or replace the original transaction error.
        Write-Warning ('Setup progress could not be updated: '+$_.Exception.Message) -WarningAction Continue
    }
}
function Read-MoonSetupStatus([string]$OperationId) {
    $files=Get-MoonSetupFiles $OperationId
    if (!(Test-Path -LiteralPath $files.status)) {return $null}
    $stream=[IO.FileStream]::new($files.status,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    try {
        if ($stream.Length -gt 1048576) {throw 'Setup progress exceeds its expected size.'}
        $reader=[IO.StreamReader]::new($stream,[Text.UTF8Encoding]::new($false,$true))
        try {$reader.ReadToEnd() | ConvertFrom-Json} finally {$reader.Dispose()}
    } finally {$stream.Dispose()}
}
function Test-MoonSetupCancellation([string]$OperationId) {
    if ($OperationId -and (Test-Path -LiteralPath (Get-MoonSetupFiles $OperationId).cancel)) {throw [OperationCanceledException]::new('Setup was canceled. Previous installation changes were restored.')}
}
function Get-MoonWebViewRuntime {
    # Microsoft's documented Evergreen runtime identity; Edge browser versions
    # are deliberately not treated as an installed WebView2 runtime.
    $client='Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    foreach ($entry in @(@([Microsoft.Win32.Registry]::CurrentUser,('Software\'+$client)),@([Microsoft.Win32.Registry]::LocalMachine,('Software\WOW6432Node\'+$client)))) {
        $key=$entry[0].OpenSubKey($entry[1])
        if (!$key) {continue}
        try {$version=$null;if ([version]::TryParse([string]$key.GetValue('pv'),[ref]$version) -and $version -gt [version]'0.0.0.0') {return $version.ToString()}}
        finally {$key.Dispose()}
    }
    return $null
}
