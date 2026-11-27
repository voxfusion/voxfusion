; Inno Setup script for VoxFusion on Windows (x64).
; Invoked by packages/app/scripts/bundle-windows.ps1.

#ifndef AppVersion
  #define AppVersion "0.3.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\dist\x86_64-pc-windows-msvc\VoxFusion"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist\x86_64-pc-windows-msvc"
#endif
#ifndef OutputBaseFilename
  #define OutputBaseFilename "VoxFusion-windows-x64-setup"
#endif

[Setup]
AppId={{B3D6B1D2-9F8A-4D71-9E5A-2C8B6A1E4F90}
AppName=VoxFusion
AppVersion={#AppVersion}
AppVerName=VoxFusion {#AppVersion}
AppPublisher=VoxFusion
AppPublisherURL=https://voxfusion.io
AppSupportURL=https://github.com/voxfusion/voxfusion/issues
AppUpdatesURL=https://github.com/voxfusion/voxfusion/releases
DefaultDirName={autopf}\VoxFusion
DefaultGroupName=VoxFusion
AllowNoIcons=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename={#OutputBaseFilename}
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\voxfusion-app.exe
UninstallDisplayName=VoxFusion
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
CloseApplicationsFilter=*.exe
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\VoxFusion"; Filename: "{app}\voxfusion-app.exe"; IconFilename: "{app}\voxfusion-app.exe"
Name: "{autodesktop}\VoxFusion"; Filename: "{app}\voxfusion-app.exe"; IconFilename: "{app}\voxfusion-app.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\voxfusion-app.exe"; Description: "{cm:LaunchProgram,VoxFusion}"; Flags: nowait postinstall skipifsilent
