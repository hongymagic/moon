# A standalone extension using only moon's public named-pipe protocol.
[CmdletBinding()]
param([switch]$Once, [string]$PipeName)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (!$PipeName) {
    if (!('MoonExtensionIdentity' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class MoonExtensionIdentity {
    [StructLayout(LayoutKind.Sequential)] struct Label { public IntPtr Sid; public uint Attributes; }
    [StructLayout(LayoutKind.Sequential)] struct LogonGroups { public uint Count; public Label Group; }
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr token,int kind,IntPtr value,uint length,out uint returned);
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
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try { $PipeName = [MoonExtensionIdentity]::PipeName($identity.Token) } finally { $identity.Dispose() }
}
$PipeName = $PipeName -replace '^\\\\\.\\pipe\\', ''
$pipe = New-Object IO.Pipes.NamedPipeClientStream('.', $PipeName, [IO.Pipes.PipeDirection]::InOut, [IO.Pipes.PipeOptions]::Asynchronous)
try {
    $pipe.Connect(5000)
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
