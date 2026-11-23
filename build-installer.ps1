# 构建单文件安装包：把 dist\ 的运行文件压成 payload，再编译进 LedSdk-Setup-<version>.exe。
# 依赖 Windows 自带的 .NET Framework 4.8（csc.exe），不引入第三方安装器工具。
param(
    [string]$Version = '1.0.4'
)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$dist = Join-Path $PSScriptRoot 'dist'
$exe = Join-Path $dist 'ledsdk-tauri.exe'
$setup = Join-Path $dist 'setup.ps1'
$config = Join-Path $dist 'config\appsettings.json'
$source = Join-Path $PSScriptRoot 'installer\LedSdkSetup.cs'
$uninstallerSource = Join-Path $PSScriptRoot 'installer\LedSdkUninstaller.cs'
$icon = Join-Path $PSScriptRoot 'icons\icon.ico'
$csc = 'C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe'
$out = Join-Path $dist ("LedSdk-Setup-{0}.exe" -f $Version)

foreach ($path in @($exe, $setup, $config, $source, $uninstallerSource, $icon, $csc)) {
    if (-not (Test-Path $path)) { throw "缺少打包文件：$path" }
}

$work = Join-Path $PSScriptRoot 'target\installer'
New-Item -ItemType Directory -Force -Path $work | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$zip = Join-Path $work ("payload-{0}-{1}.zip" -f $stamp, $PID)

# 0) 先编译卸载程序，随负载一起安装到目标目录。
$framework = Split-Path -Parent $csc
$references = @(
    (Join-Path $framework 'System.dll'),
    (Join-Path $framework 'System.Core.dll'),
    (Join-Path $framework 'System.Drawing.dll'),
    (Join-Path $framework 'System.Windows.Forms.dll'),
    (Join-Path $framework 'System.IO.Compression.dll'),
    (Join-Path $framework 'System.IO.Compression.FileSystem.dll')
)
foreach ($reference in $references) {
    if (-not (Test-Path $reference)) { throw "缺少 .NET Framework 引用：$reference" }
}

$uninstallerExe = Join-Path $work 'uninstall.exe'
$uninstallerArgs = @(
    '/nologo',
    '/codepage:65001',
    '/target:winexe',
    '/platform:anycpu',
    '/optimize+',
    ("/win32icon:{0}" -f $icon),
    ("/out:{0}" -f $uninstallerExe)
)
foreach ($reference in $references) {
    $uninstallerArgs += ("/reference:{0}" -f $reference)
}
$uninstallerArgs += $uninstallerSource
& $csc $uninstallerArgs
if ($LASTEXITCODE -ne 0) { throw "卸载程序编译失败（退出码 $LASTEXITCODE）" }
Copy-Item $uninstallerExe (Join-Path $dist 'uninstall.exe') -Force

# 1) 生成干净负载：排除运行残留、旧安装包与配置备份，避免把现场数据打进安装器。
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::Open($zip, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    $excludeDirs = @('logs', 'export', 'cache', 'upload', 'verify', 'installer')
    $excludeFiles = @('config\appsettings.json.bak')
    foreach ($file in Get-ChildItem $dist -Recurse -File) {
        $relative = $file.FullName.Substring($dist.Length + 1)
        $skip = $false
        foreach ($dir in $excludeDirs) {
            if ($relative.StartsWith($dir + '\')) { $skip = $true; break }
        }
        if ($excludeFiles -contains $relative) { $skip = $true }
        if ($file.Name -like 'LedSdk-Setup-*.exe') { $skip = $true }
        if ($skip) { continue }

        $entryName = $relative.Replace('\', '/')
        [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
            $archive,
            $file.FullName,
            $entryName,
            [System.IO.Compression.CompressionLevel]::Optimal) | Out-Null
    }
} finally {
    $archive.Dispose()
}

$zipInfo = Get-Item $zip
Write-Host ("安装负载：{0}（{1:N0} 字节）" -f $zipInfo.Name, $zipInfo.Length)

# 2) 编译安装程序，把负载作为内嵌资源写入单文件 EXE。
$cscArgs = @(
    '/nologo',
    '/codepage:65001',
    '/target:winexe',
    '/platform:anycpu',
    '/optimize+',
    ("/win32icon:{0}" -f $icon),
    ("/resource:{0},LedSdkPayload.zip" -f $zip),
    ("/out:{0}" -f $out)
)
foreach ($reference in $references) {
    if (-not (Test-Path $reference)) { throw "缺少 .NET Framework 引用：$reference" }
    $cscArgs += ("/reference:{0}" -f $reference)
}
$cscArgs += $source

& $csc $cscArgs
if ($LASTEXITCODE -ne 0) { throw "csc 编译失败（退出码 $LASTEXITCODE）" }

$outInfo = Get-Item $out
$hash = (Get-FileHash $out -Algorithm SHA256).Hash
Write-Host ("安装包已生成：{0}（{1:N0} 字节，SHA256={2}）" -f $outInfo.FullName, $outInfo.Length, $hash)
