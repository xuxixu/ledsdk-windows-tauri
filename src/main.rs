// 发布版不弹控制台窗口；debug 版保留，方便看 selfcheck / simulate 输出。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod ffi;
mod host;
mod jobs;
mod logging;
mod models;
mod net;
mod pipeline;
mod retention;
mod runtime;
mod selfcheck;
mod server;
#[cfg(all(any(windows, target_os = "linux"), debug_assertions))]
mod simulate;
#[cfg(windows)]
mod winos;
#[cfg(not(windows))]
#[path = "winos_linux.rs"]
mod winos;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::{error, info, warn};
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, RunEvent, State};

type SharedRuntime = Arc<runtime::Runtime>;

/// 退出流程只允许执行一次：托盘菜单、界面按钮和系统关闭可能同时触发。
static EXITING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn get_status(runtime: State<'_, SharedRuntime>) -> serde_json::Value {
    let settings = runtime.registry.sdk();
    let status = runtime.host.runtime_status();
    serde_json::json!({
        "sdkAvailable": runtime.host.available(),
        "sdk": runtime.host.version(),
        "httpRunning": runtime.host.http_running(),
        "httpPort": runtime.host.http_port(),
        "activeTaskCount": status.as_ref().map(|s| s.active_task_count).unwrap_or(0),
        "searchTaskCount": status.as_ref().map(|s| s.search_task_count).unwrap_or(0),
        "screens": runtime.registry.screens().len(),
        "apiUrl": format!(
            "http://{}:{}",
            net::browsable_address(&runtime.registry.settings().upload_api.listen_address),
            api_port(runtime.as_ref())
        ),
        "uploadApiRunning": runtime.upload_api_status.running(),
        "uploadApiError": runtime.upload_api_status.error(),
        "configPath": runtime.config_path.to_string_lossy(),
        "exportRoot": settings.export_root,
        "logDirectory": runtime.log_directory.to_string_lossy(),
    })
}

fn api_port(runtime: &runtime::Runtime) -> u16 {
    runtime.registry.settings().upload_api.port
}

fn screens_json(runtime: &runtime::Runtime) -> serde_json::Value {
    // 与 HTTP `/api/screens` 共用同一个视图结构，避免两套字段口径。
    serde_json::to_value(server::screen_views(&runtime.registry))
        .unwrap_or_else(|_| serde_json::json!([]))
}

#[tauri::command]
fn list_screens(runtime: State<'_, SharedRuntime>) -> serde_json::Value {
    screens_json(runtime.inner())
}

/// 手动扫描：取一次搜索快照并写回配置（取代原来的后台自动写回）。
#[tauri::command]
fn scan_screens(runtime: State<'_, SharedRuntime>) -> serde_json::Value {
    let (scanned, updated) = runtime.scan_screens();
    serde_json::json!({
        "scanned": scanned,
        "updated": updated,
        "screens": screens_json(runtime.inner()),
    })
}

/// 重新从磁盘读取配置（外部改了 appsettings.json 后用它）。
#[tauri::command]
fn reload_screens(runtime: State<'_, SharedRuntime>) -> serde_json::Value {
    let count = runtime.registry.reload();
    serde_json::json!({ "scanned": count, "updated": 0, "screens": screens_json(runtime.inner()) })
}

#[tauri::command]
fn delete_screen(
    runtime: State<'_, SharedRuntime>,
    device_id: String,
    name: String,
) -> Result<(), String> {
    runtime.registry.remove_screen(&device_id, &name)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn update_screen(
    runtime: State<'_, SharedRuntime>,
    device_id: String,
    name: String,
    ip: String,
    port: u16,
    width: i32,
    height: i32,
    rotate: i32,
) -> Result<(), String> {
    runtime
        .registry
        .update_screen(&device_id, &name, &ip, port, width, height, rotate)
}

#[tauri::command]
fn list_jobs(runtime: State<'_, SharedRuntime>, limit: Option<usize>) -> serde_json::Value {
    serde_json::json!(runtime.jobs.recent(limit.unwrap_or(20).clamp(1, 200)))
}

#[tauri::command]
fn read_logs(runtime: State<'_, SharedRuntime>, lines: Option<usize>) -> Vec<String> {
    runtime.log_tail(lines.unwrap_or(200).clamp(1, 2000))
}

/// 前端未捕获异常（window.onerror / unhandledrejection）写进 errors 日志。
#[tauri::command]
fn log_ui_error(message: String, source: String) {
    let text = format!("前端异常：{message}\n位置：{source}");
    crate::logging::record_exception_global("UI", &text);
    log::warn!("前端异常：{message}（{source}）");
}

#[tauri::command]
fn open_folder(runtime: State<'_, SharedRuntime>, path: String) -> bool {
    let target = PathBuf::from(&path);
    if !runtime.is_allowed_folder(&target) {
        log::warn!("拒绝打开目录：{path}（不在导出/上传/日志/配置目录内）");
        return false;
    }
    #[cfg(windows)]
    let mut command = std::process::Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");

    command.arg(target).spawn().is_ok()
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    request_exit(&app);
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text("show", "打开界面")
        .separator()
        .text("quit", "退出")
        .build()?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("LedSdk 屏幕节目上传服务")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "quit" => request_exit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    info!("系统托盘图标已启用。");
    Ok(())
}

/// 主动退出：先在后台完成 SDK/HTTP 清理，再直接结束进程。
///
/// 不走 `app.exit()` 是为了跳过 Tauri 事件循环销毁托盘与原生菜单的路径：
/// 在托盘菜单回调仍在栈上时销毁菜单，Windows 会在 ntdll 中以 0xC000000D
/// 终止进程。`TerminateProcess` 不跑 DllMain(DETACH)，也不会触发这条路径。
fn request_exit(app: &tauri::AppHandle) {
    if EXITING.swap(true, Ordering::SeqCst) {
        return;
    }
    let runtime = app
        .try_state::<SharedRuntime>()
        .map(|state| state.inner().clone());
    std::thread::spawn(move || {
        if let Some(runtime) = runtime {
            info!("开始关闭：停上传接口 → 停 SDK（搜索/任务/HTTP）→ 关日志。");
            if !runtime.stop() {
                warn!("关闭未完全完成，仍将结束进程。");
            }
        }
        exit_now(0);
    });
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli_mode = args
        .iter()
        .any(|arg| arg == "--selfcheck" || arg == "--simulate");

    // 单实例保护：托盘模式下重复点桌面图标时唤起已有窗口，避免 SDK/HTTP 端口冲突后静默退出。
    if !cli_mode && !winos::ensure_single_instance() {
        exit_now(0);
    }

    // 作业对象要在任何子进程出现之前建立：之后 ffmpeg / TextRenderer 才会随主进程一起结束。
    // 无论带什么参数都先建立（CLI 模式同样可能拉起子进程，曾经因为 `--` 前缀跳过这里）。
    winos::assign_current_process();

    if args.iter().any(|arg| arg == "--selfcheck") {
        exit_now(selfcheck::run());
    }
    if args.iter().any(|arg| arg == "--simulate") {
        // 模拟测试依赖 ffmpeg 与测试素材，只编进 debug 版；发行版给出明确说明。
        #[cfg(all(any(windows, target_os = "linux"), debug_assertions))]
        exit_now(simulate::run());
        #[cfg(not(all(any(windows, target_os = "linux"), debug_assertions)))]
        {
            eprintln!(
                "--simulate 只在 debug 版本提供（发行版不含模拟测试，请用 --selfcheck 自检）。"
            );
            exit_now(2);
        }
    }

    tauri::Builder::default()
        .setup(|app| {
            let base = exe_directory();
            let config_path = base.join("config").join("appsettings.json");
            ensure_default_config(&config_path);
            let log_directory = base.join("logs");

            let runtime = match runtime::Runtime::start(&config_path, &log_directory, true) {
                Ok(runtime) => runtime,
                Err(err) => {
                    // 启动失败时 SDK 可能已加载但没有 initialize，正常退出会在 C++ 析构里抛异常
                    // （0xC0000409）；这里直接终止进程，留下日志与本条 stderr。
                    eprintln!("启动失败：{err}");
                    winos::show_error_dialog("LedSdk 启动失败", &format!("启动失败：{err}"));
                    exit_now(1);
                }
            };
            app.manage(runtime.clone());

            let settings = runtime.registry.settings();
            if settings.app.tray_icon {
                if let Err(err) = setup_tray(app) {
                    error!("创建系统托盘失败：{err}");
                }
            }
            let state = server::ApiState {
                host: runtime.host.clone(),
                registry: runtime.registry.clone(),
                jobs: runtime.jobs.clone(),
                listen_address: settings.upload_api.listen_address.clone(),
                port: settings.upload_api.port,
                token: Some(settings.upload_api.token.clone()).filter(|token| !token.is_empty()),
                upload_api_status: runtime.upload_api_status.clone(),
            };
            let http_shutdown = runtime.http_shutdown.clone();
            let api_status = runtime.upload_api_status.clone();
            std::thread::spawn(move || {
                let tokio_runtime = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(err) => {
                        error!("创建异步运行时失败：{err}");
                        api_status.mark_failed(format!("创建异步运行时失败：{err}"));
                        return;
                    }
                };
                if let Err(err) = tokio_runtime.block_on(server::serve(state, http_shutdown)) {
                    error!("上传接口异常退出：{err}");
                    api_status.mark_failed(format!("上传接口异常退出：{err}"));
                }
            });

            info!("桌面程序已启动。");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            list_screens,
            scan_screens,
            reload_screens,
            delete_screen,
            update_screen,
            list_jobs,
            read_logs,
            open_folder,
            quit_app,
            log_ui_error
        ])
        .build(tauri::generate_context!())
        .expect("构建 Tauri 应用失败")
        .run(|app, event| {
            if let RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } = &event
            {
                if label == "main" {
                    if let Some(runtime) = app.try_state::<SharedRuntime>() {
                        if runtime.registry.settings().app.tray_icon {
                            if app.tray_by_id("main").is_some() {
                                api.prevent_close();
                                if let Some(window) = app.get_webview_window("main") {
                                    let _ = window.hide();
                                }
                                info!("窗口已隐藏到系统托盘；右键托盘图标可重新打开或退出。");
                            } else {
                                error!("配置要求托盘模式，但托盘图标不存在；本次关闭将直接退出。");
                            }
                        }
                    }
                }
            }
            if let RunEvent::Exit = event {
                if EXITING.swap(true, Ordering::SeqCst) {
                    // 主动退出路径已经接手清理；这里直接结束，避免 Tauri
                    // 再销毁托盘/菜单原生对象。
                    exit_now(0);
                }
                if let Some(runtime) = app.try_state::<SharedRuntime>() {
                    info!("开始关闭：停上传接口 → 停 SDK（搜索/任务/HTTP）→ 关日志。");
                    // 兜底：SDK 原生调用万一不返回，8 秒后强制退出（子进程由作业对象连带结束）。
                    std::thread::spawn(|| {
                        std::thread::sleep(std::time::Duration::from_secs(8));
                        eprintln!("关闭超时，强制退出进程。");
                        std::process::exit(0);
                    });
                    if runtime.stop() {
                        info!("关闭完成，退出进程。");
                    } else {
                        warn!("关闭未完全完成，等待 watchdog 强制退出。");
                    }
                    exit_now(0);
                }
            }
        });
}

/// 立即结束进程，退出码原样返回。
///
/// `--selfcheck` / `--simulate` 与启动失败路径不能走正常退出：LedSDK.dll 由静态导入在进程
/// 启动时载入，若从未 `ledsdk_initialize` 过，它的 C++ 静态析构会在退出时抛异常，Rust 侧
/// 表现为 `fatal runtime error: Rust cannot catch foreign exceptions` + 0xC0000409。
/// TerminateProcess 不跑 DllMain(DETACH)，因此不受影响。
fn exit_now(code: i32) -> ! {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
        TerminateProcess(GetCurrentProcess(), code as u32);
    }
    std::process::exit(code)
}

fn exe_directory() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn ensure_default_config(path: &Path) {
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let settings = config::AppSettings::default();
    match config::save(path, &settings) {
        Ok(()) => eprintln!("已生成默认配置：{}", path.display()),
        Err(err) => eprintln!("生成默认配置失败：{}：{err}", path.display()),
    }
}
