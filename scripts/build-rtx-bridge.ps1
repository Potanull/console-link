[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$SdkDirectory,
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '../target/release'),
    [string]$Linker = 'link.exe'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$sdk = (Resolve-Path -LiteralPath $SdkDirectory).Path
$library = Join-Path $sdk 'lib/Windows/x64/nvsdk_ngx_s.lib'
$runtime = Join-Path $sdk 'bin/Windows/x64/rel/nvngx_vsr.dll'
$license = Join-Path $sdk 'NVIDIA_RTX_Video_SDK_License.pdf'
foreach ($file in @($library, $runtime, $license)) {
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Official RTX Video SDK file missing: $file" }
}
$linkerPath = (Get-Command $Linker -ErrorAction Stop).Source
$definition = Join-Path $PSScriptRoot '../native/rtx-video/console_link_ngx.def'
$buildDirectory = Join-Path ([IO.Path]::GetTempPath()) ('console-link-ngx-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $buildDirectory -Force | Out-Null
$dll = Join-Path $buildDirectory 'console_link_ngx.dll'
# Static CRT: the bridge must not require an additional VC runtime installation.
# Linker uses LIB from a VS x64 developer environment (VC CRT + Windows SDK).
& $linkerPath /NOLOGO /DLL /MACHINE:X64 /OPT:REF /OPT:ICF "/DEF:$definition" "/OUT:$dll" $library libcmt.lib libcpmt.lib libvcruntime.lib libucrt.lib kernel32.lib advapi32.lib user32.lib shell32.lib ole32.lib
if ($LASTEXITCODE -ne 0) { throw "RTX bridge link failed ($LASTEXITCODE). Use a VS x64 developer environment." }
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
Copy-Item -LiteralPath $dll -Destination $OutputDirectory -Force
Copy-Item -LiteralPath $runtime -Destination $OutputDirectory -Force
Copy-Item -LiteralPath $license -Destination $OutputDirectory -Force
Write-Output "RTX Video release runtime prepared in $OutputDirectory"
