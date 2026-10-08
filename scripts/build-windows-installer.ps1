[CmdletBinding()]
param(
    [string]$CompilerPath,
    [switch]$BootstrapCompiler,
    [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'The Windows installer must be built on Windows.' }
$repositoryRoot = Split-Path -Parent $PSScriptRoot
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repositoryRoot 'dist' }
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)

if (-not $CompilerPath) {
    $compilerCommand = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($compilerCommand) { $CompilerPath = $compilerCommand.Source }
    $compilerCandidates = @(
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 7\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 7\ISCC.exe')
    )
    foreach ($candidate in $compilerCandidates) {
        if (-not $CompilerPath -and (Test-Path -LiteralPath $candidate)) { $CompilerPath = $candidate }
    }
}

if (-not $CompilerPath -and $BootstrapCompiler) {
    $toolsDirectory = Join-Path $repositoryRoot 'target\installer-tools'
    New-Item -ItemType Directory -Path $toolsDirectory -Force | Out-Null
    $compilerSetup = Join-Path $toolsDirectory 'innosetup-6.7.3.exe'
    Invoke-WebRequest -Uri 'https://github.com/jrsoftware/issrc/releases/download/is-6_7_3/innosetup-6.7.3.exe' -OutFile $compilerSetup -UseBasicParsing
    $expectedHash = '9c73c3bae7ed48d44112a0f48e66742c00090bdb5bef71d9d3c056c66e97b732'
    if ((Get-FileHash -LiteralPath $compilerSetup -Algorithm SHA256).Hash -ne $expectedHash) {
        throw 'The Inno Setup download does not match the pinned SHA-256 checksum.'
    }
    $compilerDirectory = Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6'
    $setupArguments = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CURRENTUSER /NOICONS /TASKS="" /DIR="' + $compilerDirectory + '"'
    $setupProcess = Start-Process -FilePath $compilerSetup -ArgumentList $setupArguments -WindowStyle Hidden -Wait -PassThru
    if ($setupProcess.ExitCode -ne 0) { throw "Inno Setup installation failed: $($setupProcess.ExitCode)" }
    $CompilerPath = Join-Path $compilerDirectory 'ISCC.exe'
}
if (-not $CompilerPath -or -not (Test-Path -LiteralPath $CompilerPath)) {
    throw 'Inno Setup is required. Install Inno Setup 6/7, pass -CompilerPath, or use -BootstrapCompiler.'
}

Push-Location $repositoryRoot
$previousRustFlags = $env:RUSTFLAGS
$previousEncodedFlags = $env:CARGO_ENCODED_RUSTFLAGS
try {
    $metadataText = & cargo metadata --no-deps --locked --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
    $metadata = $metadataText | ConvertFrom-Json
    $package = @($metadata.packages | Where-Object name -EQ 'dup-remover')
    if ($package.Count -ne 1) { throw 'Expected one dup-remover package in Cargo metadata.' }
    $version = $package[0].version
    if ($version -notmatch '^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.+-]+)?$') { throw 'Unsupported package version.' }

    # --target keeps these flags off host-side proc macros. The target and its C
    # dependencies link their runtime statically, so VC++ Redistributable is not needed.
    if ($previousEncodedFlags) {
        $separator = [char]31
        $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedFlags + $separator + '-C' + $separator + 'target-feature=+crt-static'
    } else {
        $env:RUSTFLAGS = ($previousRustFlags + ' -C target-feature=+crt-static').Trim()
    }
    & cargo build --release --locked --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $binaryPath = Join-Path $metadata.target_directory 'x86_64-pc-windows-msvc\release\dup-remover.exe'
    if (-not (Test-Path -LiteralPath $binaryPath)) { throw 'Release binary was not produced.' }
    & $binaryPath --version
    if ($LASTEXITCODE -ne 0) { throw 'The packaged binary could not be started.' }

    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $compilerArguments = @(
        '/Qp', "/DAppVersion=$version", "/DBinaryPath=$binaryPath",
        "/DRepositoryRoot=$repositoryRoot", "/O$OutputDirectory",
        (Join-Path $repositoryRoot 'packaging\windows\installer.iss')
    )
    & $CompilerPath @compilerArguments
    if ($LASTEXITCODE -ne 0) { throw 'Inno Setup compilation failed.' }
    $installerPath = Join-Path $OutputDirectory "dup-remover-$version-windows-x64-setup.exe"
    $checksumPath = $installerPath + '.sha256'
    $checksum = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText($checksumPath, "$checksum  $([System.IO.Path]::GetFileName($installerPath))`n", (New-Object System.Text.UTF8Encoding($false)))
    Write-Output "Installer: $installerPath"
    Write-Output "SHA-256: $checksumPath"
} finally {
    $env:RUSTFLAGS = $previousRustFlags
    $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedFlags
    Pop-Location
}
