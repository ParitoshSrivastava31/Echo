; Inno Setup Script for Echo - Offline Voice Keyboard for Windows
#define MyAppName "Echo"
#define MyAppVersion "0.2.0"
#define MyAppPublisher "Echo Team"
#define MyAppURL "https://github.com/ParitoshSrivastava31/Echo"
#define MyAppExeName "echo.exe"

[Setup]
AppId={{E68A9D21-F94A-4A73-B5B2-7DF0B06637D4}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
OutputDir=..\dist
OutputBaseFilename=Echo-Setup-v{#MyAppVersion}
SetupIconFile=..\echo.ico
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\{#MyAppExeName}
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "autostart"; Description: "Launch Echo automatically when Windows starts"; GroupDescription: "Windows Integration:"

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\echo.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\models\onnxruntime.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\models\asr4all-s\*"; DestDir: "{app}\models\asr4all-s"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\models\jfk.wav"; DestDir: "{app}\models"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\echo.ico"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\echo.ico"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "EchoVoiceKeyboard"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: files; Name: "{app}\echo.log"
