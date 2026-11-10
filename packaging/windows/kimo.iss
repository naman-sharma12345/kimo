#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
#ifndef PackageDir
  #error PackageDir must point to the staged portable package
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

[Setup]
AppId={{F2592CD5-EE78-4E46-9629-E43B9536D891}
AppName=kimo
AppVersion={#AppVersion}
AppPublisher=naman-sharma12345
AppPublisherURL=https://github.com/naman-sharma12345/kimo
DefaultDirName={localappdata}\Programs\kimo
DefaultGroupName=kimo
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=kimo-{#AppVersion}-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\bin\kimo-mcp.exe
LicenseFile={#PackageDir}\LICENSE

[Files]
Source: "{#PackageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Windows setup guide"; Filename: "{app}\WINDOWS.md"
Name: "{group}\Uninstall kimo"; Filename: "{uninstallexe}"
