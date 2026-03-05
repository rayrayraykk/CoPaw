; Inno Setup script for CoPaw Windows installer.
; Expects Nuitka standalone output in dist/copaw_desktop_main.dist/
; Build with: ISCC.exe scripts/pack/CoPaw.iss

#define AppName "CoPaw"
#define AppExe "CoPaw.exe"
#define RunNumber "0"
; Paths relative to script dir (scripts/pack); repo root = ../..
#define DistDir "..\..\dist\copaw_desktop_main.dist"

[Setup]
AppId={{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}
AppName={#AppName}
AppVersion=0.0.5
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
OutputDir=..\..\dist
OutputBaseFilename=CoPaw-Setup-{#RunNumber}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional icons:"

[Files]
Source: "{#DistDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent
