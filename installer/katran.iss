#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef PackageRoot
  #error PackageRoot is required
#endif
#ifndef OutputRoot
  #error OutputRoot is required
#endif
[Setup]
AppId={{395E905A-BB31-49BD-BF72-D0FD3ADCFD22}
AppName=Katran
AppVersion={#AppVersion}
AppPublisher=LilyAvelis
AppPublisherURL=https://github.com/LilyAvelis/Katran
DefaultDirName={localappdata}\Programs\Katran
DefaultGroupName=Katran
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputRoot}
OutputBaseFilename=Katran-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
CloseApplications=no
RestartApplications=no
UninstallDisplayIcon={app}\Katran.exe
[Files]
Source: "{#PackageRoot}\Katran.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageRoot}\katran-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageRoot}\runtime\*"; DestDir: "{app}\runtime"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#PackageRoot}\AFIO-LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageRoot}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageRoot}\THIRD-PARTY-NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
[UninstallDelete]
Type: files; Name: "{app}\installed.flag"
[Icons]
Name: "{autoprograms}\Katran"; Filename: "{app}\Katran.exe"; WorkingDir: "{app}"
[Code]
procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    if not SaveStringToFile(ExpandConstant('{app}\installed.flag'), '', False) then
      RaiseException('Cannot create the Katran installed-mode marker.');
end;
// Data in {localappdata}\Katran\data is deliberately outside the install tree.
// Never remove it in [UninstallDelete] or execute the application from setup.
