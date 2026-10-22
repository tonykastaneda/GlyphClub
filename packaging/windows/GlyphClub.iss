; GlyphClub's Windows installer (Inno Setup 6): GlyphClub-Setup.exe.
; Installs for the current user only, so it needs no administrator prompt:
; GlyphClub.exe, the icon font sets (icon-fonts\) and the Illustrator
; plug-ins (plugins\) go together into %LOCALAPPDATA%\Programs\GlyphClub,
; where the app looks for them beside its .exe.
;
; Built by scripts/package-windows.ps1 from the staged folder:
;   iscc /DAppVersion=0.2.4 /DSourceDir=..\..\target\package\windows packaging\windows\GlyphClub.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\target\package\windows"
#endif

[Setup]
; Never change AppId: it's how Windows knows a new version replaces the old.
AppId={{7767BEF3-DFB7-4330-876A-880178EE59CE}
AppName=GlyphClub
AppVersion={#AppVersion}
AppVerName=GlyphClub {#AppVersion}
AppPublisher=GlyphClub
AppPublisherURL=https://tonykastaneda.github.io/GlyphClub/
AppSupportURL=https://github.com/tonykastaneda/GlyphClub
DefaultDirName={localappdata}\Programs\GlyphClub
DisableDirPage=yes
DefaultGroupName=GlyphClub
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
; x64 build; runs on ARM64 Windows too, under its x64 emulation.
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\target\package
OutputBaseFilename=GlyphClub-Setup
SetupIconFile=..\..\branding\windows\GlyphClub.ico
UninstallDisplayIcon={app}\GlyphClub.exe
UninstallDisplayName=GlyphClub
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
; A running GlyphClub is closed first so its files can be replaced.
CloseApplications=force
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\GlyphClub.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\icon-fonts\*"; DestDir: "{app}\icon-fonts"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#SourceDir}\plugins\*"; DestDir: "{app}\plugins"; Flags: ignoreversion recursesubdirs createallsubdirs skipifsourcedoesntexist

[InstallDelete]
; Icon sets and plug-ins are replaced whole, so removed ones don't linger.
Type: filesandordirs; Name: "{app}\icon-fonts"
Type: filesandordirs; Name: "{app}\plugins"

[Icons]
Name: "{autoprograms}\GlyphClub"; Filename: "{app}\GlyphClub.exe"
Name: "{autodesktop}\GlyphClub"; Filename: "{app}\GlyphClub.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\GlyphClub.exe"; Description: "Open GlyphClub"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Quit GlyphClub so its files can be removed.
Filename: "{sys}\taskkill.exe"; Parameters: "/IM GlyphClub.exe /F"; Flags: runhidden; RunOnceId: "QuitGlyphClub"

[UninstallDelete]
; What GlyphClub itself leaves beside the app; the catalog and settings
; in %APPDATA%\glyphclub are the user's and stay.
Type: filesandordirs; Name: "{app}"

[Code]
// Settings > General > Launch GlyphClub on startup writes this Run value
// (src/login_item.rs); it would point at a removed GlyphClub.exe.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'GlyphClub');
end;
