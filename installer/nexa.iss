; Nexa Dir 2 — 설치형 exe(보조 채널, DR-3 개정 07-16). 기본 채널은 포터블 단일 exe.
; 빌드: ISCC.exe /DAppVersion=<버전> /DExePath=<포터블 exe 경로> installer\nexa.iss
; CI(release.yml)가 태그 push 시 자동 빌드 — windows-latest 러너에 Inno Setup 6 내장.
;
; 설계(docs/21-distribution.md §3):
; - PrivilegesRequired=lowest + 다이얼로그 = 기본 **사용자별 설치**(관리자 불요 —
;   {autopf} = %LOCALAPPDATA%\Programs, VS Code 방식). 관리자 선택 시 Program Files.
; - Program Files 설치에서도 데이터는 앱의 data_dir 폴백(%LOCALAPPDATA%\NexaDir2\data)
;   이 처리 — 설치 스크립트는 데이터 경로를 만들지 않는다.
; - 제거 시 사용자 데이터(설정·세션)는 보존(명시 삭제 안 함 — 재설치 복원 기대).

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef ExePath
  #define ExePath "..\target\release\nexa-app.exe"
#endif

[Setup]
AppId={{7E4B1C9D-3A52-4F8E-9B70-6C2D815FA3E1}
AppName=Nexa Dir 2
AppVersion={#AppVersion}
AppPublisher=SosomLab
AppPublisherURL=https://sosomlab.com
DefaultDirName={autopf}\NexaDir2
DefaultGroupName=Nexa Dir 2
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=out
OutputBaseFilename=NexaDir2-Setup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
LicenseFile=..\LICENSE.md
UninstallDisplayIcon={app}\NexaDir2.exe

[Languages]
; Korean.isl은 Inno 공식 배포에 없음(비공식 번역) — 러너 빌드 실패 방지 위해 영어만.
; 한국어 설치 UI가 필요해지면 번역 .isl을 installer/에 동봉 후 여기 추가.
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; 포터블 단일 exe 그대로 설치(추가 파일 0 — 최소파일 규율 공유)
Source: "{#ExePath}"; DestDir: "{app}"; DestName: "NexaDir2.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Nexa Dir 2"; Filename: "{app}\NexaDir2.exe"
Name: "{autodesktop}\Nexa Dir 2"; Filename: "{app}\NexaDir2.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\NexaDir2.exe"; Description: "{cm:LaunchProgram,Nexa Dir 2}"; Flags: nowait postinstall skipifsilent
