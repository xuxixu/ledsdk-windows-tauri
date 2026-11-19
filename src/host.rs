//! SDK 宿主：初始化、HTTP 文件服务、常驻搜索、任务提交与有序关闭。

use std::ffi::{c_char, c_void, CStr, CString};
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};

use crate::config::SdkSettings;
use crate::ffi::{self, Handle};
use crate::models::{DeviceSnapshot, Outcome};

pub struct SdkError {
    pub result: i32,
    pub operation: String,
    pub detail: String,
}

impl SdkError {
    pub fn new(result: i32, operation: impl Into<String>) -> Self {
        Self {
            result,
            operation: operation.into(),
            detail: String::new(),
        }
    }

    pub fn with_detail(
        result: i32,
        operation: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            result,
            operation: operation.into(),
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for SdkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} 失败：result={} ({})",
            self.operation,
            self.result,
            ffi::result_message(self.result)
        )?;
        if !self.detail.is_empty() {
            write!(f, "，{}", self.detail)?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for SdkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

#[derive(Clone, Debug)]
pub struct ExportRequest {
    pub output_root: String,
    pub download_port: u16,
    pub download_address: String,
    pub transcode_cache_root: String,
}

#[derive(Clone, Debug)]
pub struct SendRequest {
    pub file_type: u32,
    pub retry_count: i32,
    pub retry_backoff_ms: i32,
    pub md5_hex: String,
}

struct Inner {
    context: Handle,
    http: Handle,
    search: Handle,
    http_running: bool,
    http_port: u16,
    version: String,
}

pub struct SdkHost {
    inner: Mutex<Inner>,
    /// 关闭开始后置位：此后再提交任务 / 取搜索快照一律直接失败，避免调用已释放的句柄。
    shutting_down: AtomicBool,
    /// SDK 已完整释放；用于 shutdown 幂等与重复调用等待。
    closed: AtomicBool,
}

/// 含内嵌 NUL 的字符串无法交给 C：明确报错，而不是静默降级成空串。
fn cstring_arg(value: &str, field: &str) -> Result<CString, SdkError> {
    CString::new(value).map_err(|_| {
        SdkError::with_detail(
            ffi::result::INVALID_ARGUMENT,
            "参数校验",
            format!("{field} 包含 NUL 字符，无法传递给 SDK"),
        )
    })
}

// 句柄本身是 SDK 内部对象指针，跨线程使用由 SDK 的「不同任务可并发、同句柄不并发修改」规则约束。
unsafe impl Send for SdkHost {}
unsafe impl Sync for SdkHost {}

impl SdkHost {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                context: ffi::NULL_HANDLE,
                http: ffi::NULL_HANDLE,
                search: ffi::NULL_HANDLE,
                http_running: false,
                http_port: 0,
                version: String::new(),
            }),
            shutting_down: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        }
    }

    fn ensure_open(&self) -> Result<(), SdkError> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(SdkError::with_detail(
                ffi::result::INVALID_STATE,
                "提交任务",
                "程序正在关闭，不再接受新的任务",
            ));
        }
        Ok(())
    }

    /// 启动失败时回滚已创建的 context / http / search。
    fn abort_start(&self, err: SdkError) -> SdkError {
        warn!("启动 SDK 失败，回滚已创建的资源：{err}");
        self.shutdown();
        err
    }

    /// 当前平台是否有可用的 LedSDK（厂商提供 Windows x64 与 Linux x64 版）。
    pub fn available(&self) -> bool {
        cfg!(any(windows, target_os = "linux"))
    }

    pub fn version(&self) -> String {
        let version = self.lock().version.clone();
        if version.is_empty() && !self.available() {
            return "不可用（LedSDK 仅支持 Windows x64 / Linux x64）".to_string();
        }
        version
    }

    pub fn context(&self) -> Handle {
        self.lock().context
    }

    pub fn http_running(&self) -> bool {
        self.lock().http_running
    }

    pub fn http_port(&self) -> u16 {
        self.lock().http_port
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub fn start(&self, settings: &SdkSettings) -> Result<(), SdkError> {
        if !self.available() {
            warn!(
                "本平台没有 LedSDK（仅 Windows x64 / Linux x64）：界面与 HTTP 接口可用，建树/校验/导出/发送一律不可用。"
            );
            return Ok(());
        }
        let bin = Path::new(&settings.bin_directory);
        let library = bin.join(ffi::library_name());
        if !library.exists() {
            return Err(SdkError::with_detail(
                ffi::result::IO_ERROR,
                "加载 LedSDK",
                format!("未找到 {}", library.display()),
            ));
        }
        ffi::load(&library).map_err(|err| {
            SdkError::with_detail(ffi::result::INTERNAL_ERROR, "加载 LedSDK", err)
        })?;
        // 不再改进程 PATH：SDK 本体由 libloading 按 BinDirectory 的绝对路径装载；
        // 多线程进程里 set_var("PATH") 是不健全的（Rust 文档明确禁止）。

        let (mut major, mut minor, mut patch, mut abi) = (0u32, 0u32, 0u32, 0u32);
        let rc =
            unsafe { ffi::ledsdk_get_api_version(&mut major, &mut minor, &mut patch, &mut abi) };
        if rc != ffi::result::OK {
            return Err(SdkError::new(rc, "ledsdk_get_api_version"));
        }
        if abi != 2 {
            return Err(SdkError::with_detail(
                ffi::result::IO_ERROR,
                "校验 ABI 版本",
                format!("需要 ABI 2，实际 {abi}"),
            ));
        }
        let version = format!("{major}.{minor}.{patch} (ABI {abi})");

        let config = ffi::init::<ffi::LedsdkConfig>(|c| {
            c.size = std::mem::size_of::<ffi::LedsdkConfig>() as u32;
            c.version = ffi::STRUCT_VERSION;
            c.worker_count = settings.worker_count;
            c.log_callback = Some(on_log);
        });
        let mut context: Handle = ffi::NULL_HANDLE;
        let rc = unsafe { ffi::ledsdk_initialize(&config, &mut context) };
        if rc != ffi::result::OK || context.is_null() {
            return Err(SdkError::new(rc, "ledsdk_initialize"));
        }
        self.lock().context = context;
        self.lock().version = version.clone();
        info!("已加载 LedSDK {version}，DLL 目录 {}", bin.display());

        for dir in [
            &settings.export_root,
            &settings.upload_root,
            &settings.transcode_cache_root,
        ] {
            let _ = std::fs::create_dir_all(dir);
        }
        // SDK 的 HTTP 服务没有“关闭上传”开关：只要 upload_root 存在，就会接受
        // 未认证的 POST/PUT。这里给它一个独立的临时目录，start 成功后立即删除；
        // 后续上传请求会得到 400，而 GET 下载仍由 download_root 正常提供。
        let http_upload_root = Path::new(&settings.upload_root).join(".sdk-http-incoming");
        std::fs::create_dir_all(&http_upload_root).map_err(|err| {
            self.abort_start(SdkError::with_detail(
                ffi::result::INTERNAL_ERROR,
                "创建 SDK HTTP 上传暂存目录",
                format!("{}：{err}", http_upload_root.display()),
            ))
        })?;

        // 字符串参数含 NUL 也要走 abort_start 回收已创建的 context，不能 ? 直接返回。
        let bind = cstring_arg(&settings.bind_address, "bind_address")
            .map_err(|err| self.abort_start(err))?;
        let export_root = cstring_arg(&settings.export_root, "export_root")
            .map_err(|err| self.abort_start(err))?;
        let upload_root = cstring_arg(&http_upload_root.to_string_lossy(), "upload_root")
            .map_err(|err| self.abort_start(err))?;
        let http_options = ffi::init_versioned::<ffi::LedsdkHttpOptions>(|o| {
            o.bind_address_utf8 = bind.as_ptr();
            o.port = settings.download_port as u32;
            o.download_root_utf8 = export_root.as_ptr();
            o.upload_root_utf8 = upload_root.as_ptr();
            o.max_upload_size = 1024 * 1024 * 1024;
            o.request_timeout_ms = 30_000;
            // 绝对路径 flag 会让未认证的 18080 变成任意文件读取服务，必须保持关闭。
            o.flags = ffi::http_flag::ALLOW_RANGE;
        });
        let mut http: Handle = ffi::NULL_HANDLE;
        let rc = unsafe { ffi::ledsdk_http_create(context, &http_options, &mut http) };
        if rc != ffi::result::OK || http.is_null() {
            return Err(self.abort_start(SdkError::new(rc, "ledsdk_http_create")));
        }
        // create 成功就立刻登记：后续 start/get_status 失败走 abort_start→shutdown 时，
        // shutdown 才能 release 这个句柄（原来等到最后才写，启动失败就泄漏）。
        self.lock().http = http;
        let rc = unsafe { ffi::ledsdk_http_start(http) };
        if !ffi::is_success(rc) {
            return Err(self.abort_start(SdkError::new(rc, "ledsdk_http_start")));
        }
        let mut status = ffi::init_versioned::<ffi::LedsdkHttpStatus>(|_| {});
        let rc = unsafe { ffi::ledsdk_http_get_status(http, &mut status) };
        if rc != ffi::result::OK {
            return Err(self.abort_start(SdkError::new(rc, "ledsdk_http_get_status")));
        }
        if let Err(err) = remove_http_upload_root(&http_upload_root) {
            return Err(self.abort_start(SdkError::with_detail(
                ffi::result::INTERNAL_ERROR,
                "关闭 SDK HTTP 上传目录",
                format!(
                    "{} 无法删除，18080 将保持可写：{err}",
                    http_upload_root.display()
                ),
            )));
        }
        info!("SDK HTTP 上传入口已禁用（启动后删除 upload_root）。");
        {
            let mut guard = self.lock();
            guard.http_running = status.running != 0;
            guard.http_port = status.port as u16;
        }
        info!(
            "HTTP 文件服务已启动于 {}:{}（根目录 {}；本机可访问 http://{}:{}，设备请使用同网段本机 IPv4）",
            settings.bind_address,
            status.port,
            settings.export_root,
            crate::net::browsable_address(&settings.bind_address),
            status.port
        );

        // 搜索：不注册设备回调，改由后台线程轮询快照（这样才能拿到「命中网卡」的本机地址）。
        let search_options = ffi::init_versioned::<ffi::LedsdkSearchOptions>(|o| {
            o.broadcast_port = 0;
        });
        let mut search: Handle = ffi::NULL_HANDLE;
        let rc = unsafe {
            ffi::ledsdk_search_create(
                context,
                &search_options,
                None,
                None,
                std::ptr::null_mut(),
                &mut search,
            )
        };
        if rc != ffi::result::OK || search.is_null() {
            return Err(self.abort_start(SdkError::new(rc, "ledsdk_search_create")));
        }
        let rc = unsafe { ffi::ledsdk_search_start(search) };
        if !ffi::is_success(rc) {
            unsafe { ffi::ledsdk_search_release(search) };
            return Err(self.abort_start(SdkError::new(rc, "ledsdk_search_start")));
        }
        self.lock().search = search;
        info!(
            "设备搜索已启动（SDK 侧每 2 秒一轮 UDP 广播，仅用于发现设备与发送前核对；屏幕清单只在点「扫描」时写回配置，不做自动扫描）。"
        );
        Ok(())
    }

    pub fn snapshot_devices(&self) -> Vec<DeviceSnapshot> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Vec::new();
        }
        // 整个枚举都持锁：shutdown() 会在同一把锁里 search_release，解锁后再用旧句柄
        // 就会访问已释放的原生对象（扫描正好撞上关程序时崩）。
        let guard = self.lock();
        let search = guard.search;
        if search.is_null() {
            return Vec::new();
        }
        let mut count = 0u32;
        let rc = unsafe { ffi::ledsdk_search_get_device_count(search, &mut count) };
        if rc != ffi::result::OK {
            debug!("search_get_device_count 返回 result={rc}");
            return Vec::new();
        }

        let mut devices = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut info = ffi::init_versioned::<ffi::LedsdkDeviceInfo>(|_| {});
            let rc = unsafe { ffi::ledsdk_search_get_device(search, index, &mut info) };
            if rc != ffi::result::OK {
                continue;
            }
            devices.push(DeviceSnapshot {
                device_id: cstr(info.device_id_utf8),
                name: cstr(info.name_utf8),
                ip: cstr(info.address_utf8),
                port: info.port as u16,
                device_type: info.device_type,
                online: info.online != 0,
                local_address: first_local_address(search, index),
            });
        }
        devices
    }

    pub fn ensure_http_running(&self) -> Result<(), SdkError> {
        // 读句柄与调用 SDK 必须在同一把锁内：否则 shutdown 可能刚释放 http 就轮到这里。
        let mut guard = self.lock();
        let http = guard.http;
        if http.is_null() {
            return Err(SdkError::with_detail(
                ffi::result::INVALID_STATE,
                "http_get_status",
                "HTTP 文件服务未启动",
            ));
        }
        let mut status = ffi::init_versioned::<ffi::LedsdkHttpStatus>(|_| {});
        let rc = unsafe { ffi::ledsdk_http_get_status(http, &mut status) };
        if rc != ffi::result::OK || status.running == 0 {
            return Err(SdkError::with_detail(
                ffi::result::INVALID_STATE,
                "http_get_status",
                "HTTP 文件服务未运行，设备无法下载节目包；请检查端口是否被占用并重启程序",
            ));
        }
        guard.http_running = true;
        guard.http_port = status.port as u16;
        Ok(())
    }

    pub fn export_playlist(
        &self,
        playlist: Handle,
        request: &ExportRequest,
        timeout: Duration,
    ) -> Result<Outcome, SdkError> {
        self.ensure_open()?;
        let context = self.context();
        let output_root = cstring_arg(&request.output_root, "output_root")?;
        let cache = cstring_arg(&request.transcode_cache_root, "transcode_cache_root")?;
        let address = cstring_arg(&request.download_address, "download_address")?;
        let options = ffi::init_versioned::<ffi::LedsdkExportOptions>(|o| {
            o.output_root_utf8 = output_root.as_ptr();
            o.download_port = request.download_port as u32;
            o.transcode_cache_root_utf8 = cache.as_ptr();
            o.download_address_utf8 = address.as_ptr();
        });

        let signal = Arc::new(TaskSignal::default());
        let raw = Arc::into_raw(signal.clone());
        let mut task: Handle = ffi::NULL_HANDLE;
        let rc = unsafe {
            ffi::ledsdk_playlist_export(
                context,
                playlist,
                &options,
                Some(on_task),
                raw as *mut c_void,
                &mut task,
            )
        };
        self.finish_submit(rc, task, raw, signal, "playlist_export", timeout)
    }

    pub fn send_program(
        &self,
        device_id: &str,
        package_path: &str,
        request: &SendRequest,
        timeout: Duration,
    ) -> Result<Outcome, SdkError> {
        self.ensure_open()?;
        self.ensure_http_running()?;
        let context = self.context();
        let device = cstring_arg(device_id, "device_id")?;
        let package = cstring_arg(package_path, "package_path")?;
        let md5 = cstring_arg(&request.md5_hex, "md5_hex")?;
        let options = ffi::init_versioned::<ffi::LedsdkSendOptions>(|o| {
            o.file_type = request.file_type;
            o.retry_count = request.retry_count.max(0) as u32;
            o.retry_backoff_ms = request.retry_backoff_ms.max(0) as u32;
            o.md5_hex_utf8 = md5.as_ptr();
        });

        let signal = Arc::new(TaskSignal::default());
        let raw = Arc::into_raw(signal.clone());
        let mut task: Handle = ffi::NULL_HANDLE;
        let rc = unsafe {
            ffi::ledsdk_send_program(
                context,
                device.as_ptr(),
                package.as_ptr(),
                &options,
                Some(on_task),
                raw as *mut c_void,
                &mut task,
            )
        };
        self.finish_submit(rc, task, raw, signal, "send_program", timeout)
    }

    fn finish_submit(
        &self,
        rc: i32,
        task: Handle,
        raw: *const TaskSignal,
        signal: Arc<TaskSignal>,
        operation: &str,
        timeout: Duration,
    ) -> Result<Outcome, SdkError> {
        if !ffi::is_success(rc) {
            unsafe { drop(Arc::from_raw(raw)) };
            return Err(SdkError::new(rc, operation));
        }
        if task.is_null() {
            unsafe { drop(Arc::from_raw(raw)) };
            return Err(SdkError::with_detail(
                ffi::result::INTERNAL_ERROR,
                operation,
                "SDK 未返回任务句柄",
            ));
        }

        match signal.wait(timeout) {
            Some(outcome) => Ok(outcome),
            None => {
                warn!(
                    "{operation} 任务超时（{} 秒），请求取消并等待终态。",
                    timeout.as_secs()
                );
                // task_cancel 只是「请求」，完成仍通过回调报告（见 ledsdk.h:641）。
                // 关键：返回前必须等到终态（task_get_status 轮询）。否则 pipeline 的
                // finally 会 playlist_release 掉整棵树，而 SDK 导出线程仍持该句柄 → UAF。
                unsafe { ffi::ledsdk_task_cancel(task) };
                let grace = Duration::from_secs(30);
                if wait_until(
                    || {
                        let mut status = ffi::init_versioned::<ffi::LedsdkTaskStatus>(|_| {});
                        let rc = unsafe { ffi::ledsdk_task_get_status(task, &mut status) };
                        rc == ffi::result::OK && ffi::task_state::is_terminal(status.state)
                    },
                    grace,
                ) {
                    debug!("{operation} 已确认任务进入终态，可安全释放节目树。");
                } else {
                    warn!("{operation} 取消后 {grace:?} 仍未到终态，继续返回（节目树可能仍被 SDK 引用）。");
                }
                // 终态回调到达时 on_task 会释放 task 句柄与 userData；
                // 这里绝不能自行 from_raw/drop，否则与回调双重释放。
                Err(SdkError::with_detail(
                    ffi::result::INTERNAL_ERROR,
                    operation,
                    format!("等待任务终态超时（{} 秒）", timeout.as_secs()),
                ))
            }
        }
    }

    pub fn runtime_status(&self) -> Option<ffi::LedsdkRuntimeStatus> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return None;
        }
        let context = self.context();
        if context.is_null() {
            return None;
        }
        let mut status = ffi::init_versioned::<ffi::LedsdkRuntimeStatus>(|_| {});
        let rc = unsafe { ffi::ledsdk_get_runtime_status(context, &mut status) };
        (rc == ffi::result::OK).then_some(status)
    }

    /// 返回 true 表示 SDK 已完整释放；false 表示有任务/搜索未停下，
    /// 为避免释放仍被原生线程使用的句柄，本次放弃清理，交给进程 watchdog 强退。
    pub fn shutdown(&self) -> bool {
        if self.closed.load(Ordering::SeqCst) {
            return true;
        }
        // 先置位：此后提交/快照直接失败，不再碰即将释放的句柄。
        if self.shutting_down.swap(true, Ordering::SeqCst) {
            // 另一个线程正在关闭；等它完成，避免重复释放。
            return wait_until(
                || self.closed.load(Ordering::SeqCst),
                Duration::from_secs(10),
            );
        }
        // 关键：在同一把锁里把 search/http 摘成 NULL 再释放。snapshot_devices 也是
        // 持锁枚举的：它要么在我们摘句柄前完成，要么之后看到 NULL 直接返回，
        // 绝不会拿到一个正在被 release 的悬空句柄。context 保留到关闭末尾，供
        // runtime_status() 轮询任务数（search_task_count/active_task_count）。
        let (context, http, search) = {
            let mut guard = self.lock();
            let handles = (guard.context, guard.http, guard.search);
            guard.http = ffi::NULL_HANDLE;
            guard.search = ffi::NULL_HANDLE;
            guard.http_running = false;
            handles
        };
        if context.is_null() {
            self.closed.store(true, Ordering::SeqCst);
            return true;
        }

        if !search.is_null() {
            unsafe { ffi::ledsdk_search_cancel(search) };
            // cancel 之后任务还要收尾，立刻 release 会得到 INVALID_STATE。
            if !wait_until(|| self.search_task_count() == 0, Duration::from_secs(3)) {
                warn!("搜索任务 3 秒内未停止，放弃本次 SDK 清理（交给进程 watchdog）。");
                return false;
            }
            unsafe { ffi::ledsdk_search_release(search) };
            self.lock().search = ffi::NULL_HANDLE;
            info!("搜索已停止并释放。");
        }

        // task_cancel_all 在「搜索任务失败」后可能永不返回，放到线程里限时等。
        let cancel = {
            let context = context as usize;
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let context = context as Handle;
                unsafe { ffi::ledsdk_task_cancel_all(context) };
                let _ = tx.send(());
            });
            rx.recv_timeout(Duration::from_secs(3)).is_ok()
        };
        if !cancel {
            warn!("task_cancel_all 3 秒内未返回，放弃本次 SDK 清理（交给进程 watchdog）。");
            return false;
        }
        if !wait_until(|| self.active_task_count() == 0, Duration::from_secs(3)) {
            warn!("仍有任务未结束，放弃本次 SDK 清理（交给进程 watchdog）。");
            return false;
        }

        if !http.is_null() {
            unsafe { ffi::ledsdk_http_stop(http) };
            unsafe { ffi::ledsdk_http_release(http) };
            let mut guard = self.lock();
            guard.http = ffi::NULL_HANDLE;
            guard.http_running = false;
            drop(guard);
            info!("HTTP 文件服务已停止。");
        }

        if let Some(status) = self.runtime_status() {
            info!(
                "关闭前运行状态：services={:#x} active_tasks={}",
                status.running_services, status.active_task_count
            );
        }

        let mut rc = unsafe { ffi::ledsdk_shutdown(context) };
        if rc != ffi::result::OK {
            std::thread::sleep(Duration::from_millis(200));
            rc = unsafe { ffi::ledsdk_shutdown(context) };
            if rc != ffi::result::OK {
                warn!("ledsdk_shutdown 返回 result={rc}");
            }
        }
        {
            let mut guard = self.lock();
            guard.context = ffi::NULL_HANDLE;
        }
        self.closed.store(true, Ordering::SeqCst);
        info!("SDK 已关闭。");
        true
    }

    fn search_task_count(&self) -> u32 {
        self.runtime_status()
            .map(|s| s.search_task_count)
            .unwrap_or(0)
    }

    fn active_task_count(&self) -> u32 {
        self.runtime_status()
            .map(|s| s.active_task_count)
            .unwrap_or(0)
    }
}

impl Default for SdkHost {
    fn default() -> Self {
        Self::new()
    }
}

fn wait_until(mut done: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn remove_http_upload_root(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn first_local_address(search: Handle, index: u32) -> String {
    let mut count = 0u32;
    if unsafe { ffi::ledsdk_search_get_device_address_count(search, index, &mut count) }
        != ffi::result::OK
    {
        return String::new();
    }
    // 同一设备可能记录过多个网卡地址：优先在线条目，其次取最近回包的那条。
    let mut best: Option<(bool, u64, String)> = None;
    for address_index in 0..count {
        let mut address = ffi::init_versioned::<ffi::LedsdkDeviceAddress>(|_| {});
        if unsafe {
            ffi::ledsdk_search_get_device_address(search, index, address_index, &mut address)
        } != ffi::result::OK
        {
            continue;
        }
        let candidate = cstr(address.local_address_utf8);
        if !candidate
            .parse::<Ipv4Addr>()
            .map(crate::net::is_usable)
            .unwrap_or(false)
        {
            continue;
        }
        let online = address.flags & ffi::device_flag::ONLINE != 0;
        let better = match &best {
            None => true,
            Some((best_online, best_seen, _)) => match (online, *best_online) {
                (true, false) => true,
                (false, true) => false,
                _ => address.last_seen_ms > *best_seen,
            },
        };
        if better {
            best = Some((online, address.last_seen_ms, candidate));
        }
    }
    best.map(|(_, _, candidate)| candidate).unwrap_or_default()
}

pub fn cstr(value: *const c_char) -> String {
    if value.is_null() {
        return String::new();
    }
    // SAFETY: 指针来自 SDK 的 UTF-8 缓冲，且只在当前回调/查询作用域内使用。
    unsafe { CStr::from_ptr(value).to_string_lossy().into_owned() }
}

// ---------------------------------------------------------------------------
// 任务回调与等待
// ---------------------------------------------------------------------------

/// 任务完成信号：终态回调写入 outcome 并释放句柄，等待方通过 Condvar 取结果。
/// released 仅用于在回调被 SDK 重复触发时去重，避免双重释放 task 句柄。
#[derive(Default)]
struct TaskSignal {
    slot: Mutex<Option<Outcome>>,
    ready: Condvar,
    released: AtomicBool,
}

impl TaskSignal {
    /// 抢占释放权：只有一次回调能真正释放 task 句柄与 userData。
    fn claim_release(&self) -> bool {
        !self.released.swap(true, Ordering::SeqCst)
    }

    fn wait(&self, timeout: Duration) -> Option<Outcome> {
        let deadline = Instant::now() + timeout;
        let mut slot = self.slot.lock().unwrap_or_else(|err| err.into_inner());
        loop {
            if let Some(outcome) = slot.take() {
                return Some(outcome);
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (next, _) = self
                .ready
                .wait_timeout(slot, deadline - now)
                .unwrap_or_else(|err| err.into_inner());
            slot = next;
        }
    }
}

unsafe extern "C" fn on_task(task: Handle, event: *const ffi::LedsdkTaskEvent, user: *mut c_void) {
    if event.is_null() || user.is_null() {
        return;
    }
    let event = &*event;
    if !ffi::task_state::is_terminal(event.state) {
        return;
    }

    let signal = &*(user as *const TaskSignal);
    // 终态回调（含 CANCELLED）：无论是否走过超时取消，都要写回结果并释放句柄。
    // 之前 CANCELLED 直接 return 会永久泄漏 task 句柄与 userData（A3）。
    let outcome = Outcome {
        state: event.state,
        result: event.result,
        message: cstr(event.message_utf8),
    };
    {
        let mut slot = signal.slot.lock().unwrap_or_else(|err| err.into_inner());
        if slot.is_none() {
            *slot = Some(outcome);
        }
    }
    signal.ready.notify_all();

    // 句柄与 userData 只在终态之后释放，而且不留在 SDK 回调栈上。
    // 抢不到释放权只可能是 SDK 重复回调，跳过即可。
    if !signal.claim_release() {
        debug!("任务终态回调重复到达，跳过重复释放。");
        return;
    }
    let raw = user as usize;
    // Handle（*mut c_void）不实现 Send，转成 usize 再交给新线程。
    let task_raw = task as usize;
    std::thread::spawn(move || unsafe {
        let task = task_raw as Handle;
        if !task.is_null() {
            ffi::ledsdk_task_release(task);
        }
        drop(Arc::from_raw(raw as *const TaskSignal));
    });
}

unsafe extern "C" fn on_log(level: u32, message: *const c_char, _user: *mut c_void) {
    let text = cstr(message);
    match level {
        0 => error!("[SDK] {text}"),
        1 => warn!("[SDK] {text}"),
        2 => info!("[SDK] {text}"),
        _ => debug!("[SDK] {text}"),
    }
}
