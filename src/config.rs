//! 配置（屏幕清单）读写。
//!
//! 配置文件是屏幕清单的持久化来源：字段为 PascalCase（与 C# 版 appsettings.json 兼容），
//! 相对路径一律以「配置文件所在目录」为基准解析。写盘只在扫描/编辑/删除等显式操作后发生。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use log::{error, info, warn};
use serde::{Deserialize, Serialize};

use crate::models::DeviceSnapshot;

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct AppSettings {
    pub sdk: SdkSettings,
    pub upload_api: UploadApiSettings,
    pub app: AppHostSettings,
    pub screens: Vec<ScreenProfile>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct SdkSettings {
    pub bin_directory: String,
    pub bind_address: String,
    pub download_port: u16,
    pub export_root: String,
    pub upload_root: String,
    pub transcode_cache_root: String,
    pub worker_count: u32,
    pub default_duration_seconds: i32,
    pub default_repeat_times: i32,
    pub send_retry_count: i32,
    pub send_retry_backoff_ms: i32,
    pub export_only: bool,
}

impl Default for SdkSettings {
    fn default() -> Self {
        Self {
            // `..` 相对配置文件目录（<exe>\config）→ exe 目录；build.rs 已把 sdk/x64 的文件复制到那里。
            bin_directory: "..".to_string(),
            bind_address: "0.0.0.0".to_string(),
            download_port: 18080,
            export_root: "../export".to_string(),
            upload_root: "../upload".to_string(),
            transcode_cache_root: "../cache".to_string(),
            worker_count: 2,
            default_duration_seconds: 5,
            default_repeat_times: 1,
            send_retry_count: 2,
            send_retry_backoff_ms: 500,
            export_only: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct UploadApiSettings {
    pub listen_address: String,
    pub port: u16,
    /// 可选共享 token：非空时上传/几何接口要求 `Authorization: Bearer <token>`。
    pub token: String,
}

impl Default for UploadApiSettings {
    fn default() -> Self {
        Self {
            listen_address: "0.0.0.0".to_string(),
            port: 5180,
            token: String::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct AppHostSettings {
    pub auto_start: bool,
    /// 显示系统托盘图标；关闭后窗口关闭即退出。
    pub tray_icon: bool,
    /// 在用户桌面创建/维护快捷方式；无快捷方式时按此开关补建。
    pub desktop_shortcut: bool,
    pub retention_days: i32,
}

impl Default for AppHostSettings {
    fn default() -> Self {
        Self {
            auto_start: false,
            tray_icon: true,
            desktop_shortcut: true,
            retention_days: 30,
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ScreenSource {
    #[default]
    Manual,
    Discovered,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct ScreenProfile {
    pub name: String,
    pub device_id: String,
    pub ip: String,
    pub port: u16,
    pub device_type: u32,
    pub width: i32,
    pub height: i32,
    pub rotate: i32,
    pub local_address: String,
    pub last_seen: Option<String>,
    pub online: bool,
    pub source: ScreenSource,
}

impl ScreenProfile {
    pub fn has_geometry(&self) -> bool {
        self.width > 0 && self.height > 0
    }
}

fn resolve_against(base: &Path, value: &str) -> String {
    let path = Path::new(value);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    joined.to_string_lossy().to_string()
}

impl SdkSettings {
    /// 把相对路径解析成绝对路径（相对基准 = 配置文件所在目录）。
    pub fn resolve_paths(&mut self, base: &Path) {
        self.bin_directory = resolve_against(base, &self.bin_directory);
        self.export_root = resolve_against(base, &self.export_root);
        self.upload_root = resolve_against(base, &self.upload_root);
        self.transcode_cache_root = resolve_against(base, &self.transcode_cache_root);
    }

    /// 启动前校验路径/端口，避免空路径把清理逻辑指向程序目录。
    pub fn validate(&self, config_directory: &Path) -> Result<(), String> {
        if self.download_port == 0 {
            return Err("Sdk.DownloadPort 必须在 1-65535 之间。".to_string());
        }
        if self.bind_address.trim().is_empty() {
            return Err("Sdk.BindAddress 不能为空。".to_string());
        }
        for (name, value) in [
            ("Sdk.BinDirectory", &self.bin_directory),
            ("Sdk.ExportRoot", &self.export_root),
            ("Sdk.UploadRoot", &self.upload_root),
            ("Sdk.TranscodeCacheRoot", &self.transcode_cache_root),
        ] {
            let path = Path::new(value);
            if value.trim().is_empty() || !path.is_absolute() {
                return Err(format!("{name} 必须是非空的绝对路径：{value:?}"));
            }
        }
        let config_dir = normalized(config_directory);
        let export = normalized(Path::new(&self.export_root));
        let upload = normalized(Path::new(&self.upload_root));
        let cache = normalized(Path::new(&self.transcode_cache_root));
        for (name, path) in [
            ("Sdk.ExportRoot", &export),
            ("Sdk.UploadRoot", &upload),
            ("Sdk.TranscodeCacheRoot", &cache),
        ] {
            if path == &config_dir {
                return Err(format!("{name} 不能指向配置目录：{}", path.display()));
            }
        }
        if export == upload || export == cache || upload == cache {
            return Err("ExportRoot / UploadRoot / TranscodeCacheRoot 必须互相独立。".to_string());
        }
        Ok(())
    }
}

impl UploadApiSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.port == 0 {
            return Err("UploadApi.Port 必须在 1-65535 之间。".to_string());
        }
        if self.listen_address.trim().is_empty() {
            return Err("UploadApi.ListenAddress 不能为空。".to_string());
        }
        Ok(())
    }
}

fn normalized(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn load_or_default(path: &Path) -> AppSettings {
    match read_settings(path) {
        Ok(settings) => settings,
        Err(_) if !path.exists() => default_for(path),
        Err(err) => {
            // 主文件损坏时优先恢复上一版 .bak，避免直接把屏幕清单清空。
            let backup = path.with_extension("json.bak");
            match read_settings(&backup) {
                Ok(settings) => {
                    warn!("配置文件解析失败，已从备份恢复：{path:?} → {backup:?}：{err}");
                    settings
                }
                Err(backup_err) => {
                    error!(
                        "配置文件与备份均无法解析，使用默认配置继续：{path:?}：{err}；备份：{backup:?}：{backup_err}"
                    );
                    default_for(path)
                }
            }
        }
    }
}

fn read_settings(path: &Path) -> Result<AppSettings, String> {
    let text = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let mut settings = serde_json::from_str::<AppSettings>(&text).map_err(|err| err.to_string())?;
    settings.sdk.resolve_paths(&base_dir_of(path));
    Ok(settings)
}

fn default_for(path: &Path) -> AppSettings {
    let mut settings = AppSettings::default();
    settings.sdk.resolve_paths(&base_dir_of(path));
    settings
}

/// 写盘串行化：UI 线程（扫描/编辑）与 HTTP 线程（几何补录）可能同时触发保存。
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 把绝对路径还原成相对配置目录的写法，避免迁移安装目录后失效。
fn relativize(base: &Path, value: &str) -> String {
    let target = Path::new(value);
    if !target.is_absolute() {
        return value.to_string();
    }
    let base_parts: Vec<_> = base.components().collect();
    let target_parts: Vec<_> = target.components().collect();
    // 盘符/根不一致时无法相对化，保留原样。
    if base_parts.first() != target_parts.first() {
        return value.to_string();
    }
    let mut common = 0;
    while common < base_parts.len()
        && common < target_parts.len()
        && base_parts[common] == target_parts[common]
    {
        common += 1;
    }
    let mut out = PathBuf::new();
    for _ in common..base_parts.len() {
        out.push("..");
    }
    for part in &target_parts[common..] {
        out.push(part.as_os_str());
    }
    if out.as_os_str().is_empty() {
        ".".to_string()
    } else {
        out.to_string_lossy().to_string()
    }
}

/// 原子写回：先写 `.tmp`，再 rename 覆盖（同卷 rename 是原子替换）。
pub fn save(path: &Path, settings: &AppSettings) -> io::Result<()> {
    // 并发写同一份配置会互相覆盖，先串行化。
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let base = base_dir_of(path);
    let mut serialized = settings.clone();
    serialized.sdk.bin_directory = relativize(&base, &settings.sdk.bin_directory);
    serialized.sdk.export_root = relativize(&base, &settings.sdk.export_root);
    serialized.sdk.upload_root = relativize(&base, &settings.sdk.upload_root);
    serialized.sdk.transcode_cache_root = relativize(&base, &settings.sdk.transcode_cache_root);
    let text = serde_json::to_string_pretty(&serialized)?;
    // 写之前先自检：反序列化不回去就不落盘，避免写出坏配置。
    serde_json::from_str::<AppSettings>(&text)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    // 临时文件名带进程号：多开实例时也不会互相踩。
    let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(&temp, text.as_bytes())?;
    if path.exists() {
        let _ = fs::copy(path, path.with_extension("json.bak"));
    }
    fs::rename(&temp, path)?;
    Ok(())
}

pub fn base_dir_of(path: &Path) -> PathBuf {
    path.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 屏幕清单：内存缓存 + 显式写回（扫描/编辑/删除/改几何后立即落盘）。
pub struct ScreenRegistry {
    path: PathBuf,
    inner: Mutex<AppSettings>,
}

impl ScreenRegistry {
    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let settings = load_or_default(&path);
        let count = settings.screens.len();
        info!("已加载屏幕清单 {}，共 {count} 台屏幕。", path.display());
        Self {
            path,
            inner: Mutex::new(settings),
        }
    }

    pub fn settings(&self) -> AppSettings {
        // 锁中毒（某线程持锁 panic）时恢复内部数据，而不是返回空默认：
        // 空默认会让屏幕清单"消失"、上传全 400，直到重启才发现。
        self.inner
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    pub fn sdk(&self) -> SdkSettings {
        self.settings().sdk
    }

    pub fn screens(&self) -> Vec<ScreenProfile> {
        self.settings().screens
    }

    pub fn resolve_by_device_id(&self, device_id: &str) -> Option<ScreenProfile> {
        let wanted = device_id.trim();
        if wanted.is_empty() {
            return None;
        }
        self.screens()
            .into_iter()
            .find(|s| s.device_id.eq_ignore_ascii_case(wanted))
    }

    /// 用搜索快照更新清单：自动字段刷新，人工字段（宽高/旋转）永不覆盖。
    /// 只改内存；由调用方（手动扫描）决定何时写盘。
    pub fn apply_discovery(&self, device: &DeviceSnapshot) -> bool {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        apply_locked(&mut guard, device)
    }

    pub fn set_geometry(&self, device_id: &str, width: i32, height: i32, rotate: i32) -> bool {
        if !valid_geometry(width, height, rotate) {
            warn!(
                "拒绝几何：设备={device_id} {width}x{height} 旋转={rotate}°（宽高必须为正，旋转只能是 0/90/180/270）"
            );
            return false;
        }
        {
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            let Some(entry) = guard
                .screens
                .iter_mut()
                .find(|s| s.device_id.eq_ignore_ascii_case(device_id.trim()))
            else {
                return false;
            };
            entry.width = width;
            entry.height = height;
            entry.rotate = rotate;
        }
        self.save_now();
        true
    }

    /// 用编辑界面提交的字段更新一条屏幕（DeviceId 不变）。
    #[allow(clippy::too_many_arguments)]
    pub fn update_screen(
        &self,
        device_id: &str,
        name: &str,
        ip: &str,
        port: u16,
        width: i32,
        height: i32,
        rotate: i32,
    ) -> Result<(), String> {
        if !valid_geometry(width, height, rotate) {
            warn!(
                "拒绝修改屏幕：{device_id} {width}x{height} 旋转={rotate}°（宽高必须为正，旋转只能是 0/90/180/270）"
            );
            return Err("宽度/高度必须大于 0，旋转只能是 0/90/180/270。".to_string());
        }
        {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| "配置被占用，请稍后重试。".to_string())?;
            let Some(entry) = guard
                .screens
                .iter_mut()
                .find(|s| matches(s, device_id, name))
            else {
                return Err("该条目已不在清单中。".to_string());
            };
            let previous = entry.name.clone();
            entry.name = name.trim().to_string();
            entry.ip = ip.trim().to_string();
            entry.port = port;
            entry.width = width;
            entry.height = height;
            entry.rotate = rotate;
            if entry.name != previous {
                info!("屏幕改名：{previous} → {}", entry.name);
            }
        }
        self.save_now();
        Ok(())
    }

    /// 删除一条屏幕；DeviceId 为空的人工条目按名称匹配。
    pub fn remove_screen(&self, device_id: &str, name: &str) -> Result<(), String> {
        {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| "配置被占用，请稍后重试。".to_string())?;
            let Some(index) = guard
                .screens
                .iter()
                .position(|s| matches(s, device_id, name))
            else {
                return Err("该条目已不在清单中。".to_string());
            };
            let removed = guard.screens.remove(index);
            info!(
                "已删除屏幕：{}（{}）{}:{}",
                removed.name, removed.device_id, removed.ip, removed.port
            );
        }
        self.save_now();
        Ok(())
    }

    /// 从磁盘重新读取配置（界面「重新加载配置文件」）。
    pub fn reload(&self) -> usize {
        let settings = load_or_default(&self.path);
        let count = settings.screens.len();
        if let Ok(mut guard) = self.inner.lock() {
            *guard = settings;
        }
        info!(
            "已从磁盘重新加载配置 {}，共 {count} 台屏幕。",
            self.path.display()
        );
        count
    }

    pub fn save_now(&self) {
        // 持锁写盘：避免「克隆旧状态 → 别人更新 → 自己写回」把更新冲掉。
        let guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        let count = guard.screens.len();
        match save(&self.path, &guard) {
            Ok(()) => info!("屏幕清单已写回 {}（{count} 台）。", self.path.display()),
            Err(err) => error!(
                "写回屏幕清单失败，原文件保持不变：{}：{err}",
                self.path.display()
            ),
        }
    }
}

/// 只接受正尺寸与 0/90/180/270 旋转；其余值 SDK 会拒绝，且错误信息很难懂。
fn valid_geometry(width: i32, height: i32, rotate: i32) -> bool {
    width > 0 && height > 0 && matches!(rotate, 0 | 90 | 180 | 270)
}

/// 编辑/删除时定位条目：有 DeviceId 按 DeviceId，没有（人工占位）按名称。
fn matches(profile: &ScreenProfile, device_id: &str, name: &str) -> bool {
    let wanted_id = device_id.trim();
    if !wanted_id.is_empty() {
        return profile.device_id.eq_ignore_ascii_case(wanted_id);
    }
    let wanted_name = name.trim();
    profile.device_id.trim().is_empty() && profile.name.eq_ignore_ascii_case(wanted_name)
}

fn apply_locked(settings: &mut AppSettings, device: &DeviceSnapshot) -> bool {
    let device_id = device.device_id.trim();
    if device_id.is_empty() {
        return false;
    }

    let index = settings
        .screens
        .iter()
        .position(|s| !s.device_id.is_empty() && s.device_id.eq_ignore_ascii_case(device_id))
        .or_else(|| {
            // 人工条目还没被搜索发现过（deviceId 为空）：按 IP + 端口认领。
            settings
                .screens
                .iter()
                .position(|s| s.device_id.is_empty() && s.ip == device.ip && s.port == device.port)
        });

    let now = crate::models::now_iso();

    match index {
        Some(index) => {
            let entry = &mut settings.screens[index];
            let mut changed = false;
            let old_ip = entry.ip.clone();
            let old_port = entry.port;
            let old_type = entry.device_type;

            if entry.device_id.is_empty() {
                entry.device_id = device_id.to_string();
                changed = true;
            }
            if entry.name.trim().is_empty() {
                entry.name = display_name(device);
                changed = true;
            }
            if entry.ip != device.ip || entry.port != device.port {
                entry.ip = device.ip.clone();
                entry.port = device.port;
                changed = true;
            }
            if entry.device_type != device.device_type {
                entry.device_type = device.device_type;
                changed = true;
            }
            if !device.local_address.is_empty() && entry.local_address != device.local_address {
                entry.local_address = device.local_address.clone();
                changed = true;
            }
            if entry.online != device.online {
                entry.online = device.online;
                changed = true;
            }
            entry.last_seen = Some(now);
            entry.source = ScreenSource::Discovered;

            if old_ip != entry.ip || old_port != entry.port {
                info!(
                    "设备 {device_id} 地址更新：{old_ip}:{old_port} → {}:{}（已写回配置）",
                    entry.ip, entry.port
                );
            }
            if old_type != entry.device_type {
                info!(
                    "设备 {device_id} 型号更新：{old_type} → {}（已写回配置）",
                    entry.device_type
                );
            }
            changed
        }
        None => {
            info!(
                "发现新屏幕 {}（{device_id}）{}:{} 型号={}",
                display_name(device),
                device.ip,
                device.port,
                device.device_type
            );
            settings.screens.push(ScreenProfile {
                name: display_name(device),
                device_id: device_id.to_string(),
                ip: device.ip.clone(),
                port: device.port,
                device_type: device.device_type,
                width: 0,
                height: 0,
                rotate: 0,
                local_address: device.local_address.clone(),
                last_seen: Some(now),
                online: device.online,
                source: ScreenSource::Discovered,
            });
            true
        }
    }
}

fn display_name(device: &DeviceSnapshot) -> String {
    let name = device.name.trim();
    if name.is_empty() {
        device.device_id.clone()
    } else {
        name.to_string()
    }
}
