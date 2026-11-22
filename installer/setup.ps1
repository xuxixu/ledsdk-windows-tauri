param(
    [switch]$Uninstall,
    [switch]$NoDesktopShortcut,
    [switch]$NoStartMenuShortcut,
    [switch]$Quiet,
    [string]$TargetDir = '',
    [switch]$PurgeData
)

$ErrorActionPreference = 'Stop'

if ($TargetDir) {
    $candidate = $TargetDir.Replace('"', '').Trim()
    if ($candidate -match '^[A-Za-z]:$') {
        $candidate += '\'
    }
    if (-not [System.IO.Path]::IsPathRooted($candidate)) {
        throw "安装路径必须是绝对路径：$candidate"
    }
    $target = [System.IO.Path]::GetFullPath($candidate).TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar)
    if ($target -eq [System.IO.Path]::GetPathRoot($target).TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar)) {
        throw "不能直接安装到磁盘根目录，请选择子目录。"
    }
} else {
    $target = Join-Path $env:LOCALAPPDATA 'Programs\LedSdk'
}
if ([System.IO.File]::Exists($target)) {
    throw "安装路径已被文件占用：$target"
}
$windowsDir = [System.IO.Path]::GetFullPath($env:WINDIR).TrimEnd(
    [System.IO.Path]::DirectorySeparatorChar,
    [System.IO.Path]::AltDirectorySeparatorChar)
if ($target.Equals($windowsDir, [System.StringComparison]::OrdinalIgnoreCase) -or
    $target.StartsWith($windowsDir + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "不能安装到 Windows 系统目录，请选择其他目录。"
}
$exe = Join-Path $target 'ledsdk-tauri.exe'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'

function Show-Usage {
    Write-Host @"
LedSdk 安装程序

用法：
  .\setup.ps1                    交互式安装（默认创建桌面和开始菜单快捷方式）
  .\setup.ps1 -NoDesktopShortcut 安装但不创建桌面图标
  .\setup.ps1 -NoStartMenuShortcut 安装但不创建开始菜单图标
  .\setup.ps1 -Quiet             静默安装（使用默认选项）
  .\setup.ps1 -Uninstall         卸载（默认保留用户数据）
  .\setup.ps1 -Uninstall -PurgeData 卸载并删除用户数据
  .\setup.ps1 -TargetDir <目录>  安装到指定目录（默认 %LOCALAPPDATA%\Programs\LedSdk）
"@
}

function Remove-OwnedShortcut([string]$Path) {
    if (-not (Test-Path $Path)) { return }
    try {
        $shell = New-Object -ComObject WScript.Shell
        if ($shell.CreateShortcut($Path).TargetPath -eq $exe) {
            Remove-Item -LiteralPath $Path -Force
        }
    } catch { }
}

function Stop-RunningApp {
    param([switch]$Force)
    $running = @(Get-Process -Name 'ledsdk-tauri' -ErrorAction SilentlyContinue | Where-Object {
        try {
            [System.IO.Path]::GetFullPath($_.MainModule.FileName) -eq [System.IO.Path]::GetFullPath($exe)
        } catch {
            # 无法读取路径时按进程名处理；LedSdk 进程名唯一。
            $true
        }
    })
    if ($running.Count -eq 0) { return }
    if ($Force) {
        $running | Stop-Process -Force -ErrorAction SilentlyContinue
    } else {
        $answer = Read-Host "LedSdk 正在运行，是否强制停止？[y/N]"
        if ($answer -notmatch '^[yY]') {
            Write-Host "已取消。"
            exit 0
        }
        $running | Stop-Process -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Milliseconds 500
}

function Test-WebView2 {
    $keys = @(
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKCU:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    )
    foreach ($key in $keys) {
        if ((Get-ItemProperty -Path $key -Name pv -ErrorAction SilentlyContinue).pv) {
            return $true
        }
    }
    return $false
}

function Test-VCRuntime {
    return (Test-Path (Join-Path $env:WINDIR 'System32\msvcp120.dll')) -and (Test-Path (Join-Path $env:WINDIR 'System32\msvcr120.dll'))
}

if ($Uninstall) {
    if (-not $Quiet) {
        $answer = Read-Host "确认卸载 $target 并删除快捷方式？[y/N]"
        if ($answer -notmatch '^[yY]') {
            Write-Host "已取消卸载。"
            exit 0
        }
    }
    Stop-RunningApp -Force:$Quiet
    $desktop = [Environment]::GetFolderPath('Desktop')
    $startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
    foreach ($link in @(
        (Join-Path $desktop 'LedSdk.lnk'),
        (Join-Path $startMenu 'LedSdk.lnk')
    )) {
        Remove-OwnedShortcut $link
    }
    $runValue = (Get-ItemProperty -Path $runKey -Name 'LedSdkTauri' -ErrorAction SilentlyContinue).LedSdkTauri
    if ($runValue -and $runValue -like "*$exe*") {
        Remove-ItemProperty -Path $runKey -Name 'LedSdkTauri' -ErrorAction SilentlyContinue
    }
    Remove-Item -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\LedSdk' -Recurse -Force -ErrorAction SilentlyContinue
    $keep = @('config', 'export', 'logs', 'cache', 'upload')
    if (Test-Path $target) {
        if ($PurgeData) {
            Remove-Item -LiteralPath $target -Recurse -Force
        } else {
            foreach ($entry in Get-ChildItem -LiteralPath $target -Force) {
                if ($keep -contains $entry.Name) { continue }
                Remove-Item -LiteralPath $entry.FullName -Recurse -Force
            }
        }
    }
    $dataState = '已保留'
    if ($PurgeData) { $dataState = '已删除' }
    Write-Host "已卸载 LedSdk。用户数据（导出/日志/缓存/配置）$dataState。"
    exit 0
}

if (-not $Quiet) {
    Show-Usage
    $choice = Read-Host "创建桌面图标？[Y/n]"
    if ($choice -match '^[nN]') { $NoDesktopShortcut = $true }
    $choice = Read-Host "创建开始菜单图标？[Y/n]"
    if ($choice -match '^[nN]') { $NoStartMenuShortcut = $true }
}

$source = $PSScriptRoot

if (-not (Test-Path (Join-Path $source 'ledsdk-tauri.exe'))) {
    throw "安装包不完整：未找到 $source\ledsdk-tauri.exe"
}

if (-not [Environment]::Is64BitOperatingSystem) {
    throw "LedSdk 仅支持 64 位 Windows 10 / Windows 11。"
}
$osVersion = [Environment]::OSVersion.Version
if ($osVersion.Major -lt 10 -or ($osVersion.Major -eq 10 -and $osVersion.Build -lt 17763)) {
    throw "LedSdk 需要 Windows 10 1809 或更高版本（当前 $osVersion）。"
}
if (-not (Test-WebView2)) {
    throw "未检测到 Microsoft Edge WebView2 Runtime，请先安装：https://go.microsoft.com/fwlink/p/?LinkId=2124703"
}
if (-not (Test-VCRuntime)) {
    $redist = Join-Path $source 'vcredist_x64.exe'
    if (Test-Path $redist) {
        Write-Host "未检测到 VC++ 2013 x64 运行库，正在安装…"
        Start-Process -FilePath $redist -ArgumentList '/install','/quiet','/norestart' -Verb RunAs -Wait
    }
    if (-not (Test-VCRuntime)) {
        throw "VC++ 2013 x64 运行库未安装成功，请手动运行 $source\vcredist_x64.exe 后重试。"
    }
}

Stop-RunningApp -Force:$Quiet

New-Item -ItemType Directory -Force -Path $target | Out-Null
robocopy $source $target /E /XF setup.ps1 appsettings.json /NFL /NDL /NJH /NJS | Out-Null
if ($LASTEXITCODE -ge 8) { throw "复制安装文件失败（robocopy 退出码 $LASTEXITCODE）" }

$configSource = Join-Path $source 'config\appsettings.json'
$configPath = Join-Path $target 'config\appsettings.json'
if (-not (Test-Path $configPath)) {
    Copy-Item -LiteralPath $configSource -Destination $configPath -Force
    Write-Host "已写入默认配置：$configPath"
} else {
    Write-Host "保留现有配置：$configPath"
}
if (-not (Test-Path $configPath)) {
    throw "安装包不完整：未找到 config\appsettings.json"
}
$config = [System.IO.File]::ReadAllText($configPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
if (-not $config.App) {
    $config | Add-Member -NotePropertyName App -NotePropertyValue ([pscustomobject]@{})
}
# 旧配置可能没有这两个字段；PSCustomObject 必须用 Add-Member 新增，直接赋值会抛异常。
$config.App | Add-Member -NotePropertyName TrayIcon -NotePropertyValue $true -Force
$config.App | Add-Member -NotePropertyName DesktopShortcut -NotePropertyValue (-not $NoDesktopShortcut) -Force
$configJson = $config | ConvertTo-Json -Depth 10
# 显式写 UTF-8 无 BOM：Rust 端 serde_json 不接受 BOM。
[System.IO.File]::WriteAllText($configPath, $configJson, (New-Object System.Text.UTF8Encoding($false)))

function New-Shortcut([string]$Path) {
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($Path)
    $shortcut.TargetPath = $exe
    $shortcut.WorkingDirectory = $target
    $shortcut.IconLocation = $exe
    $shortcut.Description = 'LedSdk 屏幕节目上传服务'
    $shortcut.Save()
}

if (-not $NoDesktopShortcut) {
    New-Shortcut (Join-Path ([Environment]::GetFolderPath('Desktop')) 'LedSdk.lnk')
}
if (-not $NoStartMenuShortcut) {
    $startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
    New-Item -ItemType Directory -Force -Path $startMenu | Out-Null
    New-Shortcut (Join-Path $startMenu 'LedSdk.lnk')
}

if ($config.App.AutoStart) {
    New-Item -Path $runKey -Force | Out-Null
    Set-ItemProperty -Path $runKey -Name 'LedSdkTauri' -Value "`"$exe`""
} else {
    Remove-ItemProperty -Path $runKey -Name 'LedSdkTauri' -ErrorAction SilentlyContinue
}

Write-Host "已安装到：$target"
Write-Host "桌面图标：$(-not $NoDesktopShortcut)；开始菜单图标：$(-not $NoStartMenuShortcut)"
Write-Host "配置：$configPath"
