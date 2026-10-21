; Instalator bluepane dla Windows (Inno Setup 6). Budowany przez workflow `Wydanie`:
;   iscc /DAppVersion=0.1.0 /DSourceDir=C:\...\dist\bluepane-0.1.0-windows-x86_64 /DOutDir=C:\... installer\bluepane.iss
; Domyślnie instaluje dla bieżącego użytkownika (bez praw administratora); okno instalatora pozwala
; wybrać instalację dla wszystkich. Ustawienia użytkownika (%APPDATA%\bluepane) zostają przy
; odinstalowaniu.

#ifndef AppVersion
  #error Podaj wersję: /DAppVersion=0.1.0
#endif
#ifndef SourceDir
  #error Podaj katalog z paczką: /DSourceDir=...
#endif
#ifndef OutDir
  #define OutDir "."
#endif

[Setup]
AppId={{6F0B8E2A-4C1D-4E7B-9A53-00BF1A9E0001}
AppName=bluepane
AppVersion={#AppVersion}
AppVerName=bluepane {#AppVersion}
AppPublisher=bluepane
AppPublisherURL=https://github.com/mateuszlenart-pixel/bluepane
AppSupportURL=https://github.com/mateuszlenart-pixel/bluepane/issues
AppUpdatesURL=https://github.com/mateuszlenart-pixel/bluepane/releases
DefaultDirName={autopf}\bluepane
DefaultGroupName=bluepane
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutDir}
OutputBaseFilename=bluepane-{#AppVersion}-windows-x86_64-setup
SetupIconFile={#SourceDir}\icon.ico
UninstallDisplayIcon={app}\bluepane.exe
LicenseFile={#SourceDir}\LICENSE-MIT
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "polish"; MessagesFile: "compiler:Languages\Polish.isl"

[Tasks]
Name: "desktopicon"; Description: "Skrót na pulpicie"; GroupDescription: "Skróty:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\bluepane.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\bluepane-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\icon.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\bluepane"; Filename: "{app}\bluepane.exe"; IconFilename: "{app}\icon.ico"
Name: "{group}\Odinstaluj bluepane"; Filename: "{uninstallexe}"
Name: "{autodesktop}\bluepane"; Filename: "{app}\bluepane.exe"; IconFilename: "{app}\icon.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\bluepane.exe"; Description: "Uruchom bluepane"; Flags: nowait postinstall skipifsilent
