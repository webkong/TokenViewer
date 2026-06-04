$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$coreDir = Join-Path $root "core"
$windowsDir = Join-Path $root "windows/TokenViewer"
$distDir = Join-Path $root "dist/windows"
$releaseDir = Join-Path $root "dist/release"

$target = if ($env:RUST_TARGET) { $env:RUST_TARGET } else { "x86_64-pc-windows-msvc" }
$runtime = if ($env:WINDOWS_RUNTIME) { $env:WINDOWS_RUNTIME } else { "win-x64" }
$configuration = if ($env:CONFIGURATION) { $env:CONFIGURATION } else { "Release" }
$version = if ($env:VERSION) { $env:VERSION } else { "0.1.0" }

Write-Host "Building Rust core for $target..."
Push-Location $coreDir
cargo build --release --target $target
Pop-Location

Write-Host "Publishing WPF app for $runtime..."
New-Item -ItemType Directory -Force -Path $distDir | Out-Null
New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
Push-Location $windowsDir
dotnet publish TokenViewer.csproj `
  -c $configuration `
  -r $runtime `
  --self-contained false `
  -p:EnableWindowsTargeting=true `
  -p:CoreRustTarget=$target `
  -p:Version=$version `
  -p:AssemblyVersion=$version `
  -p:FileVersion=$version `
  -p:PublishSingleFile=false `
  -o $distDir
Pop-Location

$dllName = "tokenviewer_core.dll"
$builtDll = Join-Path $coreDir "target/$target/release/$dllName"
if (-not (Test-Path $builtDll)) {
  throw "Missing Rust DLL: $builtDll"
}

Copy-Item $builtDll (Join-Path $distDir $dllName) -Force

$zipPath = Join-Path $releaseDir "TokenViewer-Windows-$runtime.zip"
if (Test-Path $zipPath) {
  Remove-Item $zipPath -Force
}
Compress-Archive -Path (Join-Path $distDir "*") -DestinationPath $zipPath -Force

Write-Host "Output: $distDir"
Write-Host "Archive: $zipPath"
