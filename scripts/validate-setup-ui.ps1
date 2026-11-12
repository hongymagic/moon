[CmdletBinding()]
param([string]$OutputDirectory,[string]$Binary)
. (Join-Path $PSScriptRoot 'install-common.ps1')
if($Binary -and !(Test-Path -LiteralPath $Binary -PathType Leaf)){throw 'The selected setup preview executable does not exist.'}
if (!$OutputDirectory) {$OutputDirectory=Join-Path $env:LOCALAPPDATA 'moon-dev\setup-preview'}
[void][IO.Directory]::CreateDirectory($OutputDirectory)
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing,System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class MoonSetupPreview {
 [StructLayout(LayoutKind.Sequential)] public struct Rect {public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window,out Rect bounds);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window,IntPtr dc,uint flags);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window,uint message,IntPtr w,IntPtr l);
}
'@
$previous=[MoonSetupPreview]::GetForegroundWindow()
$checks=[Collections.Generic.List[object]]::new()
$powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
foreach ($mode in @('Install','Uninstall')) {
    $arguments=@('-NoProfile','-ExecutionPolicy','Bypass','-STA','-WindowStyle','Hidden','-File',(Join-Path $PSScriptRoot 'setup.ps1'),'-Mode',$mode)
    if($Binary){$arguments+=@('-Binary',$Binary)}
    $process=Start-Process -FilePath $powershell -ArgumentList (($arguments|ForEach-Object{ConvertTo-MoonArgument $_})-join ' ') -PassThru
    try {
        $until=[DateTime]::UtcNow.AddSeconds(10)
        do {$process.Refresh();if($process.HasExited){throw 'Setup preview closed before its window appeared.'};$handle=$process.MainWindowHandle;if($handle -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $until)
        if($handle -eq [IntPtr]::Zero){throw 'Setup preview window did not appear.'}
        $window=[Windows.Automation.AutomationElement]::FromHandle($handle)
        if($window.Current.ProcessId -ne $process.Id){throw 'Preview window owner changed.'}
        $buttonName=if($mode -eq 'Install'){'Install Moon'}else{'Remove Moon'}
        $primary=$window.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$buttonName))
        if(!$primary -or !$primary.Current.IsKeyboardFocusable){throw 'Primary setup action is not keyboard accessible.'}
        $primary.SetFocus();Start-Sleep -Milliseconds 150
        if([MoonSetupPreview]::GetForegroundWindow() -ne $handle){throw 'Preview does not own foreground; no test input was sent.'}
        $initial=[Windows.Automation.AutomationElement]::FocusedElement.Current.Name
        [Windows.Forms.SendKeys]::SendWait('{TAB}')
        Start-Sleep -Milliseconds 150
        $focused=[Windows.Automation.AutomationElement]::FocusedElement
        if($focused.Current.ProcessId -ne $process.Id){throw 'Tab moved focus outside the owned setup window.'}
        $bounds=[MoonSetupPreview+Rect]::new();[void][MoonSetupPreview]::GetWindowRect($handle,[ref]$bounds)
        $bitmap=[Drawing.Bitmap]::new($bounds.Right-$bounds.Left,$bounds.Bottom-$bounds.Top)
        $graphics=[Drawing.Graphics]::FromImage($bitmap)
        try {$dc=$graphics.GetHdc();try{if(![MoonSetupPreview]::PrintWindow($handle,$dc,2)){throw 'Owned setup screenshot failed.'}}finally{$graphics.ReleaseHdc($dc)};$image=Join-Path $OutputDirectory ($mode.ToLowerInvariant()+'.png');$bitmap.Save($image,[Drawing.Imaging.ImageFormat]::Png)}finally{$graphics.Dispose();$bitmap.Dispose()}
        $checks.Add([pscustomobject]@{mode=$mode;process_id=$process.Id;primary_accessible=$true;initial_focus=$initial;tab_focus=$focused.Current.Name;image=$image;install_or_remove_invoked=$false})
    } finally {
        if(!$process.HasExited){$process.Refresh();if($process.MainWindowHandle -ne [IntPtr]::Zero){[void][MoonSetupPreview]::PostMessage($process.MainWindowHandle,0x10,[IntPtr]::Zero,[IntPtr]::Zero)};if(!$process.WaitForExit(5000)){throw 'Owned setup preview did not close; it was not force terminated.'}}
        $process.Dispose()
    }
}
[void][MoonSetupPreview]::SetForegroundWindow($previous)
$report=[pscustomobject]@{passed=$true;checks=$checks.ToArray();uac_requested=$false;installation_changed=$false;binary_sha256=if($Binary){(Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash}else{$null}}
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'report.json'),($report|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
$report|ConvertTo-Json -Depth 5
