[CmdletBinding()]
param(
    [switch]$SmokeTest
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$projectRoot = Split-Path $PSScriptRoot -Parent
Push-Location $projectRoot

try {
    $versionLine = Select-String -Path Cargo.toml -Pattern '^version = "([0-9]+\.[0-9]+\.[0-9]+)"$' | Select-Object -First 1
    if (-not $versionLine) { throw 'Cargo.toml must contain a numeric package version.' }
    $version = $versionLine.Matches[0].Groups[1].Value
    if ($env:CONSOLE_LINK_VERSION -and $env:CONSOLE_LINK_VERSION -ne $version) {
        throw 'Requested package version does not match Cargo.toml.'
    }

    # Ship the CRT with the executable rather than requiring a machine-wide VC runtime installer.
    $previousRustFlags = $env:RUSTFLAGS
    try {
        $env:RUSTFLAGS = "$previousRustFlags -C target-feature=+crt-static".Trim()
        & cargo build --locked --release --target x86_64-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) { throw 'Windows release build failed.' }
    } finally {
        $env:RUSTFLAGS = $previousRustFlags
    }

    $output = Join-Path $projectRoot 'dist'
    $portable = Join-Path $output 'windows-portable'
    New-Item -ItemType Directory -Force $portable | Out-Null
    Copy-Item 'target/x86_64-pc-windows-msvc/release/console-link.exe' (Join-Path $portable 'console-link.exe') -Force
    foreach ($name in @('LICENSE', 'README.md', 'THIRD_PARTY.md', 'THIRD_PARTY_LICENSES.txt')) {
        Copy-Item $name (Join-Path $portable $name) -Force
    }

    $dumpbin = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
    $dumpbinPath = if ($dumpbin) { $dumpbin.Source } else { $null }
    if (-not $dumpbinPath) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        if (Test-Path -LiteralPath $vswhere -PathType Leaf) {
            $dumpbinPath = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe' | Select-Object -First 1
        }
    }
    if (-not $dumpbinPath) { throw 'MSVC dumpbin.exe is required to verify the standalone executable.' }
    $dependencies = & $dumpbinPath /nologo /dependents (Join-Path $portable 'console-link.exe')
    if ($LASTEXITCODE -ne 0) { throw 'Unable to inspect Windows PE imports.' }
    if ($dependencies -match '(?i)\b(?:vcruntime|msvcp|concrt)[a-z0-9_]*\.dll\b') {
        throw 'Release executable still imports a separately installed Visual C++ runtime DLL.'
    }
    $archive = Join-Path $output "console-link-$version-windows-x64.zip"
    Compress-Archive -Path (Join-Path $portable 'console-link.exe'), (Join-Path $portable 'LICENSE'), (Join-Path $portable 'README.md'), (Join-Path $portable 'THIRD_PARTY.md'), (Join-Path $portable 'THIRD_PARTY_LICENSES.txt') -DestinationPath $archive -Force

    $compiler = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    $compilerPath = if ($compiler) { $compiler.Source } else { Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe' }
    if (-not (Test-Path -LiteralPath $compilerPath -PathType Leaf)) {
        throw 'Inno Setup 6 is required. Install it from https://jrsoftware.org/isdl.php.'
    }
    & $compilerPath "/DAppVersion=$version" "/DSourceDir=$portable" "/DOutputDir=$output" 'packaging/windows.iss'
    if ($LASTEXITCODE -ne 0) { throw 'Inno Setup compilation failed.' }
    $installer = Join-Path $output "Console-Link-$version-Setup-x64.exe"
    if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) { throw 'Installer output was not created.' }

    if ($SmokeTest) {
        if (-not $env:RUNNER_TEMP) { throw 'Installer smoke test is restricted to the disposable CI RUNNER_TEMP directory.' }
        $installDir = Join-Path $env:RUNNER_TEMP ('Console Link smoke ' + [Guid]::NewGuid().ToString('N'))
        $installLog = Join-Path $env:RUNNER_TEMP 'console-link-install.log'
        $uninstallLog = Join-Path $env:RUNNER_TEMP 'console-link-uninstall.log'
        $installArguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/DIR=`"$installDir`"", "/LOG=`"$installLog`"")
        $installProcess = Start-Process -FilePath $installer -ArgumentList $installArguments -Wait -PassThru
        if ($installProcess.ExitCode -ne 0) { throw "Installer smoke test failed with exit code $($installProcess.ExitCode)." }
        $installedExe = Join-Path $installDir 'console-link.exe'
        $uninstaller = Join-Path $installDir 'unins000.exe'
        foreach ($file in @($installedExe, $uninstaller, (Join-Path $installDir 'THIRD_PARTY_LICENSES.txt'))) {
            if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw 'Installer did not deploy all required files.' }
        }
        if ((Get-FileHash -LiteralPath $installedExe -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath (Join-Path $portable 'console-link.exe') -Algorithm SHA256).Hash) {
            throw 'Installed executable does not match the packaged release binary.'
        }
        $deviceProbe = Start-Process -FilePath $installedExe -ArgumentList '--list-devices' -RedirectStandardOutput (Join-Path $env:RUNNER_TEMP 'console-link-devices.log') -RedirectStandardError (Join-Path $env:RUNNER_TEMP 'console-link-devices-errors.log') -PassThru
        if (-not $deviceProbe.WaitForExit(15000)) {
            Stop-Process -Id $deviceProbe.Id -Force -ErrorAction SilentlyContinue
            throw 'Installed executable device-enumeration check timed out after 15 seconds.'
        }
        if ($deviceProbe.ExitCode -ne 0) { throw 'Installed executable failed its device-enumeration startup check.' }
        $uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{7B77A76F-0FA4-455D-977A-DC05E0E5992B}_is1'
        $registration = Get-ItemProperty -LiteralPath $uninstallKey
        if ($registration.DisplayName -ne 'Console Link' -or $registration.DisplayVersion -ne $version) {
            throw 'Per-user uninstall registration has incorrect product metadata.'
        }
        $protocolKey = 'HKCU:\Software\Classes\console-link'
        $protocolCommand = (Get-Item -LiteralPath ($protocolKey + '\shell\open\command')).GetValue('')
        if ($protocolCommand -notlike ('*"' + $installedExe + '"*--return-from-discord*"%1"*')) {
            throw 'Protocol handler does not return to the installed application.'
        }
        if ($null -eq (Get-Item -LiteralPath $protocolKey).GetValue('URL Protocol')) {
            throw 'Protocol handler is not registered as a URL scheme.'
        }
        $startMenuShortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Console Link/Console Link.lnk'
        if (-not (Test-Path -LiteralPath $startMenuShortcut -PathType Leaf)) { throw 'Start menu shortcut was not created for the current user.' }

        # A repeated install must keep the same identity and preserve unrelated user files.
        $sentinel = Join-Path $installDir 'user-owned-smoke-sentinel.txt'
        $sentinelText = 'This file belongs to the installer smoke test, not the application package.'
        Set-Content -LiteralPath $sentinel -Value $sentinelText -NoNewline -Encoding utf8
        $repairProcess = Start-Process -FilePath $installer -ArgumentList $installArguments -Wait -PassThru
        if ($repairProcess.ExitCode -ne 0) { throw "Repeated installation failed with exit code $($repairProcess.ExitCode)." }
        if ((Get-Content -LiteralPath $sentinel -Raw) -cne $sentinelText) { throw 'Repeated installation changed an unrelated user file.' }
        $registration = Get-ItemProperty -LiteralPath $uninstallKey
        if ($registration.DisplayName -ne 'Console Link' -or $registration.DisplayVersion -ne $version) {
            throw 'Repeated installation changed the per-user uninstall identity.'
        }
        if ((Get-FileHash -LiteralPath $installedExe -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath (Join-Path $portable 'console-link.exe') -Algorithm SHA256).Hash) {
            throw 'Repeated installation did not preserve the packaged executable.'
        }
        $uninstallProcess = Start-Process -FilePath $uninstaller -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=`"$uninstallLog`"") -Wait -PassThru
        if ($uninstallProcess.ExitCode -ne 0) { throw "Uninstall smoke test failed with exit code $($uninstallProcess.ExitCode)." }
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        while ((Test-Path -LiteralPath $installedExe) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
        if (Test-Path -LiteralPath $installedExe) { throw 'Uninstaller left the application executable behind.' }
        foreach ($name in @('LICENSE', 'README.md', 'THIRD_PARTY.md', 'THIRD_PARTY_LICENSES.txt')) {
            if (Test-Path -LiteralPath (Join-Path $installDir $name)) { throw "Uninstaller left a managed file behind: $name" }
        }
        if (Test-Path -LiteralPath $uninstallKey) { throw 'Uninstaller left its per-user registration behind.' }
        if (Test-Path -LiteralPath $protocolKey) { throw 'Uninstaller left its URL protocol registration behind.' }
        if (Test-Path -LiteralPath $startMenuShortcut) { throw 'Uninstaller left its start menu shortcut behind.' }
        if ((Get-Content -LiteralPath $sentinel -Raw) -cne $sentinelText) { throw 'Uninstaller removed or changed an unrelated user file.' }
        Remove-Item -LiteralPath $sentinel
        if (-not (Get-ChildItem -LiteralPath $installDir -Force)) { Remove-Item -LiteralPath $installDir }
        Write-Output 'Installer smoke test passed: install, executable hash, license files, per-user registration, start menu shortcut, repeated install, user-file preservation, uninstall.'
    }

    Write-Output "Created $([IO.Path]::GetFileName($archive)) and $([IO.Path]::GetFileName($installer))."
} finally {
    Pop-Location
}
