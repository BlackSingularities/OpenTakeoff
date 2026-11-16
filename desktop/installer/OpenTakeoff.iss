; Instalator OpenTakeoff dla Windows – w Program Files, z uprawnieniami administratora.
; Aktualizacje trafiają zawsze do tego samego folderu: stały AppId, folder poprzedniej instalacji
; z rejestru (sekcja [Code] – wygrywa nawet z innym /DIR) i /DIR z zainstalowanego programu.
; Wersję podaje workflow: ISCC /DAppVersion=1.2.3 desktop\installer\OpenTakeoff.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{8C3E2A71-5B9D-4E6F-A1C4-7D2E9F0B3A58}
AppName=OpenTakeoff
AppVersion={#AppVersion}
AppVerName=OpenTakeoff {#AppVersion}
AppPublisher=Adam Rędzikowski
AppPublisherURL=https://redzikowski.com
AppSupportURL=https://redzikowski.com/portfolio/opentakeoff
AppUpdatesURL=https://redzikowski.com/portfolio/opentakeoff
AppCopyright=Pomysł i wykonanie: Adam Rędzikowski · redzikowski.com
DefaultDirName={autopf}\OpenTakeoff
DefaultGroupName=OpenTakeoff
DisableProgramGroupPage=yes
DisableDirPage=auto
UsePreviousAppDir=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\..\out
OutputBaseFilename=OpenTakeoff-Setup-{#AppVersion}
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\OpenTakeoff.exe
UninstallDisplayName=OpenTakeoff
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; force: także proces czuwania na symulator (bez okna), żeby aktualizacja mogła podmienić plik programu
CloseApplications=force
RestartApplications=yes
VersionInfoVersion={#AppVersion}
VersionInfoDescription=OpenTakeoff – dziennik lotów, briefing i most MSFS ↔ telefon

[Languages]
Name: "pl"; MessagesFile: "compiler:Languages\Polish.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"

[CustomMessages]
pl.Autostart=Uruchamiaj OpenTakeoff razem z Windows (w zasobniku)
en.Autostart=Start OpenTakeoff with Windows (in the tray)
es.Autostart=Iniciar OpenTakeoff con Windows (en la bandeja)
pl.StartGroup=Uruchamianie:
en.StartGroup=Startup:
es.StartGroup=Inicio:

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "autostart"; Description: "{cm:Autostart}"; GroupDescription: "{cm:StartGroup}"; Flags: unchecked

[Files]
Source: "..\..\dist\OpenTakeoff.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\vendor\SimConnect.dll"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\OpenTakeoff"; Filename: "{app}\OpenTakeoff.exe"
Name: "{autodesktop}\OpenTakeoff"; Filename: "{app}\OpenTakeoff.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "OpenTakeoff"; ValueData: """{app}\OpenTakeoff.exe"" --minimized"; Flags: uninsdeletevalue; Tasks: autostart

[Run]
; Telefony łączą się po Wi-Fi: zgoda zapory dla sieci prywatnych (port 47810 TCP, 47811 UDP)
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""OpenTakeoff"""; Flags: runhidden waituntilterminated
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""OpenTakeoff"" dir=in action=allow program=""{app}\OpenTakeoff.exe"" enable=yes profile=private,domain"; Flags: runhidden waituntilterminated
Filename: "{app}\OpenTakeoff.exe"; Description: "{cm:LaunchProgram,OpenTakeoff}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Usuń OpenTakeoff z exe.xml MSFS (start razem z symulatorem)
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM OpenTakeoff.exe"; Flags: runhidden waituntilterminated; RunOnceId: "StopWatch"
Filename: "{app}\OpenTakeoff.exe"; Parameters: "--sim-launch-off"; Flags: runhidden waituntilterminated; RunOnceId: "SimLaunchOff"
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""OpenTakeoff"""; Flags: runhidden; RunOnceId: "FirewallRule"

[Code]
// Aktualizacja zawsze do folderu poprzedniej instalacji – także gdy ktoś uruchomi instalator ręcznie
// albo poda inny /DIR. Folder bierzemy z wpisu deinstalatora (64- i 32-bitowy widok rejestru, HKCU).
const UninstKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{8C3E2A71-5B9D-4E6F-A1C4-7D2E9F0B3A58}_is1';

function PreviousDir(): String;
var
  S: String;
begin
  Result := '';
  if RegQueryStringValue(HKLM64, UninstKey, 'Inno Setup: App Path', S) or
     RegQueryStringValue(HKLM32, UninstKey, 'Inno Setup: App Path', S) or
     RegQueryStringValue(HKCU, UninstKey, 'Inno Setup: App Path', S) then
    if FileExists(AddBackslash(S) + 'OpenTakeoff.exe') then
      Result := RemoveBackslashUnlessRoot(S);
end;

procedure InitializeWizard();
var
  Dir: String;
begin
  Dir := PreviousDir();
  if Dir <> '' then
    WizardForm.DirEdit.Text := Dir;
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  // Przy aktualizacji nie pytamy o folder.
  Result := (PageID = wpSelectDir) and (PreviousDir() <> '');
end;

[Dirs]
; Dane programu obok programu (ustawienia, dziennik lotów, pamięć podręczna) – zapis dla użytkowników
; bez uprawnień administratora. Odinstalowanie zostawia dziennik i ustawienia.
Name: "{app}\data"; Permissions: users-modify; Flags: uninsneveruninstall

[UninstallDelete]
Type: filesandordirs; Name: "{app}\data\cache"
Type: filesandordirs; Name: "{localappdata}\OpenTakeoff\update"
