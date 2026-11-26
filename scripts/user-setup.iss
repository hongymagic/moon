#if Ver < EncodeVer(7,1,1) || Ver >= EncodeVer(7,1,2)
  #error Build Moon Setup with Inno Setup 7.1.1
#endif

[Setup]
AppId=MoonWM.User
AppName=Moon
AppVersion={#Version}
AppPublisher=Moon
AppPublisherURL=https://moon.hongy.dev
VersionInfoVersion={#NumericVersion}
VersionInfoProductTextVersion={#Version}
VersionInfoDescription=Moon Setup
SetupIconFile={#Payload}\moon.ico
UninstallDisplayIcon={app}\moon-settings.exe
DefaultDirName={userpf}\Moon
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
MinVersion=10.0.22000
SetupArchitecture=x64
ArchitecturesAllowed={#Architecture}
ArchitecturesInstallIn64BitMode={#Architecture}
OutputBaseFilename={#OutputName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ChangesEnvironment=yes
SetupMutex=MoonWM.User.Setup
CloseApplications=no
RestartApplications=no
#ifdef PublishSigned
SignTool=moon
SignedUninstaller=yes
#define SignFiles "sign"
#else
#define SignFiles ""
#endif

[Files]
Source: "{#Payload}\moon.exe"; DestDir: "{app}"; Flags: ignoreversion {#SignFiles}
Source: "{#Payload}\moon-settings.exe"; DestDir: "{app}"; Flags: ignoreversion {#SignFiles}
Source: "{#Payload}\LICENSE"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "{#Payload}\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Moon"; Filename: "{app}\moon-settings.exe"; Parameters: "--open-moon"; WorkingDir: "{app}"

[Registry]
Root: HKCU; Subkey: "Software\MoonWM\UserInstall"; ValueType: string; ValueName: "Directory"; ValueData: "{app}"; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Moon Window Manager"; ValueData: """{app}\moon-settings.exe"" --start-moon"; Check: FirstInstall; Flags: createvalueifdoesntexist
Root: HKCU; Subkey: "Software\MoonWM\UserInstall"; ValueType: dword; ValueName: "PathAdded"; ValueData: "1"; Check: AddToPath; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\MoonWM\UserInstall"; ValueType: dword; ValueName: "PathCreated"; ValueData: "1"; Check: CreatePath; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{code:UpdatedPath}"; Check: AddToPath; Flags: preservestringtype

[Run]
Filename: "{app}\moon-settings.exe"; Parameters: "--open-moon"; Description: "Open Moon"; Flags: postinstall nowait skipifsilent; Check: HasWebView
Filename: "{app}\moon-settings.exe"; Parameters: "--start-moon"; Description: "Start Moon"; Flags: postinstall nowait skipifsilent; Check: not HasWebView
Filename: "https://developer.microsoft.com/microsoft-edge/webview2/consumer/"; Description: "Get Microsoft WebView2 for Moon Settings"; Flags: postinstall shellexec skipifsilent unchecked; Check: not HasWebView

[Code]
const
  InstallKey = 'Software\MoonWM\UserInstall';
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  RunName = 'Moon Window Manager';
  WebViewKey = 'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
var
  NewInstall, AppendPath, MissingPath: Boolean;

function HasWebView: Boolean;
var Version: String;
begin
  Result := (RegQueryStringValue(HKLM32, WebViewKey, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')) or
    (RegQueryStringValue(HKCU, WebViewKey, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0'));
end;

function FirstInstall: Boolean;
begin
  Result := NewInstall;
end;

function AddToPath: Boolean;
begin
  Result := AppendPath;
end;

function CreatePath: Boolean;
begin
  Result := MissingPath;
end;

function UpdatedPath(Param: String): String;
begin
  if RegQueryStringValue(HKCU, 'Environment', 'Path', Result) and (Result <> '') then
    Result := Result + ';';
  Result := Result + ExpandConstant('{app}');
end;

function PathIndex(const Parts: TArrayOfString): Integer;
var I: Integer;
begin
  for I := GetArrayLength(Parts) - 1 downto 0 do
    if CompareText(RemoveBackslashUnlessRoot(RemoveQuotes(Trim(Parts[I]))), ExpandConstant('{app}')) = 0 then begin
      Result := I;
      Exit;
    end;
  Result := -1;
end;

function ProcessIdToSessionId(Pid: LongWord; var Session: LongWord): Boolean;
  external 'ProcessIdToSessionId@kernel32.dll stdcall';
function GetCurrentProcessId: LongWord;
  external 'GetCurrentProcessId@kernel32.dll stdcall';

function DesktopSession: LongWord;
var Session: LongWord;
begin
  if not ProcessIdToSessionId(GetCurrentProcessId, Session) then
    RaiseException('Windows could not identify the current desktop session.');
  Result := Session;
end;

function ImageInUse(const Name: String): Boolean;
var Stream: TFileStream;
begin
  Result := False;
  if not FileExists(Name) then Exit;
  try
    Stream := TFileStream.Create(Name, fmOpenReadWrite or fmShareDenyNone);
    Stream.Free;
  except
    Result := True;
  end;
end;

function CloseAppsMessage: String;
begin
  Result := '';
  if CheckForMutexes('Local\moon-v1-' + IntToStr(DesktopSession)) or
     ImageInUse(ExpandConstant('{app}\moon.exe')) then
    Result := 'Choose Exit from the Moon tray menu so Moon can restore your windows, then retry.'
  else if ImageInUse(ExpandConstant('{app}\moon-settings.exe')) then
    Result := 'Close Moon Settings, then retry.';
end;

function OwnsProtectedInstall: Boolean;
var Keys: TArrayOfString; I: Integer; Location, Id, Base: String;
begin
  Result := False;
  if not FileExists(ExpandConstant('{commonpf64}\moon\installation.json')) then Exit;
  Base := 'Software\Microsoft\Windows\CurrentVersion\Uninstall';
  if not RegGetSubkeyNames(HKCU, Base, Keys) then Exit;
  for I := 0 to GetArrayLength(Keys) - 1 do
    if RegQueryStringValue(HKCU, Base + '\' + Keys[I], 'MoonInstallId', Id) and
       (CompareText(Keys[I], 'moon-' + Id) = 0) and
       RegQueryStringValue(HKCU, Base + '\' + Keys[I], 'InstallLocation', Location) and
       (CompareText(Location, ExpandConstant('{commonpf64}\moon')) = 0) then begin
      Result := True;
      Exit;
    end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var Current, PreviousDir: String;
begin
  if OwnsProtectedInstall then begin
    Result := 'Moon is already installed under Program Files. To switch to the per-user edition, remove Moon in Settings > Apps > Installed apps, then retry. Your personal configuration is kept. To update your existing edition, use the UIAccess download.';
    Exit;
  end;
  Result := CloseAppsMessage;
  if Result <> '' then Exit;
  NewInstall := not RegQueryStringValue(HKCU, InstallKey, 'Directory', PreviousDir);
  if not NewInstall and (CompareText(PreviousDir, ExpandConstant('{app}')) <> 0) then begin
    Result := 'Use the existing Moon installation directory to update this account.';
    Exit;
  end;
  if NewInstall and (FileExists(ExpandConstant('{app}\moon.exe')) or
     FileExists(ExpandConstant('{app}\moon-settings.exe')) or FileExists(ExpandConstant('{userprograms}\Moon.lnk'))) then begin
    Result := 'A Moon executable or Start menu shortcut already occupies this location. Move the existing copy before installing.';
    Exit;
  end;
  MissingPath := not RegValueExists(HKCU, 'Environment', 'Path');
  if not MissingPath and not RegQueryStringValue(HKCU, 'Environment', 'Path', Current) then begin
    Result := 'Windows could not read your user PATH.';
    Exit;
  end;
  AppendPath := PathIndex(StringSplit(Current, [';'], stAll)) < 0;
  Result := '';
end;

function InitializeUninstall: Boolean;
var Message: String;
begin
  Message := CloseAppsMessage;
  if (Message = '') and FileExists(ExpandConstant('{localappdata}\moon\journal-v1-session') + IntToStr(DesktopSession) + '.json') then
    Message := 'Moon has a saved window recovery record. Open Moon from Start, then choose Exit from its tray menu before removing it.';
  Result := Message = '';
  if not Result then
    SuppressibleMsgBox(Message, mbInformation, MB_OK, IDOK);
end;

function RegGetValue(Root: NativeInt; Key, Name: String; Flags: LongWord; var Kind: LongWord; Data, Size: NativeInt): LongInt;
  external 'RegGetValueW@advapi32.dll stdcall';

procedure CurUninstallStepChanged(Step: TUninstallStep);
var Value, Updated: String; Parts: TArrayOfString; Added, Created, Kind: LongWord; I, Index: Integer;
begin
  if Step <> usUninstall then Exit;
  if RegQueryStringValue(HKCU, RunKey, RunName, Value) and
     (CompareText(Value, '"' + ExpandConstant('{app}\moon-settings.exe') + '" --start-moon') = 0) then
    RegDeleteValue(HKCU, RunKey, RunName);
  if not RegQueryDWordValue(HKCU, InstallKey, 'PathAdded', Added) or (Added <> 1) then Exit;
  if not RegQueryStringValue(HKCU, 'Environment', 'Path', Value) then Exit;
  Parts := StringSplit(Value, [';'], stAll);
  Index := PathIndex(Parts);
  if Index < 0 then Exit;
  for I := Index to GetArrayLength(Parts) - 2 do Parts[I] := Parts[I + 1];
  SetArrayLength(Parts, GetArrayLength(Parts) - 1);
  Updated := StringJoin(';', Parts);
  if (Updated = '') and RegQueryDWordValue(HKCU, InstallKey, 'PathCreated', Created) and (Created = 1) then begin
    RegDeleteValue(HKCU, 'Environment', 'Path');
    Exit;
  end;
  { Query the current type without expanding any other program's %variables%. }
  if RegGetValue(-2147483647, 'Environment', 'Path', $10000006, Kind, 0, 0) = 0 then begin
    if Kind = 2 then RegWriteExpandStringValue(HKCU, 'Environment', 'Path', Updated)
    else RegWriteStringValue(HKCU, 'Environment', 'Path', Updated);
  end;
end;
