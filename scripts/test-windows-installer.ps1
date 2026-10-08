[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$CompilerPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Installer tests require Windows.' }
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$testId = [guid]::NewGuid().ToString('N')
$applicationId = 'DR-Test-' + $testId
$testRegistryRoot = 'Software\Lledely\dup-remover\InstallerTests\' + $testId
$pathRegistryKey = $testRegistryRoot + '\Environment'
$stateRegistryKey = $testRegistryRoot + '\State'
$uninstallRegistryKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\' + $applicationId + '_is1'
$testDirectory = Join-Path $repositoryRoot ('target\installer-smoke\' + $testId)
$installationDirectory = Join-Path $testDirectory 'Install With Spaces'
New-Item -ItemType Directory -Path $testDirectory -Force | Out-Null
$oldProcessPath = $env:PATH
$oldInstallTestVariable = $env:DUP_REMOVER_TEST_INSTALL_DIRECTORY
$env:DUP_REMOVER_TEST_INSTALL_DIRECTORY = $installationDirectory

function Assert-Condition([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Set-TestPath([AllowNull()][object]$Value, [Microsoft.Win32.RegistryValueKind]$Kind) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($pathRegistryKey)
    try {
        if ($null -eq $Value) { $key.DeleteValue('Path', $false) }
        else { $key.SetValue('Path', $Value, $Kind) }
    } finally { $key.Dispose() }
}

function Get-TestPath {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($pathRegistryKey)
    try {
        $exists = $key -and ($key.GetValueNames() -contains 'Path')
        [pscustomobject]@{
            Exists = [bool]$exists
            Value = if ($exists) { $key.GetValue('Path', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) } else { $null }
            Kind = if ($exists) { $key.GetValueKind('Path') } else { $null }
        }
    } finally { if ($key) { $key.Dispose() } }
}

function Install-TestApp([string]$Tasks) {
    $arguments = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOICONS /LANG=russian /DIR="' +
        $installationDirectory + '" /TASKS="' + $Tasks + '" /LOG="' +
        (Join-Path $testDirectory 'setup.log') + '"'
    $process = Start-Process -FilePath $testInstaller -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
    Assert-Condition ($process.ExitCode -eq 0) "Test install failed with code $($process.ExitCode). See $testDirectory."
    Assert-Condition (Test-Path -LiteralPath (Join-Path $installationDirectory 'dup-remover.exe')) 'Installed binary is missing.'
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($uninstallRegistryKey)
    Assert-Condition ($null -ne $key) 'Windows uninstall registration is missing.'
    if ($key) { $key.Dispose() }
}

function Uninstall-TestApp {
    $uninstaller = Join-Path $installationDirectory 'unins000.exe'
    if (Test-Path -LiteralPath $uninstaller) {
        $arguments = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /LOG="' + (Join-Path $testDirectory 'uninstall.log') + '"'
        $process = Start-Process -FilePath $uninstaller -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
        Assert-Condition ($process.ExitCode -eq 0) "Test uninstall failed with code $($process.ExitCode)."
        Assert-Condition (-not (Test-Path -LiteralPath (Join-Path $installationDirectory 'dup-remover.exe'))) 'Uninstall left the binary behind.'
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($uninstallRegistryKey)
        try { Assert-Condition ($null -eq $key) 'Uninstall left its Windows registration behind.' }
        finally { if ($key) { $key.Dispose() } }
    }
}

Push-Location $repositoryRoot
try {
    $metadataText = & cargo metadata --no-deps --locked --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
    $metadata = $metadataText | ConvertFrom-Json
    $version = @($metadata.packages | Where-Object name -EQ 'dup-remover')[0].version
    $binaryPath = Join-Path $metadata.target_directory 'x86_64-pc-windows-msvc\release\dup-remover.exe'
    Assert-Condition (Test-Path -LiteralPath $binaryPath) 'Run build-windows-installer.ps1 first.'
    $compilerArguments = @(
        '/Q', "/DAppVersion=$version", "/DBinaryPath=$binaryPath", "/DRepositoryRoot=$repositoryRoot",
        "/DApplicationId=$applicationId", "/DPathRegistryKey=$pathRegistryKey", "/DStateRegistryKey=$stateRegistryKey",
        "/O$testDirectory", '/Finstaller-test', (Join-Path $repositoryRoot 'packaging\windows\installer.iss')
    )
    & $CompilerPath @compilerArguments
    if ($LASTEXITCODE -ne 0) { throw 'Test installer compilation failed.' }
    $testInstaller = Join-Path $testDirectory 'installer-test.exe'

    # Exercise the real installer and uninstaller against an isolated HKCU key:
    # the actual user PATH and any existing product installation remain untouched.
    $originalPath = 'C:\Tools;%LOCALAPPDATA%\Programs\Existing;C:\More Tools;'
    Set-TestPath $originalPath ([Microsoft.Win32.RegistryValueKind]::ExpandString)
    Install-TestApp 'addtopath'
    $firstPath = Get-TestPath
    Assert-Condition ($firstPath.Value -eq $originalPath + $installationDirectory) 'Install changed unrelated PATH entries.'
    Assert-Condition ($firstPath.Kind -eq [Microsoft.Win32.RegistryValueKind]::ExpandString) 'Install changed the PATH registry type.'
    Install-TestApp 'addtopath'
    Assert-Condition ((Get-TestPath).Value -eq $firstPath.Value) 'Reinstall added a duplicate PATH entry.'

    $env:PATH = $installationDirectory + ';' + $env:SystemRoot + '\System32'
    $versionOutput = & dup-remover --version
    Assert-Condition ($LASTEXITCODE -eq 0 -and $versionOutput -eq "dup-remover $version") 'Installed command failed without Rust in PATH.'
    $env:PATH = $oldProcessPath
    $fixtures = Join-Path $testDirectory 'fixtures'
    New-Item -ItemType Directory -Path $fixtures -Force | Out-Null
    [System.IO.File]::WriteAllBytes((Join-Path $fixtures 'original.txt'), [System.Text.Encoding]::UTF8.GetBytes('rust'))
    [System.IO.File]::WriteAllBytes((Join-Path $fixtures 'copy.txt'), [System.Text.Encoding]::UTF8.GetBytes('rust'))
    [System.IO.File]::WriteAllBytes((Join-Path $fixtures 'different.txt'), [System.Text.Encoding]::UTF8.GetBytes('code'))
    $reportPath = Join-Path $testDirectory 'scan-report.json'
    & (Join-Path $installationDirectory 'dup-remover.exe') scan $fixtures --output $reportPath
    Assert-Condition ($LASTEXITCODE -eq 0) 'Installed scan command failed.'
    $report = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
    Assert-Condition ($report.scanned_files -eq 3 -and $report.duplicate_groups.Count -eq 1 -and $report.duplicate_groups[0].paths.Count -eq 2) 'Installed command returned an incorrect report.'
    $userReport = Join-Path $installationDirectory 'keep-user-report.json'
    [System.IO.File]::WriteAllText($userReport, 'user data')
    Set-TestPath ($firstPath.Value + ';C:\Later Tools') ([Microsoft.Win32.RegistryValueKind]::ExpandString)
    Uninstall-TestApp
    Assert-Condition ((Get-TestPath).Value -eq $originalPath + 'C:\Later Tools') 'Uninstall damaged an unrelated PATH entry added later.'
    Assert-Condition ([System.IO.File]::ReadAllText($userReport) -eq 'user data') 'Uninstall removed a user-created file.'

    $originalPath = 'C:\Tools;C:\More Tools'
    Set-TestPath $originalPath ([Microsoft.Win32.RegistryValueKind]::String)
    Install-TestApp ''
    Assert-Condition ((Get-TestPath).Value -eq $originalPath) 'Disabled PATH option still modified PATH.'
    Uninstall-TestApp
    Assert-Condition ((Get-TestPath).Value -eq $originalPath -and (Get-TestPath).Kind -eq [Microsoft.Win32.RegistryValueKind]::String) 'Uninstall changed an unowned PATH.'

    # Environment references, quotation marks and a trailing slash must not
    # turn an existing user-created entry into an installer-owned entry.
    $originalPath = 'C:\Tools;"%DUP_REMOVER_TEST_INSTALL_DIRECTORY%\";C:\Later Tools'
    Set-TestPath $originalPath ([Microsoft.Win32.RegistryValueKind]::ExpandString)
    Install-TestApp 'addtopath'
    Assert-Condition ((Get-TestPath).Value -eq $originalPath) 'Install duplicated an equivalent pre-existing PATH entry.'
    Uninstall-TestApp
    Assert-Condition ((Get-TestPath).Value -eq $originalPath) 'Uninstall deleted a user-owned PATH entry.'

    Set-TestPath 'C:\Tools' ([Microsoft.Win32.RegistryValueKind]::String)
    Install-TestApp 'addtopath'
    Install-TestApp ''
    Assert-Condition ((Get-TestPath).Value -eq 'C:\Tools' -and (Get-TestPath).Kind -eq [Microsoft.Win32.RegistryValueKind]::String) 'Opting out on reinstall failed to restore the original PATH.'
    Uninstall-TestApp

    Set-TestPath $null ([Microsoft.Win32.RegistryValueKind]::ExpandString)
    Install-TestApp 'addtopath'
    Uninstall-TestApp
    Assert-Condition (-not (Get-TestPath).Exists) 'Uninstall did not restore an originally absent PATH.'

    Set-TestPath '' ([Microsoft.Win32.RegistryValueKind]::String)
    Install-TestApp 'addtopath'
    Uninstall-TestApp
    Assert-Condition ((Get-TestPath).Exists -and (Get-TestPath).Value -eq '' -and (Get-TestPath).Kind -eq [Microsoft.Win32.RegistryValueKind]::String) 'Uninstall did not preserve an originally empty PATH.'
    Write-Output 'Installer smoke checks passed: install/reinstall, CLI scan, PATH ownership/types, opt-out, uninstall and preserved user files.'
} finally {
    $env:PATH = $oldProcessPath
    $env:DUP_REMOVER_TEST_INSTALL_DIRECTORY = $oldInstallTestVariable
    try { Uninstall-TestApp } finally {
        if ($testRegistryRoot -match '^Software\\Lledely\\dup-remover\\InstallerTests\\[0-9a-f]{32}$') {
            [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($testRegistryRoot, $false)
        }
        Pop-Location
    }
}
