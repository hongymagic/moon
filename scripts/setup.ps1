[CmdletBinding()]
param([ValidateSet('Install','Uninstall')][string]$Mode='Install',[string]$Binary,[string]$SettingsBinary,[switch]$CompileOnly)
. (Join-Path $PSScriptRoot 'install-common.ps1')
Add-Type -AssemblyName PresentationFramework,PresentationCore,WindowsBase
$xaml=@'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
 Title="Moon" Width="620" Height="480" MinWidth="540" MinHeight="420" MaxWidth="700" MaxHeight="620" WindowStartupLocation="CenterScreen" ResizeMode="NoResize"
 Background="#F7F5F1" Foreground="#292723" FontFamily="Segoe UI" FontSize="14">
 <Window.Resources>
  <Style TargetType="Button">
   <Setter Property="MinHeight" Value="42"/><Setter Property="MinWidth" Value="104"/><Setter Property="Padding" Value="18,8"/><Setter Property="Margin" Value="8,0,0,0"/><Setter Property="FontWeight" Value="SemiBold"/>
   <Setter Property="Background" Value="#EDE7DF"/><Setter Property="BorderBrush" Value="#D5CEC3"/><Setter Property="BorderThickness" Value="1"/>
   <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="Button"><Border Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}" BorderThickness="{TemplateBinding BorderThickness}" CornerRadius="8" Padding="{TemplateBinding Padding}"><ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center" RecognizesAccessKey="True"/></Border><ControlTemplate.Triggers><Trigger Property="IsMouseOver" Value="True"><Setter Property="Opacity" Value="0.92"/></Trigger><Trigger Property="IsPressed" Value="True"><Setter Property="Opacity" Value="0.8"/></Trigger><Trigger Property="IsEnabled" Value="False"><Setter Property="Opacity" Value="0.55"/></Trigger></ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter>
  </Style>
 </Window.Resources>
 <Grid Margin="36,30,36,28"><Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="*"/><RowDefinition Height="Auto"/></Grid.RowDefinitions>
  <StackPanel Orientation="Horizontal"><Image x:Name="ProductIcon" Width="36" Height="36" Margin="0,0,12,0" Visibility="Collapsed"/><TextBlock Text="Moon" FontSize="24" FontWeight="SemiBold" VerticalAlignment="Center"/></StackPanel>
  <ScrollViewer Grid.Row="1" VerticalScrollBarVisibility="Auto" Margin="0,28,0,24" Focusable="False">
   <StackPanel>
    <TextBlock x:Name="Heading" Text="Make room for what matters." FontSize="29" FontWeight="SemiBold" TextWrapping="Wrap"/>
    <TextBlock x:Name="Introduction" Margin="0,14,0,0" Text="A calmer desktop, with your windows arranged and your workspace close at hand." TextWrapping="Wrap" LineHeight="23"/>
    <TextBlock x:Name="Promise" Margin="0,16,0,0" Text="Starts with Windows. Keeps your personal settings." Foreground="#625B53" TextWrapping="Wrap"/>
    <Border x:Name="RuntimeNote" Margin="0,18,0,0" Padding="14" Background="#EFE6DA" CornerRadius="8" Visibility="Collapsed">
     <StackPanel><TextBlock Text="Settings needs Microsoft Edge WebView2." FontWeight="SemiBold" TextWrapping="Wrap"/><TextBlock Text="You can install Moon now. Add this Microsoft component before opening Settings." Margin="0,6,0,8" TextWrapping="Wrap"/><Button x:Name="RuntimeLink" Content="Get WebView2" HorizontalAlignment="Left" Margin="0" AutomationProperties.Name="Open Microsoft's WebView2 download page"/></StackPanel>
    </Border>
    <ProgressBar x:Name="Progress" Height="4" Margin="0,24,0,0" Visibility="Collapsed" Foreground="#9C5A3C" AutomationProperties.Name="Setup in progress"/>
    <TextBlock x:Name="Status" Margin="0,16,0,0" TextWrapping="Wrap" LineHeight="21" AutomationProperties.LiveSetting="Polite"/>
   </StackPanel>
  </ScrollViewer>
  <Grid Grid.Row="2"><Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="Auto"/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
   <Button x:Name="Details" Content="Details" HorizontalAlignment="Left" Margin="0" Visibility="Collapsed" AutomationProperties.Name="Open setup details"/>
   <Button x:Name="Secondary" Grid.Column="1" Content="Not now" IsCancel="True"/>
   <Button x:Name="Primary" Grid.Column="2" Content="Install Moon" IsDefault="True" Background="#9C5A3C" Foreground="White" BorderBrush="#9C5A3C"/>
  </Grid>
 </Grid>
</Window>
'@
$reader=[Xml.XmlReader]::Create([IO.StringReader]::new($xaml))
try {$window=[Windows.Markup.XamlReader]::Load($reader)} finally {$reader.Dispose()}
$ui=@{};foreach ($name in @('Heading','Introduction','Promise','RuntimeNote','RuntimeLink','ProductIcon','Progress','Status','Details','Primary','Secondary')) {$ui[$name]=$window.FindName($name)}
if ($CompileOnly) {return [pscustomobject]@{parsed=$true;controls=$ui.Keys.Count;mode=$Mode;webview2=(Get-MoonWebViewRuntime)}}
if ([Windows.SystemParameters]::HighContrast) {
    $window.Background=[Windows.SystemColors]::WindowBrush;$window.Foreground=[Windows.SystemColors]::WindowTextBrush
    $ui.Promise.Foreground=[Windows.SystemColors]::WindowTextBrush;$ui.RuntimeNote.Background=[Windows.SystemColors]::ControlBrush
    $ui.RuntimeNote.Foreground=[Windows.SystemColors]::ControlTextBrush
    foreach($name in @('Primary','Secondary','Details','RuntimeLink')){$ui[$name].Background=[Windows.SystemColors]::ControlBrush;$ui[$name].Foreground=[Windows.SystemColors]::ControlTextBrush;$ui[$name].BorderBrush=[Windows.SystemColors]::ControlDarkBrush}
    $ui.Primary.Background=[Windows.SystemColors]::HighlightBrush;$ui.Primary.Foreground=[Windows.SystemColors]::HighlightTextBrush
}
if (!$Binary) {$Binary=Join-Path $PSScriptRoot 'moon.exe'}
if (!$SettingsBinary) {$SettingsBinary=Join-Path ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($Binary))) 'moon-settings.exe'}
$installed=Test-Path -LiteralPath (Join-Path (Get-MoonInstallDirectory) 'installation.json')
$runtime=Get-MoonWebViewRuntime
if ($Mode -eq 'Uninstall') {
    $ui.Heading.Text='Remove Moon?';$ui.Introduction.Text='Your windows will be restored before Moon leaves. Your personal settings will stay on this PC.'
    $ui.Promise.Text='You can return to Moon whenever you are ready.';$ui.Primary.Content='Remove Moon';$ui.Secondary.Content='Keep Moon'
} elseif ($installed) {
    $ui.Heading.Text='A fresh start for Moon.';$ui.Introduction.Text='Update or repair Moon while keeping your windows and personal settings in place.'
    $sourceVersion=if(Test-Path -LiteralPath $Binary){[Diagnostics.FileVersionInfo]::GetVersionInfo($Binary).ProductVersion}else{$null}
    $currentVersion=[Diagnostics.FileVersionInfo]::GetVersionInfo((Join-Path (Get-MoonInstallDirectory) 'moon.exe')).ProductVersion
    $ui.Primary.Content=if($sourceVersion -and $sourceVersion -eq $currentVersion){'Repair Moon'}else{'Update Moon'}
}
if (!$runtime -and $Mode -eq 'Install') {$ui.RuntimeNote.Visibility='Visible'}
try {
    $iconPath=if(Test-Path -LiteralPath $Binary){$Binary}else{Join-Path (Get-MoonInstallDirectory) 'moon.exe'}
    Add-Type -AssemblyName System.Drawing
    $icon=[Drawing.Icon]::ExtractAssociatedIcon($iconPath)
    if ($icon) {try {$ui.ProductIcon.Source=[Windows.Interop.Imaging]::CreateBitmapSourceFromHIcon($icon.Handle,[Windows.Int32Rect]::Empty,[Windows.Media.Imaging.BitmapSizeOptions]::FromEmptyOptions());$window.Icon=$ui.ProductIcon.Source;$ui.ProductIcon.Visibility='Visible'}finally{$icon.Dispose()}}
} catch { }
$context=@{busy=$false;done=$false;process=$null;files=$null;operation=$null;detail=$null;status=$null;outcome=$null}
$timer=[Windows.Threading.DispatcherTimer]::new();$timer.Interval=[TimeSpan]::FromMilliseconds(350)
$ui.RuntimeLink.Add_Click({Start-Process 'https://developer.microsoft.com/microsoft-edge/webview2/consumer/'})
$ui.Details.Add_Click({if($context.detail -and (Test-Path -LiteralPath $context.detail)){Start-Process -FilePath notepad.exe -ArgumentList (ConvertTo-MoonArgument $context.detail)}})
$ui.Secondary.Add_Click({
    if (!$context.busy) {$window.Close();return}
    if ($context.status -in @('removing','recovering')) {return}
    [IO.File]::WriteAllText($context.files.cancel,'cancel')
    $ui.Status.Text='Canceling safely. Your previous setup will be preserved.';$ui.Secondary.IsEnabled=$false
})
$window.Add_Closing({param($sender,$eventArgs) if($context.busy){$eventArgs.Cancel=$true;$ui.Status.Text='Please wait while Moon finishes safely. This window will stay open until your desktop is ready.'}})
$ui.Primary.Add_Click({
    if ($context.done) {
        if ($Mode -eq 'Install') {
            if (!(Get-MoonWebViewRuntime)) {Start-Process 'https://developer.microsoft.com/microsoft-edge/webview2/consumer/';return}
            Start-Process -FilePath (Join-Path (Get-MoonInstallDirectory) 'moon-settings.exe')
        }
        $window.Close();return
    }
    try {
        if (Test-MoonAdministrator) {throw 'Open setup from your ordinary Windows account. Moon will ask Windows for approval when needed.'}
        $context.operation=[guid]::NewGuid().ToString('N');$context.files=Get-MoonSetupFiles $context.operation
        [void][IO.Directory]::CreateDirectory($context.files.directory)
        $context.detail=Join-Path $context.files.directory ($context.operation+'.log')
        $entry=if($Mode -eq 'Uninstall'){'uninstall.ps1'}elseif(!$installed -and (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'handover.ps1'))){'handover.ps1'}else{'install.ps1'}
        $arguments=@('-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot $entry),'-OperationId',$context.operation)
        if ($Mode -eq 'Install') {$arguments+=@('-Binary',$Binary,'-SettingsBinary',$SettingsBinary)}
        $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
        $context.process=Start-Process -FilePath $powershell -ArgumentList (($arguments|ForEach-Object {ConvertTo-MoonArgument $_}) -join ' ') -WindowStyle Hidden -RedirectStandardOutput ($context.detail+'.out') -RedirectStandardError $context.detail -PassThru
        $context.busy=$true;$context.status='working';$ui.Primary.IsEnabled=$false;$ui.Secondary.Content='Cancel';$ui.Secondary.IsCancel=$false
        $ui.Progress.Visibility='Visible';$ui.Progress.IsIndeterminate=[Windows.SystemParameters]::ClientAreaAnimation
        $ui.Status.Text='Preparing Moon…';$timer.Start()
    } catch {$ui.Status.Text='Setup could not start. Open details, then try again.';$context.detail=Join-Path ([IO.Path]::GetTempPath()) ('moon-setup-error-'+[guid]::NewGuid().ToString('N')+'.log');[IO.File]::WriteAllText($context.detail,($_|Out-String));$ui.Details.Visibility='Visible'}
})
$timer.Add_Tick({
    try {
        try {
            $status=Read-MoonSetupStatus $context.operation
            if ($status) {
            if ($status.operation -eq $context.operation) {$context.status=$status.status;$context.outcome=$status;$ui.Status.Text=$status.message}
            if ($context.status -in @('removing','recovering')) {$ui.Secondary.IsEnabled=$false}
            }
        } catch {
            # Status may be unavailable, but the child's exit still determines
            # completion. Never keep the dialog busy forever on a read failure.
        }
        if (!$context.process.HasExited) {return}
        $code=$context.process.ExitCode;$context.process.Dispose();$context.process=$null;$timer.Stop();$context.busy=$false
        $ui.Progress.Visibility='Collapsed';$ui.Primary.IsEnabled=$true;$ui.Secondary.IsEnabled=$true;$ui.Secondary.Content='Close';$ui.Secondary.IsCancel=$true
        if ($code -eq 0 -and $context.status -eq 'succeeded') {
            $context.done=$true;$ui.Heading.Text=if($Mode -eq 'Install'){'Welcome to Moon.'}else{'Moon has been removed.'}
            $ui.Primary.Content=if($Mode -eq 'Uninstall'){'Done'}elseif(Get-MoonWebViewRuntime){'Open Settings'}else{'Get WebView2'}
        } elseif ($context.status -eq 'cancelled' -or $code -eq 1223) {
            $ui.Status.Text=if($Mode -eq 'Install'){'Setup was canceled. Your previous setup is unchanged.'}else{'Removal was canceled. Moon is still installed.'}
            $ui.Primary.Content='Try again'
        } else {
            if (!$context.outcome -or $context.status -in @('working','waiting','succeeded')) {$ui.Status.Text='Moon could not finish. Open details for the next step.'}
            if ($context.outcome -and $context.outcome.detail) {[IO.File]::AppendAllText($context.detail,([Environment]::NewLine+$context.outcome.detail))}
            $ui.Details.Visibility='Visible';$ui.Primary.Content='Try again'
            if($context.status -eq 'recovery_required'){$ui.Primary.IsEnabled=$false}
        }
    } catch {$ui.Status.Text='Waiting for setup to finish safely…'}
})
$window.Add_Loaded({[void]$ui.Primary.Focus()})
try {[void]$window.ShowDialog()}finally{$timer.Stop();if($context.process -and !$context.busy){$context.process.Dispose()}}
