//! 作业流水线：建节目树 → 校验 → 导出 → 包校验/MD5 → 发送。

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use log::{info, warn};
use md5::{Digest, Md5};

use crate::config::{ScreenRegistry, ScreenProfile, SdkSettings};
use crate::ffi::{self, Handle};
use crate::host::{ExportRequest, SdkError, SdkHost, SendRequest};
use crate::models::DeviceSnapshot;
use crate::net;

const EXPORT_TIMEOUT: Duration = Duration::from_secs(600);
const SEND_TIMEOUT: Duration = Duration::from_secs(1800);

#[derive(Debug)]
pub enum JobError {
    Validation(String),
    Sdk(SdkError),
    Io(String),
}

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobError::Validation(text) => write!(f, "{text}"),
            JobError::Sdk(err) => write!(f, "{err}"),
            JobError::Io(text) => write!(f, "{text}"),
        }
    }
}

impl From<SdkError> for JobError {
    fn from(value: SdkError) -> Self {
        JobError::Sdk(value)
    }
}

pub struct JobResult {
    pub package_path: String,
    pub export_directory: String,
    pub device_id: String,
    pub sent: bool,
}

pub struct JobProcessor<'a> {
    host: &'a SdkHost,
    registry: &'a ScreenRegistry,
    settings: SdkSettings,
    devices: Vec<DeviceSnapshot>,
}

impl<'a> JobProcessor<'a> {
    pub fn new(
        host: &'a SdkHost,
        registry: &'a ScreenRegistry,
        settings: SdkSettings,
        devices: Vec<DeviceSnapshot>,
    ) -> Self {
        Self {
            host,
            registry,
            settings,
            devices,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &self,
        job_id: &str,
        request: &crate::models::UploadJobRequest,
        staged_file: &Path,
        mut report: impl FnMut(i32, &str),
    ) -> Result<JobResult, JobError> {
        let screen = self
            .registry
            .resolve_by_device_id(&request.device_id)
            .ok_or_else(|| {
                JobError::Validation(format!(
                    "设备 '{}' 不在配置清单中（可用 GET /api/screens 查看 deviceId）。",
                    request.device_id
                ))
            })?;
        if !screen.has_geometry() {
            return Err(JobError::Validation(format!(
                "设备 '{}' 还没有屏幕尺寸（宽/高为 0），请补录几何参数。",
                screen.name
            )));
        }

        let media_type = media_type_of(&request.file_type)?;
        let (x, y, width, height) = clamp_region(&screen, request)?;
        let text_content = if media_type == ffi::media_type::SINGLE_LINE_TEXT {
            Some(prepare_text(staged_file)?)
        } else {
            None
        };
        let media_type = if text_content.as_deref().is_some_and(|t| t.contains('\n')) {
            ffi::media_type::MULTI_LINE_TEXT
        } else {
            media_type
        };

        report(10, "解析完成，开始建节目树");
        let download_address = self.resolve_download_address(&screen)?;
        info!(
            "作业 {job_id}：下载地址 {download_address}，窗口 [{x},{y} {width}x{height}]"
        );

        let context = self.host.context();
        let mut playlist: Handle = ffi::NULL_HANDLE;

        let outcome = (|| -> Result<JobResult, JobError> {
            let mut playlist_out: Handle = ffi::NULL_HANDLE;
            let rc = unsafe {
                ffi::ledsdk_playlist_create(
                    context,
                    screen.width as u32,
                    screen.height as u32,
                    screen.device_type,
                    0,
                    &mut playlist_out,
                )
            };
            expect_ok(rc, "playlist_create")?;
            playlist = playlist_out;

            let description = CString::new(format!("job-{job_id}")).unwrap_or_default();
            let rc = unsafe {
                ffi::ledsdk_playlist_set_display_attributes(
                    playlist,
                    description.as_ptr(),
                    screen.rotate.max(0) as u32,
                    0,
                )
            };
            expect_ok(rc, "playlist_set_display_attributes")?;
            let rc = unsafe { ffi::ledsdk_playlist_set_sync_window(playlist, 0, 0, 0) };
            expect_ok(rc, "playlist_set_sync_window")?;

            let name = CString::new("upload").unwrap_or_default();
            let empty = CString::new("").unwrap_or_default();
            let mut program: Handle = ffi::NULL_HANDLE;
            let rc = unsafe { ffi::ledsdk_program_create(playlist, name.as_ptr(), empty.as_ptr(), &mut program) };
            expect_ok(rc, "program_create")?;

            let rule = ffi::init_versioned::<ffi::LedsdkPlayRule>(|r| {
                r.mode = ffi::play_mode::LOOP;
                r.count = 1;
                r.duration_seconds = 0;
            });
            let rc = unsafe { ffi::ledsdk_program_set_play_rule(program, &rule) };
            if let Err(err) = expect_ok(rc, "program_set_play_rule") {
                unsafe { ffi::ledsdk_program_release(program) };
                return Err(err);
            }
            let mut order = 0u32;
            let rc = unsafe { ffi::ledsdk_playlist_add_program(playlist, program, &rule, &mut order) };
            if let Err(err) = expect_ok(rc, "playlist_add_program") {
                unsafe { ffi::ledsdk_program_release(program) };
                return Err(err);
            }

            let mut region: Handle = ffi::NULL_HANDLE;
            let rc = unsafe {
                ffi::ledsdk_program_add_region(program, x, y, width as u32, height as u32, 0, &mut region)
            };
            expect_ok(rc, "program_add_region")?;
            let region_name = CString::new(format!("r-{job_id}")).unwrap_or_default();
            let rc = unsafe {
                ffi::ledsdk_region_set_geometry(region, x, y, width as u32, height as u32)
            };
            expect_ok(rc, "region_set_geometry")?;
            let rc = unsafe { ffi::ledsdk_region_set_attributes(region, region_name.as_ptr(), 255, 0) };
            expect_ok(rc, "region_set_attributes")?;

            let source_path = cstring(&staged_file.to_string_lossy(), "素材路径")?;
            let source_ptr = if text_content.is_some() {
                std::ptr::null()
            } else {
                source_path.as_ptr()
            };
            let mut media: Handle = ffi::NULL_HANDLE;
            let rc = unsafe { ffi::ledsdk_region_add_media(region, media_type, source_ptr, &mut media) };
            expect_ok(rc, "region_add_media")?;

            let media_name = CString::new(format!("m-{job_id}")).unwrap_or_default();
            let content = match text_content.as_deref() {
                Some(text) => Some(cstring(text, "文本内容")?),
                None => None,
            };
            let rc = unsafe {
                ffi::ledsdk_media_set_attributes(
                    media,
                    media_name.as_ptr(),
                    source_ptr,
                    content.as_ref().map(|c| c.as_ptr()).unwrap_or(std::ptr::null()),
                )
            };
            expect_ok(rc, "media_set_attributes")?;

            // 图片/文本按配置时长；GIF/视频给 -1 让导出用源文件时长。
            let (duration, repeat) = if matches!(
                media_type,
                ffi::media_type::PICTURE
                    | ffi::media_type::SINGLE_LINE_TEXT
                    | ffi::media_type::MULTI_LINE_TEXT
            ) {
                (self.settings.default_duration_seconds, self.settings.default_repeat_times.max(0) as u32)
            } else {
                (-1, 1)
            };
            let rc = unsafe { ffi::ledsdk_media_set_timing(media, duration, repeat, 0) };
            expect_ok(rc, "media_set_timing")?;
            let rc = unsafe {
                ffi::ledsdk_media_set_effects(
                    media,
                    ffi::effect::STATIC,
                    10,
                    ffi::effect::STATIC,
                    10,
                )
            };
            expect_ok(rc, "media_set_effects")?;
            let rc = unsafe { ffi::ledsdk_media_set_transform(media, 0, 0) };
            expect_ok(rc, "media_set_transform")?;

            if let Some(text) = content.as_ref() {
                let family = CString::new("Arial").unwrap_or_default();
                let point_size = ((height as f32 * 0.6) as i32).clamp(8, 200) as u32;
                let options = ffi::init_versioned::<ffi::LedsdkTextOptions>(|o| {
                    o.html_content_utf8 = text.as_ptr();
                    o.font.size = std::mem::size_of::<ffi::LedsdkFontOptions>() as u32;
                    o.font.version = ffi::STRUCT_VERSION;
                    o.font.family_utf8 = family.as_ptr();
                    o.font.point_size = point_size;
                    o.font.color_argb = 0xFFFF_FFFF;
                    o.color_mode = ffi::text_color::SINGLE;
                    o.alignment = ffi::text_align::HCENTER | ffi::text_align::VCENTER;
                });
                let rc = unsafe { ffi::ledsdk_media_set_text_options(media, &options) };
                expect_ok(rc, "media_set_text_options")?;
            }

            report(35, "节目树已建好，开始校验");
            let (code, message) = validate_playlist(playlist);
            if code != ffi::result::OK {
                return Err(JobError::Validation(format!(
                    "playlist 校验失败：result={code} {message}"
                )));
            }
            info!("作业 {job_id}：playlist 校验通过");

            report(50, "校验通过，开始导出");
            let started = SystemTime::now();
            let export = self.host.export_playlist(
                playlist,
                &ExportRequest {
                    output_root: self.settings.export_root.clone(),
                    download_port: self.settings.download_port,
                    download_address: download_address.clone(),
                    transcode_cache_root: self.settings.transcode_cache_root.clone(),
                },
                EXPORT_TIMEOUT,
            )?;
            if !export.succeeded() {
                return Err(JobError::Validation(format!(
                    "导出失败：state={} result={} {}",
                    export.state, export.result, export.message
                )));
            }

            let package_path = find_newest_package(Path::new(&self.settings.export_root), started)?;
            let bytes = std::fs::read(&package_path)
                .map_err(|err| JobError::Io(format!("读取节目包失败：{}：{err}", package_path.display())))?;
            let md5 = hex_upper(&bytes);
            info!(
                "作业 {job_id}：导出完成 {}，整文件 MD5={md5}",
                package_path.display()
            );
            verify_package(&bytes, &download_address, self.settings.download_port, &package_path)?;

            if self.settings.export_only {
                info!("作业 {job_id}：已按 ExportOnly 配置跳过发送（模拟/离线联调模式）。");
                return Ok(JobResult {
                    export_directory: package_path
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    package_path: package_path.to_string_lossy().to_string(),
                    device_id: String::new(),
                    sent: false,
                });
            }

            self.ensure_device_sendable(&screen)?;
            if !screen.online {
                warn!("设备 {} 在配置里标记为离线，仍尝试发送。", screen.device_id);
            }

            report(80, "开始发送到屏幕");
            let send = self.host.send_program(
                &screen.device_id,
                &package_path.to_string_lossy(),
                &SendRequest {
                    file_type: ffi::file_type::LIST,
                    retry_count: self.settings.send_retry_count,
                    retry_backoff_ms: self.settings.send_retry_backoff_ms,
                    md5_hex: md5.clone(),
                },
                SEND_TIMEOUT,
            )?;
            if !send.succeeded() {
                return Err(JobError::Validation(format!(
                    "发送失败：state={} result={} {}{}",
                    send.state,
                    send.result,
                    send.message,
                    send_failure_hint(&send.message, self.settings.download_port)
                )));
            }
            info!(
                "作业 {job_id}：发送成功，设备 {} 已下载节目包。",
                screen.device_id
            );

            Ok(JobResult {
                export_directory: package_path
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
                package_path: package_path.to_string_lossy().to_string(),
                device_id: screen.device_id.clone(),
                sent: true,
            })
        })();

        if !playlist.is_null() {
            // playlist 递归释放 program/region/media（加了 program 之后由 playlist 拥有）。
            unsafe { ffi::ledsdk_playlist_release(playlist) };
        }

        outcome
    }

    fn ensure_device_sendable(&self, screen: &ScreenProfile) -> Result<(), JobError> {
        let device_id = screen.device_id.as_str();
        if device_id.is_empty() {
            return Err(JobError::Validation(
                "该设备还没有 deviceId，无法发送。请确认设备已上电并与本机同网段，稍后重试。".to_string(),
            ));
        }
        match self
            .devices
            .iter()
            .find(|d| d.device_id.eq_ignore_ascii_case(device_id))
        {
            None => Err(JobError::Validation(format!(
                "设备 '{device_id}' 不在当前搜索快照中（可能离线或未上电），已中止发送。请确认设备在线后重试。"
            ))),
            Some(device) if !device.online => Err(JobError::Validation(format!(
                "设备 '{device_id}' 当前不在线（{}:{}），已中止发送。",
                device.ip, device.port
            ))),
            Some(_) => Ok(()),
        }
    }

    fn resolve_download_address(&self, screen: &ScreenProfile) -> Result<String, JobError> {
        let device = self
            .devices
            .iter()
            .find(|d| d.device_id.eq_ignore_ascii_case(&screen.device_id));

        if let Some(device) = device {
            if !device.local_address.is_empty() {
                return Ok(device.local_address.clone());
            }
        }
        if !screen.local_address.is_empty() {
            return Ok(screen.local_address.clone());
        }
        if let Some(device) = device {
            if let Some(address) = net::local_address_for(&device.ip, device.port) {
                return Ok(address.to_string());
            }
        }
        if !screen.ip.is_empty() {
            if let Some(address) = net::local_address_for(&screen.ip, screen.port) {
                return Ok(address.to_string());
            }
        }

        let candidates = net::usable_local_addresses();
        if candidates.len() == 1 {
            return Ok(candidates[0].to_string());
        }
        if self.settings.export_only {
            if let Some(first) = candidates.first() {
                warn!(
                    "无法确定设备侧命中网卡（共 {} 个候选），ExportOnly 模式下取 {} 仅用于生成下载 URL。",
                    candidates.len(),
                    first
                );
                return Ok(first.to_string());
            }
        }
        Err(JobError::Validation(format!(
            "无法为设备 '{}' 确定设备可访问的本机下载地址：本机有 {} 个候选网卡，无法判断该走哪一张。请确认设备已上电并与本机同网段，或在配置中为该屏幕显式指定 localAddress。",
            screen.name,
            candidates.len()
        )))
    }
}

fn expect_ok(rc: i32, operation: &str) -> Result<(), JobError> {
    if rc == ffi::result::OK || rc == ffi::result::IN_PROGRESS {
        Ok(())
    } else {
        Err(JobError::Sdk(SdkError::new(rc, operation)))
    }
}

fn media_type_of(file_type: &str) -> Result<u32, JobError> {
    match file_type.trim().to_ascii_lowercase().as_str() {
        "picture" | "image" => Ok(ffi::media_type::PICTURE),
        "gif" => Ok(ffi::media_type::GIF),
        "video" => Ok(ffi::media_type::VIDEO),
        "text" => Ok(ffi::media_type::SINGLE_LINE_TEXT),
        other => Err(JobError::Validation(format!(
            "不支持的 fileType '{other}'（可用 picture / gif / video / text）。"
        ))),
    }
}

fn clamp_region(
    screen: &ScreenProfile,
    request: &crate::models::UploadJobRequest,
) -> Result<(i32, i32, i32, i32), JobError> {
    let (x, y) = (request.x, request.y);
    let (width, height) = (request.width, request.height);
    if x < 0 || y < 0 || width <= 0 || height <= 0 {
        return Err(JobError::Validation("x/y 不能为负，width/height 必须大于 0。".to_string()));
    }
    if x >= screen.width || y >= screen.height {
        return Err(JobError::Validation(format!(
            "窗口起点 ({x},{y}) 超出屏幕范围 {}x{}。",
            screen.width, screen.height
        )));
    }
    let new_width = width.min(screen.width - x);
    let new_height = height.min(screen.height - y);
    if new_width != width || new_height != height {
        warn!(
            "素材窗口 [{x},{y} {width}x{height}] 超出屏幕 {}x{}，已裁剪为 {new_width}x{new_height}。",
            screen.width, screen.height
        );
    }
    Ok((x, y, new_width, new_height))
}

/// 文本素材上限：这档大小足够整屏滚动文案，又不会把内存吃穿。
const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;

/// 含内嵌 NUL 的字符串无法交给 C：明确报错，而不是静默变成空串。
fn cstring(value: &str, field: &str) -> Result<CString, JobError> {
    CString::new(value)
        .map_err(|_| JobError::Validation(format!("{field} 包含 NUL 字符，无法传递给 SDK。")))
}

fn prepare_text(path: &Path) -> Result<String, JobError> {
    let metadata = std::fs::metadata(path)
        .map_err(|err| JobError::Io(format!("读取文本失败：{}：{err}", path.display())))?;
    if metadata.len() as usize > MAX_TEXT_BYTES {
        return Err(JobError::Validation(format!(
            "文本素材过大（{} KB，上限 {} KB）。",
            metadata.len() / 1024,
            MAX_TEXT_BYTES / 1024
        )));
    }
    let raw = std::fs::read(path)
        .map_err(|err| JobError::Io(format!("读取文本失败：{}：{err}", path.display())))?;
    let text = String::from_utf8(raw)
        .map_err(|_| JobError::Validation("文本素材不是合法 UTF-8。".to_string()))?;
    let trimmed = text.trim_end_matches(['\r', '\n']);
    if trimmed.trim().is_empty() {
        return Err(JobError::Validation("文本素材内容为空。".to_string()));
    }
    Ok(trimmed.to_string())
}

fn validate_playlist(playlist: Handle) -> (i32, String) {
    let mut required = 0u32;
    let mut buffer = vec![0u8; 1024];
    let mut rc = unsafe {
        ffi::ledsdk_playlist_validate(
            playlist,
            buffer.as_mut_ptr() as *mut i8,
            buffer.len() as u32,
            &mut required,
        )
    };
    if rc == ffi::result::BUFFER_TOO_SMALL && required > buffer.len() as u32 {
        buffer = vec![0u8; required as usize];
        rc = unsafe {
            ffi::ledsdk_playlist_validate(
                playlist,
                buffer.as_mut_ptr() as *mut i8,
                buffer.len() as u32,
                &mut required,
            )
        };
    }
    let end = buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len());
    (rc, String::from_utf8_lossy(&buffer[..end]).into_owned())
}

fn find_newest_package(root: &Path, started: SystemTime) -> Result<PathBuf, JobError> {
    let entries = std::fs::read_dir(root)
        .map_err(|err| JobError::Io(format!("读取导出目录失败：{}：{err}", root.display())))?;
    let floor = started - Duration::from_secs(2);
    let mut fresh: Option<(SystemTime, PathBuf)> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("zhl") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if modified >= floor && fresh.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
            fresh = Some((modified, path));
        }
    }

    // 只认本次导出之后产生的包：找不到就报错，绝不退回「目录里最新的那个」——
    // 那可能是上一单的节目，发错内容比起失败更难排查。
    fresh
        .map(|(_, path)| path)
        .ok_or_else(|| {
            JobError::Io(format!(
                "导出完成但没有找到新的 .zhl（{} 秒内）：{}",
                floor.elapsed().map(|d| d.as_secs()).unwrap_or(0),
                root.display()
            ))
        })
}

pub fn hex_upper(bytes: &[u8]) -> String {
    let digest = Md5::digest(bytes);
    digest.iter().map(|b| format!("{b:02X}")).collect()
}

/// 发送前核对包内下载地址（对应原项目 enqueuePackage 的检查）。
///
/// 必须按 `host:port` 精确比对：子串匹配会让 `192.168.1.1` 误匹配 `192.168.1.10`，
/// 也完全看不到端口错误。
pub fn verify_package(
    bytes: &[u8],
    download_address: &str,
    download_port: u16,
    package_path: &Path,
) -> Result<(), JobError> {
    let text = String::from_utf8_lossy(bytes);
    let authorities = http_authorities(&text);
    let mut bad = false;
    let mut matched = false;
    for authority in &authorities {
        let (host, port) = split_authority(authority);
        // 回环/通配地址一律拒绝：包括 IPv6 的 ::1 / :: 与 localhost（大小写不敏感），
        // 否则 split_authority 去掉方括号后 http://[::1]:18080 的 host=::1 会绕过防线。
        let lower = host.to_ascii_lowercase();
        if host.is_empty()
            || lower == "127.0.0.1"
            || lower == "0.0.0.0"
            || lower == "::1"
            || lower == "::"
            || lower == "localhost"
        {
            bad = true;
        }
        if !download_address.is_empty()
            && host.eq_ignore_ascii_case(download_address)
            && port == download_port
        {
            matched = true;
        }
    }
    if !bad && matched {
        return Ok(());
    }

    Err(JobError::Validation(format!(
        "节目包 {} 内的下载地址与当前不一致（包内={}，当前={download_address}:{download_port}），已中止发送。请重新上传以重新导出。",
        package_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        authorities
            .first()
            .cloned()
            .unwrap_or_else(|| "（未找到 http:// 地址）".to_string())
    )))
}

/// 取出文本里所有 `http://host[:port]` 的 authority 段。
fn http_authorities(text: &str) -> Vec<String> {
    const SCHEME: &str = "http://";
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(SCHEME) {
        let after = &rest[start + SCHEME.len()..];
        let end = after
            .find(|c: char| c == '/' || c == '\\' || c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(after.len());
        out.push(after[..end].to_string());
        rest = &after[end..];
        if rest.is_empty() {
            break;
        }
    }
    out
}

/// 拆出 `host` 与端口；没有端口时按 http 默认 80 处理。
fn split_authority(authority: &str) -> (String, u16) {
    if let Some((host, port)) = authority.rsplit_once(':') {
        if let Ok(port) = port.parse::<u16>() {
            return (host.trim_matches(['[', ']']).to_string(), port);
        }
    }
    (authority.trim_matches(['[', ']']).to_string(), 80)
}

pub fn send_failure_hint(message: &str, download_port: u16) -> String {
    let lower = message.to_ascii_lowercase();
    if !(lower.contains("timeout") || lower.contains("timed out")) {
        return String::new();
    }
    format!(
        " 设备已收到发送指令但未完成下载。请检查：1) Windows 防火墙是否允许本程序 TCP {download_port} 入站；2) 下载地址是否为设备同网段的本机 IPv4；3) HTTP 文件服务根目录是否与导出目录一致。"
    )
}


