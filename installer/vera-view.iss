; Inno Setup script for Vera View.
; Build with installer\build.ps1, which compiles the app and passes the version from Cargo.toml.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#define AppName "Vera View"
#define AppExe "vera-view.exe"
#define ProgId "VeraView.Drawing"

[Setup]
; AppId identifies the app for upgrades and uninstall. Never change it.
AppId={{76A0EEEE-6B7B-45A2-A9AD-F20B7E969FCD}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppName}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; Install for the current user without admin rights; the wizard offers "all users" too.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
ChangesAssociations=yes
SetupIconFile=..\assets\vera-view.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
OutputDir=..\dist
OutputBaseFilename=VeraView-Setup-{#AppVersion}
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}
VersionInfoDescription={#AppName} Setup

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "setdefault"; Description: "Make {#AppName} the default app for Visio drawings (.vsdx, .vsdm, .vstx, .vstm)"; GroupDescription: "File types:"; Flags: unchecked
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD-PARTY-NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; The document type Vera View opens.
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: ""; ValueData: "Visio Drawing"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\{#ProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExe},0"
Root: HKA; Subkey: "Software\Classes\{#ProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""

; "Open with" entries. These never replace an existing default such as Visio.
Root: HKA; Subkey: "Software\Classes\.vsdx\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.vsdm\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.vstx\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.vstm\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue

; How the application describes itself to the shell.
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".vsdx"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".vsdm"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".vstx"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".vstm"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""

; Lists Vera View in Settings > Apps > Default apps.
Root: HKA; Subkey: "Software\VeraView"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\VeraView\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppName}"
Root: HKA; Subkey: "Software\VeraView\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "View Microsoft Visio drawings"
Root: HKA; Subkey: "Software\VeraView\Capabilities\FileAssociations"; ValueType: string; ValueName: ".vsdx"; ValueData: "{#ProgId}"
Root: HKA; Subkey: "Software\VeraView\Capabilities\FileAssociations"; ValueType: string; ValueName: ".vsdm"; ValueData: "{#ProgId}"
Root: HKA; Subkey: "Software\VeraView\Capabilities\FileAssociations"; ValueType: string; ValueName: ".vstx"; ValueData: "{#ProgId}"
Root: HKA; Subkey: "Software\VeraView\Capabilities\FileAssociations"; ValueType: string; ValueName: ".vstm"; ValueData: "{#ProgId}"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "{#AppName}"; ValueData: "Software\VeraView\Capabilities"; Flags: uninsdeletevalue

; Optional default. Written per user only, so uninstalling restores any machine-wide default (e.g. Visio).
Root: HKCU; Subkey: "Software\Classes\.vsdx"; ValueType: string; ValueName: ""; ValueData: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKCU; Subkey: "Software\Classes\.vsdm"; ValueType: string; ValueName: ""; ValueData: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKCU; Subkey: "Software\Classes\.vstx"; ValueType: string; ValueName: ""; ValueData: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: setdefault
Root: HKCU; Subkey: "Software\Classes\.vstm"; ValueType: string; ValueName: ""; ValueData: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: setdefault

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
