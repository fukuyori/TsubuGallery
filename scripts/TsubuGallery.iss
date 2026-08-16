; TsubuGallery の Windows インストーラ (Inno Setup 6)。
;
; 単体では組み立てない。版番号と exe の場所を渡す必要があるので、
; scripts\build-installer.ps1 から呼ぶこと。
;
;   powershell -File scripts\build-installer.ps1
;
; 配る中身は tsubugallery.exe 1 つだけ。翻訳も同梱作品も SQLite も exe へ
; 取り込んであり、フォントは OS のものを借りるので、添えるファイルは無い。

#ifndef AppVersion
  #error AppVersion が未定義。build-installer.ps1 から実行すること。
#endif
#ifndef SourceExe
  #error SourceExe が未定義。build-installer.ps1 から実行すること。
#endif
#ifndef OutputBaseFilename
  #error OutputBaseFilename が未定義。build-installer.ps1 から実行すること。
#endif

#define AppName "TsubuGallery"
#define AppExe "tsubugallery.exe"
#define AppPublisher "fukuyori"
#define AppUrl "https://github.com/fukuyori/TsubuGallery"

[Setup]
; この GUID は版が変わっても絶対に変えない。変えると別アプリ扱いになり、
; 古い版が残ったまま二重にインストールされる。
AppId={{7B3C1D42-5E90-4A6F-9C17-2F8A6D0B4E31}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
AppUpdatesURL={#AppUrl}/releases
VersionInfoVersion={#AppVersion}

; 既定は管理者不要のユーザー単位インストール。ウィザードで「すべての
; ユーザー」も選べる。ギャラリーアプリのために UAC を出す必要はない。
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
; アプリ名はローカライズしない (設計書 §2)。
AllowNoIcons=yes
DisableProgramGroupPage=yes
; セットアップ自身のアイコン。アプリのものと揃える。
SetupIconFile=..\app\assets\icon.ico
; アンインストール一覧に出す絵は exe に埋めたものをそのまま使う。
UninstallDisplayIcon={app}\{#AppExe}

; wgpu も rusqlite も 64bit でしか組んでいない。
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0

OutputDir={#OutputDir}
; 出す名前は build-installer.ps1 が決める。あちらは同じ名前のファイルが
; できたか確認するので、二重に書くと食い違う。
OutputBaseFilename={#OutputBaseFilename}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[Code]
// Pascal Script の { } コメントは入れ子にならない。この節の説明は定数記法
// (波かっこ) をよく含むので、行コメントだけで書く。
//
// VC++ ランタイムの有無を先に見る。
//
// exe が要る外部 DLL はこれ 1 つだけ (ほかは Windows 標準)。入っていない環境で
// そのまま入れさせると、起動したところで「VCRUNTIME140.dll が見つかりません」に
// なる。原因が分かる場所で伝えたいので、インストール前に出す。
//
// Cargo に crt-static を入れて静的リンクへ切り替えたら、この関数ごと消してよい。
function VcRuntimePresent(): Boolean;
var
  Redirected: Boolean;
begin
  // セットアップは 32bit プロセスなので、そのまま sys 定数を見ると SysWOW64 の
  // 32bit 版を拾ってしまう。配る exe は 64bit なので、リダイレクトを切って
  // System32 側を見る。
  Redirected := EnableFsRedirection(False);
  try
    Result := FileExists(ExpandConstant('{sys}\VCRUNTIME140.dll'));
  finally
    EnableFsRedirection(Redirected);
  end;
end;

function InitializeSetup(): Boolean;
begin
  Result := True;
  if not VcRuntimePresent() then
    Result := MsgBox(
      'Microsoft Visual C++ 再頒布可能パッケージ (x64) が見つかりません。'#13#10#13#10 +
      'このままでは TsubuGallery を起動できません。'#13#10 +
      'https://aka.ms/vs/17/release/vc_redist.x64.exe から入れてください。'#13#10#13#10 +
      'それでもインストールを続けますか?',
      mbConfirmation, MB_YESNO) = IDYES;
end;

// 作品・サムネイル・設定はアンインストールでは消さない。
//
// データ領域はインストール先ではなく %APPDATA%\TsubuGallery にある。版を入れ
// 替えるだけのつもりで消えたら取り返しがつかないので、既定では残し、消すかを
// その場で訊く。
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  DataDir: String;
begin
  if CurUninstallStep <> usPostUninstall then
    Exit;
  // 黙って走らせているときは訊けない。訊けないなら消さない。
  // /SUPPRESSMSGBOXES は MsgBox に既定の答えを返させるので、ここを素通しにすると
  // 「無人アンインストール」が作品を消して回ることになる。
  if UninstallSilent() then
    Exit;
  DataDir := ExpandConstant('{userappdata}\{#AppName}');
  if not DirExists(DataDir) then
    Exit;
  // 既定を「いいえ」に寄せる (MB_DEFBUTTON2)。取り返しがつかないほうを
  // Enter 連打で選べてはいけない。
  if MsgBox(
      '作品・サムネイル・設定を消しますか?'#13#10#13#10 +
      DataDir + #13#10#13#10 +
      '「いいえ」を選ぶと残します。入れ直せばそのまま続きから使えます。',
      mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
    DelTree(DataDir, True, True, True);
end;
