# Foreground permission is granted by the process that owns the test window.
if (!('MoonTestForeground' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class MoonTestForeground {
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
    [DllImport("user32.dll",EntryPoint="SendMessageTimeoutW")] static extern IntPtr SendMessageTimeout(IntPtr window,uint message,UIntPtr wparam,IntPtr lparam,uint flags,uint timeout,out UIntPtr result);
    public static bool Request(long window,uint owner,uint message,uint value) {
        var handle=new IntPtr(window);
        uint process;
        if(GetWindowThreadProcessId(handle,out process)==0 || process!=owner)
            throw new InvalidOperationException("The foreground request did not identify an owned test window.");
        UIntPtr result;
        return SendMessageTimeout(handle,message,new UIntPtr(value),IntPtr.Zero,3,5000,out result)!=IntPtr.Zero && result!=UIntPtr.Zero;
    }
}
'@
}
function Try-Set-MoonTestForeground([long]$Window,[uint32]$HelperProcessId) {
    if (![MoonTestForeground]::Request($Window,$HelperProcessId,0x8060,0)) {return $false}
    $deadline=[DateTime]::UtcNow.AddSeconds(5)
    do {
        if ([MoonTestForeground]::GetForegroundWindow().ToInt64() -eq $Window) {return $true}
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    return $false
}
function Grant-MoonTestForeground([uint32]$HelperProcessId,[uint32]$ProcessId) {
    $foreground=[MoonTestForeground]::GetForegroundWindow().ToInt64()
    if (![MoonTestForeground]::Request($foreground,$HelperProcessId,0x8061,$ProcessId)) {throw 'The foreground test window could not grant activation to the requested process.'}
}
