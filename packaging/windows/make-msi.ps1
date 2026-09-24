# Build the Windows installer and a portable zip:
#   <dist>\jt-filework-<version>-windows-x64.msi
#   <dist>\jt-filework-<version>-windows-x64.zip
#
# Needs: Visual Studio's C++ toolset, Rust, CMake, NASM (aws-lc-sys assembles
# its own primitives for SFTP), Qt 6 for msvc2022_64 under C:\Qt, and WiX 5
# with its UI extension:
#   dotnet tool install --global wix --version 5.0.2
#   wix extension add --global WixToolset.UI.wixext/5.0.2
#
# Neither the installer nor the program is signed (docs/DISTRIBUTION.md 2):
# SmartScreen will warn once on another machine, and the release says so.
#
# Written for Windows PowerShell 5.1, which is what a stock machine has.
param(
  [string]$BuildRoot = (Join-Path $env:LOCALAPPDATA 'jt-filework-build')
)

# Continue, not Stop: cargo writes progress to stderr, and under Stop the
# first such line ends the script. Every native step checks $LASTEXITCODE.
$ErrorActionPreference = 'Continue'
function Fail($m) { Write-Output "FAILED: $m"; exit 1 }
function Step($m) { Write-Output ""; Write-Output "== $m" }

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$here = $PSScriptRoot
$version = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "([^"]+)"' |
  Select-Object -First 1).Matches[0].Groups[1].Value
if (-not $version) { Fail 'no version in Cargo.toml' }
$build = Join-Path $BuildRoot 'package'
$dist = Join-Path $BuildRoot 'dist'
$name = "jt-filework-$version-windows-x64"

# Found rather than assumed, so an upgrade of any of them does not break this.
# Only folders named as versions: an installer may also leave Tools there.
$qt = Get-ChildItem 'C:\Qt' -Directory -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -match '^\d+(\.\d+)+$' } |
  Sort-Object { [version]($_.Name -replace '[^0-9.]', '') } -Descending |
  ForEach-Object { Join-Path $_.FullName 'msvc2022_64' } |
  Where-Object { Test-Path (Join-Path $_ 'bin\windeployqt.exe') } |
  Select-Object -First 1
if (-not $qt) { Fail 'no Qt for msvc2022_64 under C:\Qt' }
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { Fail 'no Visual Studio C++ toolset' }
$wix = Get-Command wix -ErrorAction SilentlyContinue
if (-not $wix) { Fail 'no wix on PATH (dotnet tool install --global wix)' }

# The MSVC environment as vcvars64 leaves it, read back into this session.
cmd /c "`"$vs\VC\Auxiliary\Build\vcvars64.bat`" >nul 2>&1 && set" | ForEach-Object {
  if ($_ -match '^([^=]+)=(.*)$') {
    Set-Item -Path "env:$($matches[1])" -Value $matches[2] -ErrorAction SilentlyContinue
  }
}
$env:PATH = "$env:USERPROFILE\.cargo\bin;$qt\bin;C:\Program Files\NASM;$env:PATH"

Step "building $version with Qt at $qt"
$generator = if (Get-Command ninja -ErrorAction SilentlyContinue) { 'Ninja' } else { 'NMake Makefiles' }
cmake -S "$root\src\ui\qt6" -B $build -G $generator -DCMAKE_BUILD_TYPE=Release "-DCMAKE_PREFIX_PATH=$qt" | Out-Null
if ($LASTEXITCODE -ne 0) { Fail 'configure' }
cmake --build $build
if ($LASTEXITCODE -ne 0) { Fail 'build' }
$exe = Join-Path $build 'jt-filework.exe'
if (-not (Test-Path $exe)) { Fail "no executable at $exe" }
if (-not (Test-Path (Join-Path $build 'locales\en\main.catalog'))) {
  Fail 'no catalogue beside the executable; every label would show its key'
}
# The number the program shows is compiled into its Rust half, as plain ASCII.
# A build that reused an older library would install as one version - the
# installer and the resource stamp come from CMake - and call itself another.
$bytes = [IO.File]::ReadAllBytes($exe)
if (-not [Text.Encoding]::ASCII.GetString($bytes).Contains($version)) {
  Fail "the program was not built as $version; it would show another version"
}

Step 'staging'
$stage = Join-Path $build 'stage'
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $exe $stage
foreach ($data in 'locales', 'keymaps', 'icons', 'appicon') {
  $from = Join-Path $build $data
  if (Test-Path $from) { Copy-Item -Recurse $from (Join-Path $stage $data) }
}
Copy-Item (Join-Path $root 'LICENSE') (Join-Path $stage 'LICENSE.txt')

# Qt's libraries and plugins beside the program.
& "$qt\bin\windeployqt.exe" --release --no-translations --no-system-d3d-compiler `
  --no-opengl-sw (Join-Path $stage 'jt-filework.exe') | Out-Null
if ($LASTEXITCODE -ne 0) { Fail 'windeployqt' }

# The C++ runtime too, so the program starts on a machine that never had the
# Visual C++ redistributable installed. Beside the executable is a supported
# way to ship it, and needs no second installer.
$crt = Get-ChildItem (Join-Path $env:VCToolsRedistDir 'x64') -Directory -Filter 'Microsoft.VC*.CRT' |
  Select-Object -First 1
if (-not $crt) { Fail "no C runtime under $env:VCToolsRedistDir" }
Copy-Item (Join-Path $crt.FullName '*.dll') $stage

foreach ($needed in 'jt-filework.exe', 'Qt6Core.dll', 'Qt6Widgets.dll', 'Qt6Svg.dll',
                    'platforms\qwindows.dll', 'vcruntime140.dll', 'msvcp140.dll',
                    'locales\zh-TW\main.catalog', 'keymaps\single-key.keymap') {
  if (-not (Test-Path (Join-Path $stage $needed))) { Fail "the stage has no $needed" }
}

Step 'the licence as the installer shows it'
# WiX's licence page takes RTF. Plain text escaped, one paragraph per line.
$text = Get-Content -Raw (Join-Path $root 'LICENSE')
$escaped = $text.Replace('\', '\\').Replace('{', '\{').Replace('}', '\}')
$escaped = ($escaped -replace "`r`n", "`n") -replace "`n", "\par`r`n"
$rtf = "{\rtf1\ansi\deff0{\fonttbl{\f0 Consolas;}}\fs16`r`n$escaped}"
$licenseRtf = Join-Path $build 'License.rtf'
[IO.File]::WriteAllText($licenseRtf, $rtf, [Text.Encoding]::ASCII)

Step 'building the installer'
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$msi = Join-Path $dist "$name.msi"
if (Test-Path $msi) { Remove-Item -Force $msi }
$icon = Join-Path $root 'assets\icon\generated\jt-filework.ico'
if (-not (Test-Path $icon)) { Fail "no $icon (assets/icon/build-icons.sh)" }
wix build -arch x64 -ext WixToolset.UI.wixext `
  -d "Version=$version" -d "StageDir=$stage" -d "IconPath=$icon" -d "LicenseRtf=$licenseRtf" `
  -o $msi (Join-Path $here 'jt-filework.wxs')
if ($LASTEXITCODE -ne 0) { Fail 'wix build' }
# `wixpdb` is for debugging the installer, not for shipping.
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $dist "$name.wixpdb")

Step 'building the portable zip'
$zip = Join-Path $dist "$name.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
# Inside a folder of its own, so unpacking it does not scatter two hundred
# files into Downloads.
$portable = Join-Path $build $name
if (Test-Path $portable) { Remove-Item -Recurse -Force $portable }
Copy-Item -Recurse $stage $portable
# Every entry named here, with `/` between folders as the zip format
# requires. Windows PowerShell 5.1 writes `\` - both Compress-Archive and
# ZipFile.CreateFromDirectory, because it runs .NET in its old compatibility
# mode. Windows unpacks that anyway; macOS and Linux make files with
# backslashes in their names.
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$parent = Split-Path (Resolve-Path $portable).Path -Parent
$archive = [IO.Compression.ZipFile]::Open($zip, [IO.Compression.ZipArchiveMode]::Create)
try {
  Get-ChildItem -Recurse -File $portable | ForEach-Object {
    $entry = $_.FullName.Substring($parent.Length).TrimStart('\').Replace('\', '/')
    [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
      $archive, $_.FullName, $entry, [IO.Compression.CompressionLevel]::Optimal)
  }
} finally {
  $archive.Dispose()
}
if ((Get-Item $zip).Length -eq 0) { Fail 'the zip is empty' }
Remove-Item -Recurse -Force $portable

foreach ($file in $msi, $zip) {
  $hash = (Get-FileHash -Algorithm SHA256 $file).Hash.ToLower()
  $leaf = Split-Path $file -Leaf
  # Two spaces, as sha256sum writes it, so `sha256sum -c` reads it.
  [IO.File]::WriteAllText("$file.sha256", "$hash  $leaf`n", [Text.Encoding]::ASCII)
  Write-Output "$hash  $leaf"
}
Write-Output ""
Write-Output "built $msi"
Write-Output "built $zip"
