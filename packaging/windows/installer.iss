; Compile through scripts/build-windows-installer.ps1.
#ifndef AppVersion
  #error AppVersion must be supplied by the build script.
#endif
#ifndef BinaryPath
  #error BinaryPath must be supplied by the build script.
#endif
#ifndef RepositoryRoot
  #error RepositoryRoot must be supplied by the build script.
#endif
#ifndef ApplicationId
  #define ApplicationId "{{448C864A-F94F-462E-918B-1C6A991CE2AC}"
#endif
#ifndef PathRegistryKey
  #define PathRegistryKey "Environment"
#endif
#ifndef StateRegistryKey
  #define StateRegistryKey "Software\Lledely\dup-remover\Installer"
#endif

[Setup]
AppId={#ApplicationId}
AppName=dup-remover
AppVersion={#AppVersion}
AppPublisher=Lledely
AppPublisherURL=https://github.com/Lledely/dup-remover
AppSupportURL=https://github.com/Lledely/dup-remover/issues
DefaultDirName={localappdata}\Programs\dup-remover
DefaultGroupName=dup-remover
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
WizardStyle=modern
DisableProgramGroupPage=yes
DisableWelcomePage=no
ChangesEnvironment=yes
Compression=lzma2
SolidCompression=yes
OutputBaseFilename=dup-remover-{#AppVersion}-windows-x64-setup
UninstallDisplayIcon={app}\dup-remover.exe
CloseApplications=no
SetupLogging=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[CustomMessages]
english.AddToPath=Add dup-remover to the current user's PATH (recommended)
russian.AddToPath=Добавить dup-remover в PATH текущего пользователя (рекомендуется)
english.Instructions=Instructions
russian.Instructions=Инструкция
english.PathError=Could not update the current user's PATH. See the installation log.
russian.PathError=Не удалось обновить PATH текущего пользователя. Подробности в журнале установки.
english.InvalidPath=A folder added to PATH cannot contain a semicolon. Choose a different folder or disable the PATH option.
russian.InvalidPath=Папка для добавления в PATH не может содержать точку с запятой. Выберите другую папку или отключите добавление в PATH.
english.FinishInstructions=Open a new PowerShell window and run: dup-remover --help. If you did not add PATH, run dup-remover.exe from the installation folder. Instructions are available in the Start menu.
russian.FinishInstructions=Откройте новое окно PowerShell и выполните: dup-remover --help. Если добавление в PATH отключено, запускайте dup-remover.exe из папки установки. Инструкция доступна в меню «Пуск».

[Tasks]
Name: "addtopath"; Description: "{cm:AddToPath}"

[Files]
Source: "{#BinaryPath}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepositoryRoot}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepositoryRoot}\packaging\windows\usage.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{cm:Instructions}"; Filename: "{app}\usage.txt"; Check: not WizardNoIcons

[Code]
const
  UserPathKey = '{#PathRegistryKey}';
  InstallerStateKey = '{#StateRegistryKey}';
  StringValue = 1;
  ExpandStringValue = 2;

{ Inno Setup's default setup loader is 32-bit; the bundled application is x64. }
function RegGetValueW(Key: Cardinal; SubKey, ValueName: String;
  Flags: Cardinal; var ValueType: Cardinal; Data: Cardinal;
  var DataSize: Cardinal): Longint;
  external 'RegGetValueW@advapi32.dll stdcall';

function ReadUserPath(var Value: String; var ValueType: Cardinal): Boolean;
var
  DataSize: Cardinal;
begin
  Value := '';
  ValueType := ExpandStringValue;
  Result := RegValueExists(HKCU, UserPathKey, 'Path');
  if not Result then
    exit;
  DataSize := 0;
  { RRF_NOEXPAND | RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ. }
  if (RegGetValueW(HKCU, UserPathKey, 'Path', $10000006,
      ValueType, 0, DataSize) <> 0) or
      not RegQueryStringValue(HKCU, UserPathKey, 'Path', Value) then
    RaiseException(CustomMessage('PathError'));
end;

function WriteUserPath(const Value: String; ValueType: Cardinal): Boolean;
begin
  if ValueType = StringValue then
    Result := RegWriteStringValue(HKCU, UserPathKey, 'Path', Value)
  else
    Result := RegWriteExpandStringValue(HKCU, UserPathKey, 'Path', Value);
end;

function ExpandEnvironmentReferences(const Value: String): String;
var
  Remaining, Name, Expanded: String;
  Opening, Closing: Integer;
begin
  Result := '';
  Remaining := Value;
  while Remaining <> '' do begin
    Opening := Pos('%', Remaining);
    if Opening = 0 then begin
      Result := Result + Remaining;
      exit;
    end;
    Result := Result + Copy(Remaining, 1, Opening - 1);
    Delete(Remaining, 1, Opening);
    Closing := Pos('%', Remaining);
    if Closing = 0 then begin
      Result := Result + '%' + Remaining;
      exit;
    end;
    Name := Copy(Remaining, 1, Closing - 1);
    Expanded := GetEnv(Name);
    if Expanded = '' then
      Result := Result + '%' + Name + '%'
    else
      Result := Result + Expanded;
    Delete(Remaining, 1, Closing);
  end;
end;

function NormalizePath(const Value: String): String;
begin
  Result := ExpandEnvironmentReferences(RemoveQuotes(Trim(Value)));
  StringChangeEx(Result, '/', '\', True);
  while Length(Result) > 3 do begin
    if Result[Length(Result)] <> '\' then
      break;
    Delete(Result, Length(Result), 1);
  end;
  Result := LowerCase(Result);
end;

function FindPathEntry(const Value, Entry: String;
  var First, Last: Integer): Boolean;
var
  Start, Separator: Integer;
  Remaining: String;
begin
  Result := False;
  Start := 1;
  Remaining := Value;
  while Remaining <> '' do begin
    Separator := Pos(';', Remaining);
    if Separator = 0 then
      Separator := Length(Remaining) + 1;
    if NormalizePath(Copy(Remaining, 1, Separator - 1)) = NormalizePath(Entry) then begin
      First := Start;
      Last := Start + Separator - 2;
      Result := True;
      exit;
    end;
    Delete(Remaining, 1, Separator);
    Start := Start + Separator;
  end;
end;

procedure ClearOwnership;
begin
  RegDeleteValue(HKCU, InstallerStateKey, 'OwnedPathEntry');
  RegDeleteValue(HKCU, InstallerStateKey, 'AddedSeparator');
  RegDeleteValue(HKCU, InstallerStateKey, 'PathExisted');
  RegDeleteKeyIfEmpty(HKCU, InstallerStateKey);
end;

procedure RemoveOwnedPath;
var
  OwnedEntry, Value: String;
  ValueType, AddedSeparator, PathExisted: Cardinal;
  First, Last: Integer;
begin
  if not RegQueryStringValue(HKCU, InstallerStateKey, 'OwnedPathEntry', OwnedEntry) then
    exit;
  AddedSeparator := 1;
  PathExisted := 1;
  RegQueryDWordValue(HKCU, InstallerStateKey, 'AddedSeparator', AddedSeparator);
  RegQueryDWordValue(HKCU, InstallerStateKey, 'PathExisted', PathExisted);
  if ReadUserPath(Value, ValueType) and FindPathEntry(Value, OwnedEntry, First, Last) then begin
    if Last < Length(Value) then
      { Keep the preceding separator when later entries have been added. }
      Delete(Value, First, Last - First + 2)
    else begin
      if (First > 1) and (AddedSeparator = 1) then
        First := First - 1;
      Delete(Value, First, Last - First + 1);
    end;
    if (Value = '') and (PathExisted = 0) then begin
      if not RegDeleteValue(HKCU, UserPathKey, 'Path') then
        RaiseException(CustomMessage('PathError'));
    end else if not WriteUserPath(Value, ValueType) then
      RaiseException(CustomMessage('PathError'));
  end;
  ClearOwnership;
end;

procedure AddInstallPath;
var
  Value, InstallPath, OwnedEntry: String;
  ValueType, AddedSeparator, Existed: Cardinal;
  First, Last: Integer;
begin
  InstallPath := ExpandConstant('{app}');
  if RegQueryStringValue(HKCU, InstallerStateKey, 'OwnedPathEntry', OwnedEntry) then
    if NormalizePath(OwnedEntry) <> NormalizePath(InstallPath) then
      RemoveOwnedPath;
  Existed := 0;
  if ReadUserPath(Value, ValueType) then
    Existed := 1;
  if FindPathEntry(Value, InstallPath, First, Last) then
    { A pre-existing entry is not owned by this installer. Keep prior ownership
      during upgrades, but never claim a user-created entry. }
    exit;
  AddedSeparator := 0;
  if Value <> '' then
    if Value[Length(Value)] <> ';' then begin
      Value := Value + ';';
      AddedSeparator := 1;
    end;
  if not RegWriteStringValue(HKCU, InstallerStateKey, 'OwnedPathEntry', InstallPath) or
     not RegWriteDWordValue(HKCU, InstallerStateKey, 'AddedSeparator', AddedSeparator) or
     not RegWriteDWordValue(HKCU, InstallerStateKey, 'PathExisted', Existed) then
    RaiseException(CustomMessage('PathError'));
  if not WriteUserPath(Value + InstallPath, ValueType) then begin
    ClearOwnership;
    RaiseException(CustomMessage('PathError'));
  end;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = wpReady) and WizardIsTaskSelected('addtopath') and
      (Pos(';', WizardDirValue) > 0) then begin
    SuppressibleMsgBox(CustomMessage('InvalidPath'), mbError, MB_OK, IDOK);
    Result := False;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then begin
    if WizardIsTaskSelected('addtopath') then
      AddInstallPath
    else
      RemoveOwnedPath;
  end;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if CurPageID = wpFinished then
    WizardForm.FinishedLabel.Caption := CustomMessage('FinishInstructions');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RemoveOwnedPath;
end;
