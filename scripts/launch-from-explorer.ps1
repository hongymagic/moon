[CmdletBinding(DefaultParameterSetName='Launch')]
param(
    [Parameter(Mandatory=$true,ParameterSetName='Launch')][ValidateSet('Install','Handover')][string]$Mode,
    [Parameter(Mandatory=$true,ParameterSetName='Launch')][string]$Binary,
    [Parameter(ParameterSetName='Launch')][string]$SettingsBinary,
    [Parameter(ParameterSetName='Launch')][string]$PortableBinary,
    [Parameter(ParameterSetName='Launch')][switch]$KeepPreviousDesktopStopped,
    [Parameter(Mandatory=$true,ParameterSetName='Worker')][string]$WorkerDirectory
)
. (Join-Path $PSScriptRoot 'install-common.ps1')

function Initialize-MoonExplorerNative {
    if ('MoonExplorerLaunch' -as [type]) {return}
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Security.Principal;
public static class MoonExplorerLaunch {
    [DllImport("user32.dll")] public static extern IntPtr GetShellWindow();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
    [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,uint process);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr process,uint access,out IntPtr token);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr token,int kind,IntPtr data,uint length,out uint returned);
    static Exception Error(){return new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());}
    public static uint WindowProcess(IntPtr window){uint process;GetWindowThreadProcessId(window,out process);return process;}
    static uint Word(IntPtr token,int kind){var data=Marshal.AllocHGlobal(4);try{uint length;if(!GetTokenInformation(token,kind,data,4,out length))throw Error();return unchecked((uint)Marshal.ReadInt32(data));}finally{Marshal.FreeHGlobal(data);}}
    public static void AssertOrdinary(uint processId,string owner,uint session) {
        var process=OpenProcess(0x1000,false,processId);if(process==IntPtr.Zero)throw Error();
        try {IntPtr token;if(!OpenProcessToken(process,8,out token))throw Error();try {
            using(var identity=new WindowsIdentity(token)){if(identity.User.Value!=owner)throw new InvalidOperationException("The Windows desktop belongs to another account.");}
            if(Word(token,12)!=session || Word(token,20)!=0 || Word(token,26)!=0)throw new InvalidOperationException("An ordinary process in the same Windows session is required.");
            uint length;GetTokenInformation(token,25,IntPtr.Zero,0,out length);var data=Marshal.AllocHGlobal((int)length);
            try {if(!GetTokenInformation(token,25,data,length,out length))throw Error();if(new SecurityIdentifier(Marshal.ReadIntPtr(data)).Value!="S-1-16-8192")throw new InvalidOperationException("A normal Windows desktop is required.");}finally{Marshal.FreeHGlobal(data);}
        }finally{CloseHandle(token);}}finally{CloseHandle(process);}
    }
}
'@
}

function Write-MoonLaunchRecord([string]$Path,$Value) {
    $temporary=$Path+'.'+[guid]::NewGuid().ToString('N')+'.tmp'
    try {
        [IO.File]::WriteAllText($temporary,($Value|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
        [IO.File]::Move($temporary,$Path)
    } finally {if([IO.File]::Exists($temporary)){[IO.File]::Delete($temporary)}}
}
function Read-MoonLaunchRecord([string]$Path) {
    Assert-MoonNoReparse $Path
    if((Get-Item -LiteralPath $Path).Length -gt 1048576){throw 'The setup launch record is too large.'}
    [IO.File]::ReadAllText($Path)|ConvertFrom-Json
}
function Invoke-MoonExplorerWorker([string]$Directory) {
    Initialize-MoonExplorerNative
    Assert-MoonNoReparse $Directory
    $request=Read-MoonLaunchRecord (Join-Path $Directory 'request.json')
    $owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $session=[Diagnostics.Process]::GetCurrentProcess().SessionId
    [MoonExplorerLaunch]::AssertOrdinary([uint32]$PID,$owner,[uint32]$session)
    if($request.version -ne 1 -or $request.owner_sid -ne $owner -or $request.session -ne $session -or
       $request.nonce -notmatch '^[a-f0-9]{32}$' -or (Split-Path -Leaf $Directory) -cne ('.moon-launch-'+$request.nonce)) {throw 'Setup launch ownership does not match this desktop.'}
    $identity=[MoonInstallNative]::ProcessIdentity([uint32]$PID)
    Write-MoonLaunchRecord (Join-Path $Directory 'ready.json') @{nonce=$request.nonce;process_id=$PID;birth=$identity.Birth;path=$identity.Path}
    $deadline=[DateTime]::Parse($request.start_deadline).ToUniversalTime()
    $goPath=Join-Path $Directory 'go.json'
    while(!(Test-Path -LiteralPath $goPath)) {
        if([DateTime]::UtcNow -ge $deadline){return 1}
        Start-Sleep -Milliseconds 100
    }
    $go=Read-MoonLaunchRecord $goPath
    if($go.nonce -cne $request.nonce -or [DateTime]::Parse($go.approved_at).ToUniversalTime() -gt $deadline){throw 'Setup launch authorization expired.'}
    $exitCode=1
    try {
        $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
        # Once approved, a UAC prompt or the installation may take arbitrarily
        # long. Never impose a child timeout or terminate a setup process here.
        $result=Invoke-MoonNative $powershell (@('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',[string]$request.script)+[string[]]$request.arguments) -TimeoutSeconds 0 -AllowFailure
        [IO.File]::WriteAllText((Join-Path $Directory 'stdout.txt'),$result.Output,[Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText((Join-Path $Directory 'stderr.txt'),$result.Error,[Text.UTF8Encoding]::new($false))
        $exitCode=$result.ExitCode
    } catch {
        [IO.File]::WriteAllText((Join-Path $Directory 'stderr.txt'),($_|Out-String),[Text.UTF8Encoding]::new($false))
    }
    Write-MoonLaunchRecord (Join-Path $Directory 'result.json') @{version=1;nonce=$request.nonce;exit_code=$exitCode;finished_at=[DateTime]::UtcNow.ToString('o')}
    return $exitCode
}

function Invoke-MoonExplorerScript {
    param([string]$Script,[string[]]$Arguments=@(),[int]$StartupSeconds=30)
    Initialize-MoonExplorerNative
    $owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $session=[Diagnostics.Process]::GetCurrentProcess().SessionId
    [MoonExplorerLaunch]::AssertOrdinary([uint32]$PID,$owner,[uint32]$session)
    $desktop=[MoonExplorerLaunch]::GetShellWindow()
    if($desktop -eq [IntPtr]::Zero){throw 'The Windows desktop is not available. Sign in and try again.'}
    $explorerId=[MoonExplorerLaunch]::WindowProcess($desktop)
    [MoonExplorerLaunch]::AssertOrdinary($explorerId,$owner,[uint32]$session)
    $explorer=[MoonInstallNative]::ProcessIdentity($explorerId)
    if(![string]::Equals($explorer.Path,(Join-Path $env:SystemRoot 'explorer.exe'),[StringComparison]::OrdinalIgnoreCase)){throw 'The active desktop is not Windows Explorer.'}
    $profileRoot=[Environment]::GetFolderPath('UserProfile');Assert-MoonNoReparse $profileRoot
    $nonce=[guid]::NewGuid().ToString('N');$directory=Join-Path $profileRoot ('.moon-launch-'+$nonce)
    $acl=[Security.AccessControl.DirectorySecurity]::new();$acl.SetAccessRuleProtection($true,$false)
    $acl.SetOwner([Security.Principal.SecurityIdentifier]::new($owner))
    foreach($sid in @($owner,'S-1-5-18')) {
        $rule=[Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid),[Security.AccessControl.FileSystemRights]::FullControl,([Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'),[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow)
        [void]$acl.AddAccessRule($rule)
    }
    if(Test-Path -LiteralPath $directory){throw 'The temporary setup directory already exists.'}
    [void][IO.Directory]::CreateDirectory($directory,$acl)
    $worker=$null;$shell=$null;$windows=$null;$view=$null;$application=$null;$complete=$false
    try {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'launch-from-explorer.ps1') -Destination (Join-Path $directory 'worker.ps1')
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'install-common.ps1') -Destination (Join-Path $directory 'install-common.ps1')
        $deadline=[DateTime]::UtcNow.AddSeconds($StartupSeconds)
        Write-MoonLaunchRecord (Join-Path $directory 'request.json') @{version=1;nonce=$nonce;owner_sid=$owner;session=$session;script=$Script;arguments=@($Arguments);start_deadline=$deadline.ToString('o')}
        $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
        $parameters=(@('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File',(Join-Path $directory 'worker.ps1'),'-WorkerDirectory',$directory)|ForEach-Object {ConvertTo-MoonArgument $_}) -join ' '
        # Obtain the running desktop automation object, not ShellExecute on a
        # newly created shell object in the caller's inherited file context.
        $shell=New-Object -ComObject Shell.Application;$windows=$shell.Windows()
        $handle=0;$view=$windows.FindWindowSW(0,0,8,[ref]$handle,1)
        if(!$view -or [MoonExplorerLaunch]::WindowProcess([IntPtr]$handle) -ne $explorerId -or [MoonExplorerLaunch]::GetShellWindow() -ne $desktop){throw 'The Windows desktop changed. Try again.'}
        $application=$view.Document.Application
        [Console]::Error.WriteLine('Starting Moon setup through the Windows desktop. Diagnostics: '+$directory)
        [void]$application.ShellExecute($powershell,$parameters,$directory,'open',0)
        $readyPath=Join-Path $directory 'ready.json'
        while(!(Test-Path -LiteralPath $readyPath)) {
            if([DateTime]::UtcNow -ge $deadline){throw 'The Windows desktop did not start the setup worker. No setup was authorized.'}
            Start-Sleep -Milliseconds 100
        }
        $ready=Read-MoonLaunchRecord $readyPath
        if($ready.nonce -cne $nonce){throw 'Unexpected setup worker identity.'}
        $worker=[Diagnostics.Process]::GetProcessById([int]$ready.process_id)
        [void]$worker.Handle
        $identity=[MoonInstallNative]::ProcessIdentity([uint32]$worker.Id)
        [MoonExplorerLaunch]::AssertOrdinary([uint32]$worker.Id,$owner,[uint32]$session)
        if($identity.Birth -ne $ready.birth -or ![string]::Equals($identity.Path,$powershell,[StringComparison]::OrdinalIgnoreCase) -or [DateTime]::UtcNow -ge $deadline){throw 'The setup worker changed or its startup authorization expired.'}
        Write-MoonLaunchRecord (Join-Path $directory 'go.json') @{nonce=$nonce;approved_at=[DateTime]::UtcNow.ToString('o')}
        $worker.WaitForExit()
        $result=Read-MoonLaunchRecord (Join-Path $directory 'result.json')
        if($result.version -ne 1 -or $result.nonce -cne $nonce -or $result.exit_code -ne $worker.ExitCode){throw 'The setup worker did not publish a matching completion result.'}
        foreach($stream in @('stdout','stderr')) {
            $path=Join-Path $directory ($stream+'.txt')
            if(Test-Path -LiteralPath $path){$text=[IO.File]::ReadAllText($path);if($stream -eq 'stdout'){[Console]::Out.Write($text)}else{[Console]::Error.Write($text)}}
        }
        $complete=$result.exit_code -eq 0
        return [int]$result.exit_code
    } finally {
        if($worker){$worker.Dispose()}
        foreach($reference in @($application,$view,$windows,$shell)){if($reference -and [Runtime.InteropServices.Marshal]::IsComObject($reference)){try{[void][Runtime.InteropServices.Marshal]::ReleaseComObject($reference)}catch{ }}}
        if($complete){try{Remove-Item -LiteralPath $directory -Recurse -Force}catch{[Console]::Error.WriteLine('Setup completed. Temporary launch files remain at '+$directory)}}
        else {[Console]::Error.WriteLine('Setup diagnostics were retained at '+$directory+'. An approved setup is allowed to finish even if this launcher closes.')}
    }
}

try {
    if($PSCmdlet.ParameterSetName -eq 'Worker'){exit (Invoke-MoonExplorerWorker $WorkerDirectory)}
    if($KeepPreviousDesktopStopped -and $Mode -ne 'Handover'){throw 'The desktop recovery preference applies only to handover.'}
    $arguments=@('-Binary',(Resolve-Path -LiteralPath $Binary).ProviderPath)
    if($SettingsBinary){$arguments+=@('-SettingsBinary',(Resolve-Path -LiteralPath $SettingsBinary).ProviderPath)}
    if($PortableBinary){$arguments+=@('-PortableBinary',(Resolve-Path -LiteralPath $PortableBinary).ProviderPath)}
    if($KeepPreviousDesktopStopped){$arguments+='-KeepPreviousDesktopStopped'}
    exit (Invoke-MoonExplorerScript (Join-Path $PSScriptRoot ($Mode.ToLowerInvariant()+'.ps1')) $arguments)
} catch {
    $failure=$_
    if($PSCmdlet.ParameterSetName -eq 'Worker') {
        try {
            Assert-MoonNoReparse $WorkerDirectory
            if((Split-Path -Parent $WorkerDirectory) -eq [Environment]::GetFolderPath('UserProfile') -and (Split-Path -Leaf $WorkerDirectory) -match '^\.moon-launch-[a-f0-9]{32}$') {
                [IO.File]::WriteAllText((Join-Path $WorkerDirectory 'worker-error.txt'),($failure|Out-String),[Text.UTF8Encoding]::new($false))
            }
        } catch { }
    }
    Write-Error $failure -ErrorAction Continue;exit 1
}
