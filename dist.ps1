# 一键构建发行版：cargo build --release，然后把 exe / 配置 / SDK 运行文件 / 说明拷到 dist\。
# 发版只跑这个脚本，避免 dist 里的二进制比 target\release 旧。
# 注意：不清理 dist 里的旧文件（删除需人工确认）；若上游文件改名，请手动清理残留。
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'cargo build --release 失败' }

$dist = Join-Path $PSScriptRoot 'dist'
$exe = Join-Path $PSScriptRoot 'target\release\ledsdk-tauri.exe'
$sdk = Join-Path $PSScriptRoot 'sdk\x64'
if (-not (Test-Path $exe)) { throw "找不到 $exe" }
if (-not (Test-Path $sdk)) { throw "找不到 $sdk" }

New-Item -ItemType Directory -Force -Path (Join-Path $dist 'config') | Out-Null
Copy-Item $exe $dist -Force
Copy-Item (Join-Path $PSScriptRoot 'config\appsettings.json') (Join-Path $dist 'config') -Force
Copy-Item (Join-Path $PSScriptRoot 'README.md') $dist -Force
# LedSDK.lib 只在编译期用，不进发行版。
# /XD logs 排除测试遗留的日志目录，避免发行版夹带内部测试路径。
robocopy $sdk $dist /E /XF LedSDK.lib /XD logs /NFL /NDL /NJH /NJS | Out-Null
# robocopy 用 0-7 表示成功（1=有文件复制、2=有额外文件…），>=8 才是失败；以前完全没检查。
if ($LASTEXITCODE -ge 8) { throw "robocopy 复制 SDK 失败（退出码 $LASTEXITCODE）" }

$total = (Get-ChildItem $dist -Recurse -File | Measure-Object Length -Sum).Sum
$stamp = (Get-Item $exe).LastWriteTime
Write-Host ("发行版已更新：{0}（共 {1:N0} 字节；exe 构建于 {2}）" -f $dist, $total, $stamp)
