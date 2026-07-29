; Ceyrad for Windows — Inno Setup script.
;
; Per-user install, no elevation, matching the app's own design (settings in
; %APPDATA%, "Launch at Login" as an HKCU Run value the app manages itself —
; see src/launch_at_login.rs). The installer does not touch that key; the
; tray menu's own toggle is the one and only place that entry is written, so
; there is never a second, conflicting Run entry to reconcile.
;
; Build with: iscc ceyrad.iss /DMyAppVersion=1.2.3
; (CI passes the tag-derived version; a local build without /D falls back to
; the placeholder below so `iscc ceyrad.iss` alone still works for testing.)

#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif
#define MyAppName "Ceyrad"
#define MyAppPublisher "Narcissus-tazetta"
#define MyAppURL "https://github.com/Narcissus-tazetta/Ceyrad"
#define MyAppExeName "ceyrad.exe"
#define MyAppIcoName "ceyrad.ico"

[Setup]
; Fixed at random, once — this is what lets successive versions upgrade in
; place instead of installing side by side. Never regenerate it.
AppId={{E215E75C-5B06-44C6-BF5C-40CBEFE328DF}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
VersionInfoVersion={#MyAppVersion}
; %LOCALAPPDATA%\Programs is the standard per-user install location (Discord,
; VS Code and friends all land here) — writable without admin, and it keeps
; the exe out of anything a system backup/restore would treat specially.
DefaultDirName={localappdata}\Programs\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=dist
OutputBaseFilename=Ceyrad-Setup-{#MyAppVersion}
SetupIconFile=assets\{#MyAppIcoName}
UninstallDisplayIcon={app}\{#MyAppIcoName}
UninstallDisplayName={#MyAppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; ceyrad only ships an x64 build (see release.yml); refuse the 32-bit-only
; and ARM64-without-x64-emulation cases rather than install a binary that
; cannot run.
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; The app has no window and no visible "quit" while it's meant to be running
; forever in the tray, so an in-place upgrade over a running instance is the
; common case, not an edge case. Restart Manager (built into Inno Setup 6)
; finds whichever process is holding ceyrad.exe open and asks it to close.
;
; The tray's message loop does not handle WM_QUERYENDSESSION (verified by
; running an upgrade over a live instance: Setup waited the full ~30s grace
; period, the process never exited, and `yes` aborted+rolled back the whole
; install). `force` is what turns that same wait into a plain terminate
; instead of a failed install — safe here since there is no unsaved state to
; lose: every setting is written to settings.json the moment it changes, not
; on exit.
CloseApplications=force
CloseApplicationsFilter={#MyAppExeName}
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "assets\{#MyAppIcoName}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppIcoName}"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppIcoName}"; Tasks: desktopicon

[Run]
; The app has no window to bring to front, so there's nothing misleading
; about launching it silently right after Finish.
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Settings (%APPDATA%\Ceyrad\settings.json) and the Run key the app manages
; itself are deliberately left alone by the uninstaller: the settings file is
; user data, not an install artifact, and removing the Run value out from
; under a user who reinstalls later would silently change their preference.
; Only make sure the process isn't left running with no exe underneath it.
Filename: "{cmd}"; Parameters: "/C taskkill /IM {#MyAppExeName} /F"; Flags: runhidden skipifdoesntexist; RunOnceId: "StopCeyrad"
