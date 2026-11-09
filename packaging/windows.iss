#ifndef AppVersion
  #error AppVersion must be supplied by scripts/package-windows.ps1
#endif
#ifndef SourceDir
  #error SourceDir must point to the prepared portable files
#endif
#ifndef OutputDir
  #error OutputDir must point to the release output directory
#endif

[Setup]
AppId={{7B77A76F-0FA4-455D-977A-DC05E0E5992B}
AppName=Console Link
AppVersion={#AppVersion}
AppPublisher=Potanull
AppPublisherURL=https://github.com/Potanull/console-link
AppSupportURL=https://github.com/Potanull/console-link/issues
AppUpdatesURL=https://github.com/Potanull/console-link/releases
DefaultDirName={localappdata}\Programs\Console Link
DefaultGroupName=Console Link
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
OutputDir={#OutputDir}
OutputBaseFilename=Console-Link-{#AppVersion}-Setup-x64
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\console-link.exe
UninstallDisplayName=Console Link
VersionInfoVersion={#AppVersion}
VersionInfoDescription=Console Link Setup
VersionInfoCompany=Potanull
VersionInfoCopyright=Copyright (c) 2026 Potanull
LicenseFile=..\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
SetupLogging=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\console-link.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD_PARTY.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD_PARTY_LICENSES.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Console Link"; Filename: "{app}\console-link.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Console Link"; Filename: "{app}\console-link.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\console-link.exe"; Description: "{cm:LaunchProgram,Console Link}"; Flags: nowait postinstall skipifsilent
