param(
    [switch]$Uninstall,
    [switch]$NoDesktopShortcut,
    [switch]$NoStartMenuShortcut,
    [switch]$Quiet,
    [string]$TargetDir = ''
)

$ErrorActionPreference = 'Stop'

$target = if ($TargetDir) { $TargetDir } else { Join-Path $env:LOCALAPPDATA 'Programs\LedSdk' }
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
  .\setup.ps1 -Uninstall         卸载
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

if ($Uninstall) {
    if (-not $Quiet) {
        $answer = Read-Host "确认卸载 $target 并删除快捷方式？[y/N]"
        if ($answer -notmatch '^[yY]') {
            Write-Host "已取消卸载。"
            exit 0
        }
    }
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
    if (Test-Path $target) {
        Remove-Item -LiteralPath $target -Recurse -Force
    }
    Write-Host "已卸载 LedSdk。用户数据（导出/日志）保留在安装目录之外。"
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

New-Item -ItemType Directory -Force -Path $target | Out-Null
robocopy $source $target /E /XF setup.ps1 /NFL /NDL /NJH /NJS | Out-Null
if ($LASTEXITCODE -ge 8) { throw "复制安装文件失败（robocopy 退出码 $LASTEXITCODE）" }

$configPath = Join-Path $target 'config\appsettings.json'
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
