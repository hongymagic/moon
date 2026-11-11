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
    [StructLayout(LayoutKind.Sequential)] struct Luid { public uint Low; public int High; }
    [StructLayout(LayoutKind.Sequential)] struct Statistics { public Luid Token, Authentication; public long Expiration; public uint TokenType, ImpersonationLevel, DynamicCharged, DynamicAvailable, GroupCount, PrivilegeCount; public Luid Modified; }
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr token, int kind, out Statistics statistics, uint length, out uint returned);
    public static string PipeName(IntPtr token) { Statistics statistics; uint returned; if(!GetTokenInformation(token,10,out statistics,(uint)Marshal.SizeOf(typeof(Statistics)),out returned)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error()); return String.Format("moon-v1-{0:x8}-{1:x8}",unchecked((uint)statistics.Authentication.High),statistics.Authentication.Low); }
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
