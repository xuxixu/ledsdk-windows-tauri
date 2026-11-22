//! 运行时装配：配置 → 日志 → 清理 → SDK → 作业队列。
//!
//! 设备发现改为手动：界面「扫描」按钮触发一次搜索快照并写回配置，
//! 后台不再自动改写屏幕清单（SDK 的搜索服务仍然常驻，发送链路依赖它）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use log::{debug, info, warn};

use crate::config::ScreenRegistry;
use crate::host::SdkHost;
use crate::jobs::JobQueue;

/// 上传 API 的启动/运行状态，供窗口状态栏区分“SDK HTTP 服务”和“上传接口”。
#[derive(Default)]
pub struct UploadApiStatus {
    running: AtomicBool,
    error: Mutex<Option<String>>,
}

impl UploadApiStatus {
    pub fn running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn error(&self) -> Option<String> {
        self.error
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    pub fn mark_running(&self) {
        *self.error.lock().unwrap_or_else(|err| err.into_inner()) = None;
        self.running.store(true, Ordering::SeqCst);
    }

    pub fn mark_stopped(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn mark_failed(&self, error: impl Into<String>) {
        *self.error.lock().unwrap_or_else(|err| err.into_inner()) = Some(error.into());
        self.running.store(false, Ordering::SeqCst);
    }
}

pub struct Runtime {
    pub registry: Arc<ScreenRegistry>,
    pub host: Arc<SdkHost>,
    pub jobs: JobQueue,
    pub config_path: PathBuf,
    pub log_directory: PathBuf,
    /// HTTP 服务优雅关闭信号：退出时先置位，serve() 停止接收新请求并排空在途请求。
    pub http_shutdown: Arc<AtomicBool>,
    pub upload_api_status: Arc<UploadApiStatus>,
}

impl Runtime {
    /// 开机链：日志 → 清理过期文件 → 自启同步 → SDK（HTTP 18080 + 常驻搜索）→ 作业队列。
    /// `write_registry`：正常启动为 true；自检/模拟等诊断模式传 false，避免只读诊断
    /// 反向改动部署机的 HKCU Run 自启项（C1）。
    pub fn start(
        config_path: &Path,
        log_directory: &Path,
        write_registry: bool,
    ) -> Result<Arc<Self>, String> {
        std::fs::create_dir_all(log_directory)
            .map_err(|err| format!("创建日志目录失败：{}：{err}", log_directory.display()))?;
        crate::logging::init(log_directory, log::LevelFilter::Info);

        let registry = Arc::new(ScreenRegistry::load(config_path));
        let settings = registry.sdk();
        settings.validate(&crate::config::base_dir_of(config_path))?;
        let current = registry.settings();
        current.upload_api.validate()?;

        info!(
            "运行路径：配置={} 日志={} 导出={} 上传暂存={} 转码缓存={}",
            config_path.display(),
            log_directory.display(),
            settings.export_root,
            settings.upload_root,
            settings.transcode_cache_root
        );

        let app_settings = current.app;
        crate::retention::clean(
            Path::new(&settings.export_root),
            Path::new(&settings.upload_root),
            log_directory,
            app_settings.retention_days,
        );

        // 作业对象隔离子进程对任何模式都安全；但诊断模式不碰注册表自启项。
        if crate::winos::assign_current_process() {
            info!("已启用子进程随主进程退出（Windows 作业对象）：崩溃或被强杀时 ffmpeg / TextRenderer 一并结束。");
        } else if cfg!(windows) {
            warn!("未能启用子进程随主进程退出（作业对象不可用）：异常退出时可能残留 ffmpeg / TextRenderer。");
        }
        if write_registry {
            crate::winos::sync_autostart(app_settings.auto_start);
            crate::winos::sync_desktop_shortcut(app_settings.desktop_shortcut);
        } else {
            debug!("诊断模式：跳过开机自启/桌面快捷方式同步。");
        }

        let host = Arc::new(SdkHost::new());
        host.start(&settings).map_err(|err| err.to_string())?;

        let jobs = JobQueue::spawn(host.clone(), registry.clone());

        Ok(Arc::new(Self {
            registry,
            host,
            jobs,
            config_path: config_path.to_path_buf(),
            log_directory: log_directory.to_path_buf(),
            http_shutdown: Arc::new(AtomicBool::new(false)),
            upload_api_status: Arc::new(UploadApiStatus::default()),
        }))
    }

    /// 界面只允许打开程序自己的目录：前端即使被注入，也不能借 explorer 打开任意路径。
    pub fn is_allowed_folder(&self, target: &std::path::Path) -> bool {
        let sdk = self.registry.sdk();
        let mut allowed = vec![
            std::path::PathBuf::from(&sdk.export_root),
            std::path::PathBuf::from(&sdk.upload_root),
            self.log_directory.clone(),
        ];
        if let Some(parent) = self.config_path.parent() {
            allowed.push(parent.to_path_buf());
        }
        // 不存在时 canonicalize 会失败，退回原路径；两边都按同样规则归一化再比较。
        let normalize = |path: &std::path::Path| {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        };
        let target = normalize(target);
        allowed.iter().any(|dir| target.starts_with(normalize(dir)))
    }

    /// 关闭链：等作业队列真的停下 → 再停 SDK（搜索/任务/HTTP）。
    /// 作业没停就销毁 SDK 会让它继续使用已释放的句柄，所以此时直接放弃清理，
    /// 由 8 秒 watchdog 强制退出（子进程由 Job Object 连带结束）。
    /// 返回 true 表示 SDK 已真正清理；false 表示超时放弃（交给 watchdog 强退）。
    pub fn stop(&self) -> bool {
        // 1) 先停 HTTP 接口：置位后 serve() 优雅关闭，不再接收新上传。
        //    原来 axum 跑在独立线程且无 shutdown 句柄，退出后 5 秒窗口内
        //    新 POST 仍会被受理、作业排在 Stop 之后永久 Queued。
        self.http_shutdown.store(true, Ordering::SeqCst);
        // 2) 等作业队列停下。
        if self.jobs.stop() {
            if self.host.shutdown() {
                info!("运行环境已关闭。");
                return true;
            }
            warn!("SDK 有任务未在限定时间内停止，放弃清理，交给 watchdog 强制退出。");
            return false;
        }
        // 3) 作业仍在跑（可能是 10 分钟的导出）：绝不能在 worker 仍使用句柄时并发
        //    shutdown/release。放弃清理，由 8 秒 watchdog 结束整个进程（作业对象会
        //    连带结束 ffmpeg/TextRenderer）。
        warn!("仍有作业在运行，放弃 SDK 清理，交给 watchdog 强制退出。");
        false
    }

    /// 手动扫描：取一次搜索快照，刷新自动字段并立即写回配置。返回（设备数，变更数）。
    pub fn scan_screens(&self) -> (usize, usize) {
        let devices = self.host.snapshot_devices();
        let mut changed = 0;
        for device in &devices {
            if self.registry.apply_discovery(device) {
                changed += 1;
            }
        }
        if changed > 0 {
            // 没有变化就不写盘，少一次磁盘写和一条日志噪音。
            self.registry.save_now();
        }
        info!(
            "手动扫描完成：搜索到 {} 台设备，{} 条清单更新。",
            devices.len(),
            changed
        );
        (devices.len(), changed)
    }

    pub fn log_tail(&self, lines: usize) -> Vec<String> {
        let now =
            time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
        // 常规日志与异常日志分开写（app.log / errors.log），面板要一起看；
        // 当天不够时再补前一天，避免刚过零点面板变空。
        let mut collected: Vec<String> = Vec::new();
        for day_offset in 0..2 {
            let day = now - time::Duration::days(day_offset);
            let directory = self.log_directory.join(format!(
                "{:04}-{:02}-{:02}",
                day.year(),
                day.month() as u8,
                day.day()
            ));
            for name in ["app.log", "errors.log"] {
                collected.extend(read_lines_tail(&directory.join(name), lines));
            }
            if collected.len() >= lines {
                break;
            }
        }
        // 按「记录块」合并：以时间戳开头的行是新记录，其余是上一条的续行
        // （PANIC 堆栈的线程/位置/backtrace 都无时间戳）。逐行排序会把续行
        // 打散到末尾再被 drain 砍掉，面板就看不到堆栈了。
        let mut blocks: Vec<(String, Vec<String>)> = Vec::new();
        for line in collected {
            if is_log_line_start(&line) || blocks.is_empty() {
                blocks.push((line.clone(), vec![line]));
            } else if let Some(last) = blocks.last_mut() {
                last.1.push(line);
            }
        }
        blocks.sort_by(|a, b| a.0.cmp(&b.0));
        let mut merged: Vec<String> = blocks.into_iter().flat_map(|(_, lines)| lines).collect();
        if merged.len() > lines {
            merged.drain(0..merged.len() - lines);
        }
        merged
    }
}

/// 判断一行是否是新日志记录的起点：以 `yyyy-MM-dd ` 开头。
fn is_log_line_start(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() >= 11
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b' '
        && bytes[..4].iter().all(u8::is_ascii_digit)
}

/// 读文件末尾最多 `want` 行；文件不存在或读取失败时返回空（UI 面板不该报错）。
fn read_lines_tail(path: &Path, want: usize) -> Vec<String> {
    let mut window = 256 * 1024usize;
    loop {
        match read_tail(path, window) {
            Ok((text, reached_start)) => {
                let mut all: Vec<&str> = text.lines().collect();
                if !reached_start && !all.is_empty() {
                    // 窗口起点可能切在一行中间，首行是残句，丢掉。
                    all.remove(0);
                }
                if reached_start || all.len() > want {
                    let start = all.len().saturating_sub(want);
                    return all[start..].iter().map(|line| line.to_string()).collect();
                }
            }
            Err(_) => return Vec::new(),
        }
        if window >= 16 * 1024 * 1024 {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let all: Vec<&str> = text.lines().collect();
            let start = all.len().saturating_sub(want);
            return all[start..].iter().map(|line| line.to_string()).collect();
        }
        window *= 4;
    }
}

/// 读文件末尾 `window` 字节；`reached_start` 表示窗口已覆盖整个文件。
fn read_tail(path: &Path, window: usize) -> std::io::Result<(String, bool)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let take = (window as u64).min(len);
    let reached_start = take == len;
    if take > 0 {
        file.seek(SeekFrom::Start(len - take))?;
    } else {
        file.seek(SeekFrom::Start(0))?;
    }
    let mut buffer = Vec::with_capacity(take as usize);
    file.read_to_end(&mut buffer)?;
    let text = String::from_utf8_lossy(&buffer).to_string();
    Ok((text, reached_start))
}
