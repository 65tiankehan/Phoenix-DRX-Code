# 打包 SPT Forge 上架用 zip（Forge 只收 .7z / .zip，不收 exe）
#
# 用法：
#   pwsh scripts/pack-forge.ps1 -Installer <*-installer.exe> -Version <0.2.4> [-OutDir <dir>]
#
# 产出：<OutDir>/Phoenix-DRX_<Version>_win-x64.zip
#   Phoenix-DRX/install-notes.txt                        安装说明（版本号与哈希已替换）
#   Phoenix-DRX/Phoenix-DRX_<Version>_x64-installer.exe
#
# 说明文件用 ASCII 文件名（install-notes.txt）而内容用 UTF-8 中文，
# 避免不同 zip 实现把中文条目名写成系统代码页而导致解压乱码。

[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Installer,
  [Parameter(Mandatory = $true)][string]$Version,
  [string]$OutDir = '.',
  [string]$NotesTemplate
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path $Installer)) { throw "找不到安装器: $Installer" }
$Installer = (Resolve-Path $Installer).Path
$version = $Version.TrimStart('v')

if (-not $NotesTemplate) {
  $NotesTemplate = Join-Path $PSScriptRoot 'forge-install-notes.txt'
}
if (-not (Test-Path $NotesTemplate)) { throw "找不到说明模板: $NotesTemplate" }

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path

$installerHash = (Get-FileHash $Installer -Algorithm SHA256).Hash.ToLower()
$exeName = "Phoenix-DRX_${version}_x64-installer.exe"

# 组合暂存目录（每次重建，避免上一版残留混进包里）
$stageRoot = Join-Path $OutDir "_forge-stage_$version"
if (Test-Path $stageRoot) { Remove-Item $stageRoot -Recurse -Force }
$stage = Join-Path $stageRoot 'Phoenix-DRX'
New-Item -ItemType Directory -Force -Path $stage | Out-Null

Copy-Item $Installer (Join-Path $stage $exeName)

$notes = Get-Content $NotesTemplate -Raw -Encoding UTF8
$notes = $notes.Replace('{{VERSION}}', $version).Replace('{{SHA256}}', $installerHash)
Set-Content -Path (Join-Path $stage 'install-notes.txt') -Value $notes -Encoding utf8NoBOM

$zip = Join-Path $OutDir "Phoenix-DRX_${version}_win-x64.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal

Remove-Item $stageRoot -Recurse -Force

# 自检：条目名与数量必须符合预期，避免产出坏包才发现
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead($zip)
try {
  $names = $archive.Entries | ForEach-Object { $_.FullName }
} finally {
  $archive.Dispose()
}
$expected = @("Phoenix-DRX/install-notes.txt", "Phoenix-DRX/$exeName")
foreach ($want in $expected) {
  if ($names -notcontains $want) { throw "zip 里缺少条目: $want（实际: $($names -join ', ')）" }
}

$zipHash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
$zipSize = (Get-Item $zip).Length

Write-Host ''
Write-Host 'Forge 上架包已生成'
Write-Host "  zip     : $zip"
Write-Host "  size    : $zipSize bytes"
Write-Host "  sha256  : $zipHash"
Write-Host "  安装器  : $exeName"
Write-Host "  安装器哈希: $installerHash"
Write-Host ''
Write-Host 'Forge 表单：Download Link 用 zip 的直链；VirusTotal 传这个 zip（不是 exe）。'

# 显式以 0 退出：点调用 .ps1 不会自动设置 $LASTEXITCODE（它会沿用上一条原生命令的退出码），
# 调用方若用 $LASTEXITCODE 判定，就会把「脚本其实成功了」误判成失败（2026-11-20 v0.2.5 CI 事故）。
exit 0
