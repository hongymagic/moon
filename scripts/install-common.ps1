# Shared local install, uninstall and S8 verification helpers. No action runs on import.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (!('MoonInstallNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
public static class MoonInstallNative {
    [StructLayout(LayoutKind.Sequential)] struct Luid { public uint Low; public int High; }
    [StructLayout(LayoutKind.Sequential)] struct Statistics { public Luid Token, Authentication; public long Expiration; public uint TokenType, ImpersonationLevel, DynamicCharged, DynamicAvailable, GroupCount, PrivilegeCount; public Luid Modified; }
    [StructLayout(LayoutKind.Sequential)] struct Label { public IntPtr Sid; public uint Attributes; }
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr token,int kind,IntPtr value,uint length,out uint returned);
    [DllImport("advapi32.dll",EntryPoint="GetTokenInformation",SetLastError=true)] static extern bool StatisticsOf(IntPtr token,int kind,out Statistics value,uint length,out uint returned);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr process,uint access,out IntPtr token);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool SetTokenInformation(IntPtr token,int kind,ref Label value,uint length);
    [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool ConvertStringSidToSid(string value,out IntPtr sid);
    [DllImport("advapi32.dll")] static extern IntPtr GetSidSubAuthorityCount(IntPtr sid);
    [DllImport("advapi32.dll")] static extern IntPtr GetSidSubAuthority(IntPtr sid,uint index);
    [DllImport("advapi32.dll")] static extern uint GetLengthSid(IntPtr sid);
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
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
    public static string PipeName() { using(var identity=WindowsIdentity.GetCurrent()) { Statistics value; uint length; if(!StatisticsOf(identity.Token,10,out value,(uint)Marshal.SizeOf(typeof(Statistics)),out length)) throw Error(); return String.Format("moon-v1-{0:x8}-{1:x8}",unchecked((uint)value.Authentication.High),value.Authentication.Low); } }
    public static uint TokenWord(int kind) { using(var identity=WindowsIdentity.GetCurrent()) { var memory=Marshal.AllocHGlobal(4); try {uint length; if(!GetTokenInformation(identity.Token,kind,memory,4,out length)) throw Error(); return unchecked((uint)Marshal.ReadInt32(memory));} finally {Marshal.FreeHGlobal(memory);} } }
    public static uint Integrity() { using(var identity=WindowsIdentity.GetCurrent()) {uint length;GetTokenInformation(identity.Token,25,IntPtr.Zero,0,out length);var memory=Marshal.AllocHGlobal((int)length);try {if(!GetTokenInformation(identity.Token,25,memory,length,out length)) throw Error();var sid=Marshal.ReadIntPtr(memory);var count=Marshal.ReadByte(GetSidSubAuthorityCount(sid));return unchecked((uint)Marshal.ReadInt32(GetSidSubAuthority(sid,(uint)(count-1))));}finally{Marshal.FreeHGlobal(memory);}} }
    public static void LowerToLowIntegrity() {IntPtr token,sid;if(!OpenProcessToken(GetCurrentProcess(),0x88,out token))throw Error();try{if(!ConvertStringSidToSid("S-1-16-4096",out sid))throw Error();try{var label=new Label{Sid=sid,Attributes=0x20};if(!SetTokenInformation(token,25,ref label,(uint)Marshal.SizeOf(typeof(Label))+GetLengthSid(sid)))throw Error();}finally{LocalFree(sid);}}finally{CloseHandle(token);}}
    public static string Manifest(string path) {var module=LoadLibraryEx(path,IntPtr.Zero,0x22);if(module==IntPtr.Zero)throw Error();try{var resource=FindResource(module,new IntPtr(1),new IntPtr(24));if(resource==IntPtr.Zero)throw Error();var loaded=LoadResource(module,resource);if(loaded==IntPtr.Zero)throw Error();uint length=SizeofResource(module,resource);if(length==0||length>1048576)throw new InvalidOperationException("Invalid manifest size");var bytes=new byte[length];Marshal.Copy(LockResource(loaded),bytes,0,(int)length);return Encoding.UTF8.GetString(bytes).TrimEnd('\0');}finally{FreeLibrary(module);}}
    public static void EnvironmentChanged() {UIntPtr result;SendMessageTimeout(new IntPtr(0xffff),0x1a,IntPtr.Zero,"Environment",2,200,out result);}
}
'@
}

function Get-MoonInstallDirectory { Join-Path $env:ProgramW6432 'moon' }
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
function Invoke-MoonRpc {
    param([ValidateSet('query','command')][string]$Method,[string]$Query,[string[]]$Commands,[long]$ExpectedFocus)
    $pipe = [IO.Pipes.NamedPipeClientStream]::new('.', [MoonInstallNative]::PipeName(), [IO.Pipes.PipeDirection]::InOut, [IO.Pipes.PipeOptions]::Asynchronous)
    try {
        $pipe.Connect(1500)
        $encoding = [Text.UTF8Encoding]::new($false,$true)
        $reader = [IO.StreamReader]::new($pipe,$encoding,$false,4096,$true)
        $writer = [IO.StreamWriter]::new($pipe,$encoding,4096,$true)
        try {
            $request = @{version=1;id='installation';method=$Method}
            if ($Method -eq 'query') { $request.query=$Query } else { $request.commands=$Commands }
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
            return $response.result
        } finally { $reader.Dispose(); try { $writer.Dispose() } catch [IO.IOException] {} }
    } finally { $pipe.Dispose() }
}
function Get-MoonDaemon { try { Invoke-MoonRpc query stats } catch { return $null } }
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
    return $state
}
function Write-MoonJsonFile([string]$Path,$Value) {
    $directory=[IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($Path))
    $temporary=Join-Path $directory ('moon-write.'+[guid]::NewGuid().ToString('N')+'.tmp')
    [IO.File]::WriteAllText($temporary,($Value | ConvertTo-Json -Depth 20),[Text.UTF8Encoding]::new($false))
    # The directory is Administrators-owned in a real install. Set the same
    # owner explicitly so a creator's token default cannot leave user-owned
    # security metadata with the owner's implicit permission to change its ACL.
    $security=Get-Acl -LiteralPath $temporary
    $security.SetOwner((Get-Acl -LiteralPath $directory).GetOwner([Security.Principal.SecurityIdentifier]))
    Set-Acl -LiteralPath $temporary -AclObject $security
    if (Test-Path -LiteralPath $Path) { [IO.File]::Replace($temporary,$Path,[NullString]::Value) } else { [IO.File]::Move($temporary,$Path) }
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
    $expected="Start-Process -FilePath '$($Target.Replace("'","''"))' -ArgumentList 'start --background' -WindowStyle Hidden"
    if ($launch -ne $expected) { throw 'The startup task points outside the expected moon launch operation.' }
    $principal=$Task.Principal.UserId
    if ($principal -notmatch '^S-1-') { $principal=([Security.Principal.NTAccount]::new($principal)).Translate([Security.Principal.SecurityIdentifier]).Value }
    if ($principal -ne $State.owner_sid) { throw 'Unexpected startup task user.' }
}
function Get-MoonLayoutFingerprint {
    $placements=@(Invoke-MoonRpc query windows)
    (@($placements | Sort-Object id | ForEach-Object { '{0}:{1}:{2},{3},{4},{5}' -f $_.id,$_.visible,$_.rect.x,$_.rect.y,$_.rect.width,$_.rect.height }) -join '|')
}

function New-MoonTaskAction([string]$Target,[string]$Arguments='start --background') {
    $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $launch="Start-Process -FilePath '$($Target.Replace("'","''"))' -ArgumentList '$($Arguments.Replace("'","''"))' -WindowStyle Hidden"
    $encoded=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($launch))
    New-ScheduledTaskAction -Execute $powershell -Argument ('-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand '+$encoded)
}
function Start-MoonLimitedOnce([string]$Target,[string]$Arguments,$State) {
    $name='moon-install-'+[guid]::NewGuid().ToString('N')
    $principal=New-ScheduledTaskPrincipal -UserId $State.owner_sid -LogonType Interactive -RunLevel Limited
    $settings=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::FromMinutes(1))
    Register-ScheduledTask -TaskName $name -Description "moon temporary installation $($State.install_id)" -Action (New-MoonTaskAction $Target $Arguments) -Principal $principal -Settings $settings | Out-Null
    try {
        Start-ScheduledTask -TaskName $name
        Wait-MoonCondition -Description 'the restored signed daemon' -Condition {
            $running=Get-MoonDaemon
            $running -and [string]::Equals($running.executable,$Target,[StringComparison]::OrdinalIgnoreCase) -and $running.token.ui_access
        }
    } finally {Unregister-ScheduledTask -TaskName $name -Confirm:$false}
}
