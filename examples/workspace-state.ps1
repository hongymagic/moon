# A standalone extension using only moon's public named-pipe protocol.
[CmdletBinding()]
param([switch]$Once, [string]$PipeName, [string]$DaemonPath)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (!('MoonExtensionIdentity' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class MoonExtensionIdentity {
    [StructLayout(LayoutKind.Sequential)] struct Label { public IntPtr Sid; public uint Attributes; }
    [StructLayout(LayoutKind.Sequential)] struct LogonGroups { public uint Count; public Label Group; }
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr token,int kind,IntPtr value,uint length,out uint returned);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetNamedPipeServerProcessId(Microsoft.Win32.SafeHandles.SafePipeHandle pipe,out uint processId);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetNamedPipeInfo(Microsoft.Win32.SafeHandles.SafePipeHandle pipe,IntPtr flags,IntPtr outbound,IntPtr inbound,out uint maximumInstances);
    [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,uint processId);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool QueryFullProcessImageName(IntPtr process,uint flags,StringBuilder path,ref uint length);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr process);
    public static string ServerImage(Microsoft.Win32.SafeHandles.SafePipeHandle pipe) {
        uint maximum;if(!GetNamedPipeInfo(pipe,IntPtr.Zero,IntPtr.Zero,IntPtr.Zero,out maximum))throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        if(maximum!=1)throw new InvalidOperationException("Moon IPC requires one server instance per pipe name.");
        uint pid;if(!GetNamedPipeServerProcessId(pipe,out pid))throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        var process=OpenProcess(0x1000,false,pid);if(process==IntPtr.Zero)throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        try {
            var path=new StringBuilder(32768);uint length=(uint)path.Capacity;
            if(!QueryFullProcessImageName(process,0,path,ref length))throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
            return path.ToString();
        } finally {CloseHandle(process);}
    }
    public static string PipeName(IntPtr token) {
        uint length;GetTokenInformation(token,28,IntPtr.Zero,0,out length);
        if(length<Marshal.SizeOf(typeof(LogonGroups)) || length>65536)throw new InvalidOperationException("Invalid Windows logon identity size");
        var memory=Marshal.AllocHGlobal((int)length);
        try {
            if(!GetTokenInformation(token,28,memory,length,out length))throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
            var groups=(LogonGroups)Marshal.PtrToStructure(memory,typeof(LogonGroups));
            if(groups.Count!=1 || groups.Group.Sid==IntPtr.Zero)throw new InvalidOperationException("Expected one Windows logon SID");
            return "moon-v1-"+new System.Security.Principal.SecurityIdentifier(groups.Group.Sid).Value;
        } finally {Marshal.FreeHGlobal(memory);}
    }
}
'@
}
if (!$PipeName) {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try { $PipeName = [MoonExtensionIdentity]::PipeName($identity.Token) } finally { $identity.Dispose() }
}
$allowedImages=if($DaemonPath){@([IO.Path]::GetFullPath($DaemonPath))}else{@(
    (Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'moon\moon.exe'),
    (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'moon-dev\moon.exe')
)}
$PipeName = $PipeName -replace '^\\\\\.\\pipe\\', ''
$pipe=$null
$lastError=$null
$until=[DateTime]::UtcNow.AddSeconds(5)
do {
    for($slot=0;$slot -lt 16;$slot++) {
        $endpoint=if($slot){$PipeName+'-'+$slot}else{$PipeName}
        $candidate=[IO.Pipes.NamedPipeClientStream]::new('.',$endpoint,[IO.Pipes.PipeDirection]::InOut,[IO.Pipes.PipeOptions]::Asynchronous,[Security.Principal.TokenImpersonationLevel]::Identification)
        try {
            $candidate.Connect(25)
            $serverImage=[MoonExtensionIdentity]::ServerImage($candidate.SafePipeHandle)
            if ($allowedImages -inotcontains $serverImage) {throw 'The pipe does not belong to the expected Moon executable.'}
            $pipe=$candidate;break
        } catch {
            $candidate.Dispose()
            if ($_.Exception -isnot [TimeoutException]) {$lastError=$_}
        }
    }
} while (!$pipe -and [DateTime]::UtcNow -lt $until)
if(!$pipe){if($lastError){throw $lastError};throw 'Moon IPC connection timed out.'}
try {
    $encoding = New-Object Text.UTF8Encoding($false, $true)
    $reader = New-Object IO.StreamReader($pipe, $encoding, $false, 4096, $true)
    $writer = New-Object IO.StreamWriter($pipe, $encoding, 4096, $true)
    try {
        $writer.NewLine = "`n"
        $writer.AutoFlush = $true
        $request = @{version=1; id='workspace-state'; method='subscribe'}
        if ($Once) { $request.method='query'; $request.query='world' }
        $writer.WriteLine(($request | ConvertTo-Json -Compress))
        do {
            $line = $reader.ReadLine()
            if ($null -eq $line) { break }
            if ($line.Length -gt 8388608) { throw 'Response exceeds the public protocol limit.' }
            $response = $line | ConvertFrom-Json
            if (!$response.ok) { throw $response.error }
            $world = $response.result
            $workspaces = @($world.workspaces | ForEach-Object {
                $workspace = $_
                $count = @($world.windows.PSObject.Properties | Where-Object { $_.Value.workspace -eq $workspace.id }).Count
                [pscustomobject]@{ name=$workspace.name; monitor=$workspace.monitor; visible=(@($world.active.PSObject.Properties.Value) -contains $workspace.id); windows=$count }
            })
            [pscustomobject]@{ revision=$response.revision; workspaces=$workspaces; paused=$world.paused } | ConvertTo-Json -Depth 12 -Compress
        } while (!$Once)
    } finally { $reader.Dispose(); try { $writer.Dispose() } catch [IO.IOException] { } }
} finally { $pipe.Dispose() }
