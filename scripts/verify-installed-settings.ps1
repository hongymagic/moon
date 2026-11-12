# Read-only Settings verification against an already-running signed installation.
# Launches through Moon's real command path; never edits a field or saves a file.
[CmdletBinding()]
param([string]$ReportPath=(Join-Path $env:LOCALAPPDATA 'moon\installed-settings-results.json'))
. (Join-Path $PSScriptRoot 'install-common.ps1')
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
if(!('MoonInstalledSettingsNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class MoonInstalledSettingsNative {
    delegate bool EnumWindowProc(IntPtr window,IntPtr parameter);
    [StructLayout(LayoutKind.Sequential)] struct Frame {public int Left,Top,Right,Bottom;}
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowProc callback,IntPtr parameter);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window,out Frame frame);
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
}
'@
}

$failure=$null;$cleanupErrors=[Collections.Generic.List[string]]::new()
$checks=[Collections.Generic.List[string]]::new()
$daemon=$null;$settings=$null;$owned=$null;$configPath=$null;$configBefore=$null;$configAfter=$null
$daemonSignature=$null;$settingsSignature=$null
$settingsPath=Join-Path (Get-MoonInstallDirectory) 'moon-settings.exe'
$target=Join-Path (Get-MoonInstallDirectory) 'moon.exe'
$closed=$false;$readOnlyUi=$false;$session=[Diagnostics.Process]::GetCurrentProcess().SessionId

function Get-ConfigurationFingerprint([string]$Path) {
    if(!(Test-Path -LiteralPath $Path -PathType Leaf)){return [pscustomobject]@{exists=$false;sha256=$null}}
    [pscustomobject]@{exists=$true;sha256=(Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash}
}
function Get-OwnedSettingsWindow {
    if(!$owned -or !$settings){throw 'No Settings process is owned by this verification.'}
    $owned.Refresh();if($owned.HasExited){throw 'The owned Settings process exited before verification completed.'}
    $identity=[MoonInstallNative]::ProcessIdentity([uint32]$owned.Id)
    if($identity.Birth -ne $settings.birth -or ![string]::Equals($identity.Path,$settings.path,[StringComparison]::OrdinalIgnoreCase)){
        throw 'Settings process identity changed; no window action is allowed.'
    }
    $window=[MoonInstalledSettingsNative]::FindVisibleWindow([uint32]$owned.Id)
    if($window -eq [IntPtr]::Zero){throw 'Settings has no visible application window yet.'}
    [uint32]$windowOwner=0
    [void][MoonInstalledSettingsNative]::GetWindowThreadProcessId($window,[ref]$windowOwner)
    if($windowOwner -ne $owned.Id){throw 'Settings HWND owner changed.'}
    $element=[Windows.Automation.AutomationElement]::FromHandle($window)
    if($element.Current.ProcessId -ne $owned.Id -or $element.Current.Name -ne 'Moon Settings'){throw 'Settings automation owner or title changed.'}
    return $element
}
function Find-SettingsControl($Window,[string]$Name) {
    $Window.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$Name))
}
function Assert-SettingsUnedited($Window) {
    $save=Find-SettingsControl $Window 'Save changes'
    if(!$save -or $save.Current.IsEnabled -or !(Find-SettingsControl $Window 'All changes saved')){
        throw 'Settings is not confirmed unedited; it will be left open without discarding anything.'
    }
}

try {
    if((Test-MoonAdministrator) -or [MoonInstallNative]::Integrity() -ne 8192 -or [MoonInstallNative]::TokenWord(26) -ne 0){
        throw 'Run this verification from ordinary, medium-integrity PowerShell without UIAccess.'
    }
    if(@(Get-Process -Name 'moon-settings' -ErrorAction SilentlyContinue).Count){throw 'Close existing Moon Settings windows before this verification; none were changed.'}
    $daemon=Invoke-MoonRpc query stats
    if(!$daemon.token.ui_access -or ![string]::Equals($daemon.executable,$target,[StringComparison]::OrdinalIgnoreCase)){
        throw 'Verification requires the running UIAccess daemon from the Program Files installation.'
    }
    $daemonSignature=(Get-AuthenticodeSignature -LiteralPath $target).Status.ToString()
    if($daemonSignature -ne 'Valid'){throw 'The installed daemon signature is not valid.'}
    $daemonIdentity=[MoonInstallNative]::ProcessIdentity([uint32]$daemon.process_id)
    if(![string]::Equals($daemonIdentity.Path,$target,[StringComparison]::OrdinalIgnoreCase)){throw 'The responding daemon process is not the installed image.'}
    if(!(Test-Path -LiteralPath $settingsPath -PathType Leaf)){throw 'The installed Settings companion is missing.'}
    $settingsSignature=(Get-AuthenticodeSignature -LiteralPath $settingsPath).Status.ToString()
    if($settingsSignature -ne 'Valid'){throw 'The installed Settings signature is not valid.'}
    $manifest=[xml][MoonInstallNative]::Manifest($settingsPath)
    $execution=$manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
    if(!$execution -or $execution.level -ne 'asInvoker' -or $execution.uiAccess -ne 'false'){throw 'Installed Settings must declare ordinary asInvoker permissions.'}
    $configPath=[string]$daemon.config_path
    if([string]::IsNullOrWhiteSpace($configPath) -or ![IO.Path]::IsPathRooted($configPath)){throw 'The daemon did not report an absolute configuration path.'}
    $configBefore=Get-ConfigurationFingerprint $configPath
    $checks.Add('An ordinary client reaches the signed installed UIAccess daemon')

    $launchedAt=[DateTime]::UtcNow.ToFileTimeUtc()
    $null=Invoke-MoonRpc command -Commands @('settings')
    $launchDeadline=[DateTime]::UtcNow.AddSeconds(20)
    do {
        $candidates=@(Get-Process -Name 'moon-settings' -ErrorAction SilentlyContinue | Where-Object {$_.SessionId -eq $session})
        try {
            if($candidates.Count -gt 1){throw 'Several Settings processes appeared; ownership is ambiguous.'}
            if($candidates.Count -eq 1){
                $candidate=$candidates[0]
                # Retain the native process handle before binding PID/path/birth.
                $null=$candidate.Handle
                $identity=[MoonInstallNative]::ProcessIdentity([uint32]$candidate.Id)
                if($identity.Birth -lt $launchedAt -or ![string]::Equals($identity.Path,$settingsPath,[StringComparison]::OrdinalIgnoreCase)){
                    throw 'The new Settings process does not match the installed launch.'
                }
                $settings=[pscustomobject]@{process_id=$candidate.Id;path=$identity.Path;birth=$identity.Birth;sha256=(Get-FileHash -LiteralPath $settingsPath -Algorithm SHA256).Hash;token=$null}
                $owned=$candidate
            }
        } finally {foreach($candidate in $candidates){if(!$owned -or $candidate -ne $owned){$candidate.Dispose()}}}
        if(!$owned){Start-Sleep -Milliseconds 150}
    } while(!$owned -and [DateTime]::UtcNow -lt $launchDeadline)
    if(!$owned){throw 'Timed out waiting for the Settings process launched by Moon.'}
    $settings.token=[MoonInstalledSettingsNative]::ReadToken($owned.Handle)
    if($settings.token.integrity_rid -ne 8192 -or $settings.token.ui_access -or $settings.token.elevated){throw 'Settings inherited UIAccess or elevated privileges.'}
    $checks.Add('The settings RPC launches the installed companion at medium integrity without elevation or UIAccess')

    Wait-MoonCondition -Description 'Settings reports Moon is running with an unedited configuration' -Seconds 20 -Condition {
        $window=Get-OwnedSettingsWindow
        if(!(Find-SettingsControl $window 'Moon is running')){return $false}
        Assert-SettingsUnedited $window
        return $true
    }
    $readOnlyUi=$true
    $checks.Add('Read-only UI Automation sees Moon is running and All changes saved')
    $after=Invoke-MoonRpc query stats
    if($after.process_id -ne $daemon.process_id -or ![string]::Equals($after.config_path,$configPath,[StringComparison]::OrdinalIgnoreCase)){
        throw 'The daemon or selected configuration changed during Settings verification.'
    }
} catch {$failure=$_}
finally {
    if($owned){
        try {
            $owned.Refresh()
            if(!$owned.HasExited){
                $window=Get-OwnedSettingsWindow
                Assert-SettingsUnedited $window
                if(![MoonInstalledSettingsNative]::PostMessage([IntPtr]$window.Current.NativeWindowHandle,0x10,[IntPtr]::Zero,[IntPtr]::Zero)){
                    throw 'Windows refused the owned Settings close message.'
                }
                if(!$owned.WaitForExit(5000)){throw 'Owned Settings did not close gracefully; no force or discard was used.'}
            }
            $closed=$true
        } catch {$cleanupErrors.Add($_.Exception.Message);if(!$failure){$failure=$_}}
        finally {$owned.Dispose()}
    }
    if($configBefore){
        try {
            $configAfter=Get-ConfigurationFingerprint $configPath
            if($configAfter.exists -ne $configBefore.exists -or $configAfter.sha256 -ne $configBefore.sha256){throw 'The configuration changed during the verification.'}
            $checks.Add('The personal configuration existence and SHA-256 remain unchanged')
        } catch {if(!$failure){$failure=$_}}
    }
}
$report=[pscustomobject]@{
    passed=($null -eq $failure);recorded_at=[DateTime]::UtcNow.ToString('o');launch='Moon settings RPC'
    daemon=$daemon;daemon_signature=$daemonSignature;settings=$settings;settings_signature=$settingsSignature
    config_path=$configPath;config_before=$configBefore;config_after=$configAfter
    read_only_ui=$readOnlyUi;owned_settings_closed=$closed;checks=$checks.ToArray();cleanup_errors=$cleanupErrors.ToArray()
    error=if($failure){$failure.ToString()}else{$null}
}
$json=$report|ConvertTo-Json -Depth 10
$json
try {
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReportPath)))
    [IO.File]::WriteAllText($ReportPath,$json,[Text.UTF8Encoding]::new($false))
} catch {
    if(!$failure){throw}
    Write-Warning "Could not write the report; the JSON above retains the original failure: $($_.Exception.Message)" -WarningAction Continue
}
if($failure){throw $failure}
