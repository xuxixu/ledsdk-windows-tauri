//! 自检：结构体布局、SDK 装载、生命周期（无窗口，命令行 `--selfcheck`）。

use std::io::{Read, Write};
use std::mem::size_of;
use std::net::TcpStream;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::ffi;

pub fn run() -> i32 {
    let mut failures = 0;

    failures += check_layouts();
    if cfg!(any(windows, target_os = "linux")) {
        failures += check_version();
    } else {
        println!("SDK 版本：  [skip] 本平台没有 LedSDK（仅 Windows x64 / Linux x64）");
    }
    failures += check_registry_edits();
    failures += check_autostart();
    failures += check_package_guard();
    failures += check_exception_log();
    failures += check_retention();
    if cfg!(any(windows, target_os = "linux")) {
        failures += check_lifecycle();
    } else {
        println!("生命周期：  [skip] 需要真实 LedSDK，本平台跳过");
    }

    if failures == 0 {
        println!("SELF-CHECK PASSED");
        0
    } else {
        println!("SELF-CHECK FAILED（{failures} 项）");
        1
    }
}

fn expect(name: &str, actual: usize, wanted: usize) -> i32 {
    if actual == wanted {
        println!("  [ok] {name} = {actual}");
        0
    } else {
        println!("  [FAIL] {name} = {actual}，期望 {wanted}");
        1
    }
}

fn check_layouts() -> i32 {
    println!("结构体布局（x64）：");
    let mut failures = 0;
    failures += expect("LedsdkConfig", size_of::<ffi::LedsdkConfig>(), 48);
    failures += expect("LedsdkHttpOptions", size_of::<ffi::LedsdkHttpOptions>(), 80);
    failures += expect("LedsdkHttpStatus", size_of::<ffi::LedsdkHttpStatus>(), 32);
    failures += expect(
        "LedsdkSearchOptions",
        size_of::<ffi::LedsdkSearchOptions>(),
        32,
    );
    failures += expect("LedsdkDeviceInfo", size_of::<ffi::LedsdkDeviceInfo>(), 64);
    failures += expect(
        "LedsdkDeviceAddress",
        size_of::<ffi::LedsdkDeviceAddress>(),
        48,
    );
    failures += expect("LedsdkPlayRule", size_of::<ffi::LedsdkPlayRule>(), 80);
    failures += expect("LedsdkFontOptions", size_of::<ffi::LedsdkFontOptions>(), 64);
    failures += expect(
        "LedsdkTextOptions",
        size_of::<ffi::LedsdkTextOptions>(),
        128,
    );
    failures += expect(
        "LedsdkExportOptions",
        size_of::<ffi::LedsdkExportOptions>(),
        80,
    );
    failures += expect("LedsdkSendOptions", size_of::<ffi::LedsdkSendOptions>(), 56);
    failures += expect("LedsdkTaskEvent", size_of::<ffi::LedsdkTaskEvent>(), 64);
    failures += expect("LedsdkTaskStatus", size_of::<ffi::LedsdkTaskStatus>(), 40);
    failures += expect(
        "LedsdkRuntimeStatus",
        size_of::<ffi::LedsdkRuntimeStatus>(),
        48,
    );
    failures
}

fn check_version() -> i32 {
    println!("SDK 版本：");
    let library = Path::new(&default_bin_directory()).join(ffi::library_name());
    if let Err(err) = ffi::load(&library) {
        println!("  [FAIL] 加载 SDK 失败：{err}");
        return 1;
    }
    let (mut major, mut minor, mut patch, mut abi) = (0u32, 0u32, 0u32, 0u32);
    let rc = unsafe { ffi::ledsdk_get_api_version(&mut major, &mut minor, &mut patch, &mut abi) };
    if rc != ffi::result::OK {
        println!("  [FAIL] get_api_version 返回 result={rc}");
        return 1;
    }
    println!("  [ok] LedSDK {major}.{minor}.{patch} ABI {abi}");
    if abi != 2 {
        println!("  [FAIL] 需要 ABI 2，实际 {abi}");
        return 1;
    }
    0
}

/// 屏幕清单的编辑/删除/重载：这几个操作会改配置文件，必须有断言兜住。
fn check_registry_edits() -> i32 {
    use crate::config::{save, AppSettings, ScreenProfile, ScreenRegistry};

    println!("屏幕清单编辑：");
    let directory = std::env::temp_dir()
        .join("ledsdk-tauri-selfcheck")
        .join("registry");
    let path = directory.join("appsettings.json");
    let _ = std::fs::create_dir_all(&directory);

    let mut settings = AppSettings::default();
    settings.screens.push(ScreenProfile {
        name: "s1".to_string(),
        device_id: "dev-1".to_string(),
        ip: "10.0.0.9".to_string(),
        port: 20003,
        device_type: 51,
        width: 256,
        height: 256,
        ..ScreenProfile::default()
    });
    if let Err(err) = save(&path, &settings) {
        println!("  [FAIL] 准备配置失败：{err}");
        return 1;
    }

    let registry = ScreenRegistry::load(&path);
    let mut failures = 0;
    failures += expect_bool(
        "编辑已存在条目",
        registry
            .update_screen("dev-1", "s1-改名", "10.0.0.10", 20004, 1280, 128, 90)
            .is_ok(),
        true,
    );
    failures += expect_bool(
        "编辑不存在的条目",
        registry
            .update_screen("dev-x", "s1", "10.0.0.10", 20004, 1280, 128, 90)
            .is_err(),
        true,
    );
    failures += expect_bool(
        "非法几何给出原因",
        registry
            .update_screen("dev-1", "s1", "10.0.0.10", 20004, 0, 128, 90)
            .err()
            .is_some_and(|msg| msg.contains("旋转")),
        true,
    );

    let reloaded = crate::config::load_or_default(&path);
    let edited = reloaded.screens.first().cloned().unwrap_or_default();
    failures += expect_bool("改后名称已落盘", edited.name == "s1-改名", true);
    failures += expect_bool(
        "改后地址已落盘",
        edited.ip == "10.0.0.10" && edited.port == 20004,
        true,
    );
    failures += expect_bool(
        "改后几何已落盘",
        edited.width == 1280 && edited.height == 128 && edited.rotate == 90,
        true,
    );
    failures += expect_bool("deviceId 不变", edited.device_id == "dev-1", true);

    failures += expect_bool(
        "删除已存在条目",
        registry.remove_screen("dev-1", "").is_ok(),
        true,
    );
    failures += expect_bool("删除后清单为空", registry.screens().is_empty(), true);

    std::fs::write(
        &path,
        r#"{"Screens":[{"Name":"a","DeviceId":"d1"},{"Name":"b","DeviceId":"d2"}]}"#,
    )
    .ok();
    failures += expect("重载条数", registry.reload(), 2);

    let recover = directory.join("recover.json");
    std::fs::write(&recover, "{ broken json").ok();
    std::fs::write(
        recover.with_extension("json.bak"),
        r#"{"Screens":[{"Name":"bak","DeviceId":"bak-device"}]}"#,
    )
    .ok();
    failures += expect_bool(
        "坏配置从 .bak 恢复",
        crate::config::load_or_default(&recover).screens.len() == 1,
        true,
    );

    let defaults = crate::config::AppSettings::default();
    failures += expect_bool("托盘图标默认开启", defaults.app.tray_icon, true);
    failures += expect_bool("桌面快捷方式默认开启", defaults.app.desktop_shortcut, true);
    failures
}

fn expect_bool(name: &str, actual: bool, wanted: bool) -> i32 {
    if actual == wanted {
        println!("  [ok] {name}");
        0
    } else {
        println!("  [FAIL] {name} = {actual}，期望 {wanted}");
        1
    }
}

/// 发送前的包内地址校验与失败提示（对应 C# 版 PackageGuardCheck 的 5 条断言）。
/// 开机自启：写 → 读 → 删 三步都要真的落到 HKCU Run，结束时恢复自检前的状态。
/// 诊断程序不该改用户机器，但这里是程序自己的值，且会被恢复。
#[cfg(windows)]
fn check_autostart() -> i32 {
    println!("开机自启（HKCU Run）：");
    let original = crate::winos::autostart_value();
    let expected = match std::env::current_exe() {
        Ok(path) => format!("\"{}\"", path.display()),
        Err(err) => {
            println!("  [FAIL] 读取当前 exe 失败：{err}");
            return 1;
        }
    };

    let mut failures = 0;
    crate::winos::sync_autostart(true);
    let written = crate::winos::autostart_value();
    failures += expect_bool(
        "启用后写入带引号的 exe 路径",
        written.as_deref() == Some(expected.as_str()),
        true,
    );
    // 配置没变时不应重写（这里只能验证值不变，日志里的「已是最新」分支由 sync_autostart 保证）。
    crate::winos::sync_autostart(true);
    failures += expect_bool(
        "重复启用后值不变",
        crate::winos::autostart_value() == written,
        true,
    );
    crate::winos::sync_autostart(false);
    failures += expect_bool(
        "关闭后删除自启值",
        crate::winos::autostart_value().is_none(),
        true,
    );

    // 恢复原值：原先是登记状态就按原值写回，否则保持删除。
    if !crate::winos::set_autostart(original.as_deref()) {
        println!("  [FAIL] 恢复自检前的自启值失败");
        failures += 1;
    }
    failures
}

#[cfg(not(windows))]
fn check_autostart() -> i32 {
    println!("开机自启：  [skip] 仅 Windows 使用 HKCU Run。");
    0
}

fn check_package_guard() -> i32 {
    use crate::pipeline::{send_failure_hint, verify_package};
    println!("节目包校验：");
    let package = Path::new("program_1.zhl");
    let mut failures = 0;

    failures += expect_bool(
        "地址一致时通过",
        verify_package(
            b"http://192.168.1.10:18080/a.zhd",
            "192.168.1.10",
            18080,
            package,
        )
        .is_ok(),
        true,
    );
    failures += expect_bool(
        "含 127.0.0.1 时拒绝",
        verify_package(
            b"http://127.0.0.1:18080/a.zhd",
            "192.168.1.10",
            18080,
            package,
        )
        .is_err(),
        true,
    );
    failures += expect_bool(
        "地址不一致时拒绝",
        verify_package(
            b"http://192.168.1.11:18080/a.zhd",
            "192.168.1.10",
            18080,
            package,
        )
        .is_err(),
        true,
    );
    failures += expect_bool(
        "前缀相同的地址不算匹配",
        verify_package(
            b"http://192.168.1.100:18080/a.zhd",
            "192.168.1.10",
            18080,
            package,
        )
        .is_err(),
        true,
    );
    failures += expect_bool(
        "端口不一致时拒绝",
        verify_package(
            b"http://192.168.1.10:18081/a.zhd",
            "192.168.1.10",
            18080,
            package,
        )
        .is_err(),
        true,
    );
    failures += expect_bool(
        "大写/混合大小写 scheme 可识别",
        verify_package(
            b"HTTP://192.168.1.10:18080/a.zhd",
            "192.168.1.10",
            18080,
            package,
        )
        .is_ok(),
        true,
    );
    failures += expect_bool(
        "127/8 回环地址拒绝",
        verify_package(
            b"http://127.0.0.2:18080/a.zhd",
            "192.168.1.10",
            18080,
            package,
        )
        .is_err(),
        true,
    );
    failures += expect_bool(
        "超时提示带端口",
        send_failure_hint("connect timed out", 18080).contains("18080"),
        true,
    );
    failures += expect_bool(
        "非超时不给提示",
        send_failure_hint("permission denied", 18080).is_empty(),
        true,
    );
    failures
}

/// 全局异常兜底：panic hook 与前端异常都写进 logs/<日期>/errors.log。
fn check_exception_log() -> i32 {
    println!("异常日志：");
    let root = std::env::temp_dir()
        .join("ledsdk-tauri-selfcheck")
        .join("errors");
    let _ = std::fs::remove_dir_all(&root);
    // 装全局 logger + panic hook（全局只生效一次）。
    crate::logging::init(&root, log::LevelFilter::Info);

    crate::logging::record_exception(&root, "UI", "自检前端异常");
    // 主日志文件在第一条日志落盘时才创建，这里补一行。
    log::info!("自检：写一行主日志");

    // 直接制造一次 panic：hook 应该在 unwind 之前把栈写进 errors 文件。
    let caught = std::panic::catch_unwind(|| panic!("自检：故意 panic"));
    let mut failures = 0;
    failures += expect_bool("panic 被 catch_unwind 捕获", caught.is_err(), true);

    // 结构应为 logs/<日期>/app.log 与 logs/<日期>/errors.log，且只有一天。
    let days: Vec<_> = std::fs::read_dir(&root)
        .map(|it| it.flatten().filter(|e| e.path().is_dir()).collect())
        .unwrap_or_default();
    failures += expect_bool("按天目录只有一个", days.len() == 1, true);
    let day = days.first().map(|entry| entry.path()).unwrap_or_default();
    failures += expect_bool("主日志在当天目录内", day.join("app.log").exists(), true);
    let text = std::fs::read_to_string(day.join("errors.log")).unwrap_or_default();
    failures += expect_bool("前端异常已写入", text.contains("[UI] 自检前端异常"), true);
    failures += expect_bool(
        "panic 已写入",
        text.contains("[PANIC]") && text.contains("自检：故意 panic"),
        true,
    );
    failures += expect_bool("panic 带位置", text.contains("位置："), true);
    failures
}

/// 保留清理必须只删「自己的产物」：过期 .zhl 删、被清单引用的资源留、无关文件一律不动。
fn check_retention() -> i32 {
    println!("保留清理：");
    let root = std::env::temp_dir()
        .join("ledsdk-tauri-selfcheck")
        .join("retention");
    let _ = std::fs::remove_dir_all(&root);
    let export = root.join("export");
    let logs = root.join("logs");
    let resources = export.join("resources");
    let sub = export.join("sub");
    let nested = export.join("nested");
    let nested_resources = nested.join("resources");
    let other = root.join("other");
    let upload = root.join("upload");
    for dir in [&logs, &resources, &sub, &nested_resources, &other, &upload] {
        if let Err(err) = std::fs::create_dir_all(dir) {
            println!("  [FAIL] 建目录 {}：{err}", dir.display());
            return 1;
        }
    }

    let old = SystemTime::now() - Duration::from_secs(40 * 86_400);
    let write = |path: &Path, body: &str, age: Option<SystemTime>| -> Result<(), String> {
        std::fs::write(path, body).map_err(|err| format!("写 {}：{err}", path.display()))?;
        if let Some(time) = age {
            let file = std::fs::File::options()
                .write(true)
                .open(path)
                .map_err(|err| format!("打开 {}：{err}", path.display()))?;
            file.set_times(std::fs::FileTimes::new().set_modified(time))
                .map_err(|err| format!("改时间 {}：{err}", path.display()))?;
        }
        Ok(())
    };

    // 日志按天一个目录（logs/yyyy-MM-dd/）：过期天目录整个删，今天目录保留。
    let old_day = logs.join("2026-01-01");
    let today_day = logs.join("today");
    let setup = (|| -> Result<(), String> {
        write(&export.join("old.zhl"), "old", Some(old))?;
        write(&export.join("new.zhl"), "keep.png a.zhd", None)?;
        write(&resources.join("keep.png"), "kept", Some(old))?;
        write(&resources.join("dead.png"), "gone", Some(old))?;
        write(&sub.join("old.zhd"), "gone", Some(old))?;
        write(&export.join("a.zhd"), "kept", Some(old))?;
        write(&export.join("aa.zhd"), "gone", Some(old))?;
        write(&nested.join("nested.zhl"), "nested.png", None)?;
        write(&nested_resources.join("nested.png"), "kept", Some(old))?;
        write(&nested_resources.join("nested-dead.png"), "gone", Some(old))?;
        write(&export.join("notes.txt"), "别删我", Some(old))?;
        std::fs::create_dir_all(&old_day).map_err(|e| e.to_string())?;
        write(&old_day.join("app.log"), "old log", Some(old))?;
        std::fs::create_dir_all(&today_day).map_err(|e| e.to_string())?;
        write(&today_day.join("app.log"), "new log", None)?;
        // 不是导出目录（没有 .zhl）：里面的旧文件一个都不能动。
        write(&other.join("stray.zhd"), "stray", Some(old))?;
        Ok(())
    })();
    if let Err(err) = setup {
        println!("  [FAIL] 准备测试文件失败：{err}");
        return 1;
    }

    let deleted = crate::retention::clean(&export, &upload, &logs, 30);
    let mut failures = 0;
    failures += expect_bool("过期 .zhl 已删除", export.join("old.zhl").exists(), false);
    failures += expect_bool("未过期 .zhl 保留", export.join("new.zhl").exists(), true);
    failures += expect_bool(
        "被引用的资源保留",
        resources.join("keep.png").exists(),
        true,
    );
    failures += expect_bool("无引用资源删除", resources.join("dead.png").exists(), false);
    failures += expect_bool("过期 .zhd 删除", sub.join("old.zhd").exists(), false);
    failures += expect_bool("精确文件名引用保留", export.join("a.zhd").exists(), true);
    failures += expect_bool("子串同名文件删除", export.join("aa.zhd").exists(), false);
    failures += expect_bool(
        "嵌套清单引用资源保留",
        nested_resources.join("nested.png").exists(),
        true,
    );
    failures += expect_bool(
        "嵌套无引用资源删除",
        nested_resources.join("nested-dead.png").exists(),
        false,
    );
    failures += expect_bool("无关文件不动", export.join("notes.txt").exists(), true);
    failures += expect_bool("过期日志目录整目录删除", old_day.exists(), false);
    failures += expect_bool("当天日志目录保留", today_day.join("app.log").exists(), true);
    failures += expect_bool("删除计数 = 6", deleted == 6, true);
    let skipped = crate::retention::clean(&other, &upload, &logs, 30);
    failures += expect_bool(
        "非导出目录被跳过",
        skipped == 0 && other.join("stray.zhd").exists(),
        true,
    );
    // 上传暂存：第一次清理要落下归属标记；有内容却没标记的目录（UploadRoot 配错）必须跳过。
    failures += expect_bool(
        "上传目录写入归属标记",
        upload.join(".ledsdk-upload").is_file(),
        true,
    );
    let foreign = root.join("foreign-upload");
    if std::fs::create_dir_all(&foreign).is_ok() {
        let _ = write(&foreign.join("keep.bin"), "keep", Some(old));
        crate::retention::clean(&other, &foreign, &logs, 30);
        failures += expect_bool(
            "无标记的上传目录被跳过",
            foreign.join("keep.bin").exists(),
            true,
        );
    }
    failures
}

fn check_lifecycle() -> i32 {
    println!("生命周期：");
    let run_dir = std::env::temp_dir().join("ledsdk-tauri-selfcheck");
    let _ = std::fs::create_dir_all(&run_dir);

    let mut settings = crate::config::AppSettings {
        screens: vec![crate::config::ScreenProfile {
            name: "selfcheck".to_string(),
            device_id: "selfcheck-device".to_string(),
            ip: "127.0.0.1".to_string(),
            port: 20003,
            device_type: 51,
            width: 256,
            height: 256,
            rotate: 0,
            ..Default::default()
        }],
        ..Default::default()
    };
    settings.sdk.bind_address = "127.0.0.1".to_string();
    settings.sdk.download_port = 28080;
    settings.sdk.export_root = run_dir.join("export").to_string_lossy().to_string();
    settings.sdk.upload_root = run_dir.join("upload").to_string_lossy().to_string();
    settings.sdk.transcode_cache_root = run_dir.join("cache").to_string_lossy().to_string();
    settings.sdk.bin_directory = default_bin_directory();
    settings.app.tray_icon = false;
    settings.app.desktop_shortcut = false;

    let config_path = run_dir.join("appsettings.json");
    if let Err(err) = crate::config::save(&config_path, &settings) {
        println!("  [FAIL] 写配置失败：{err}");
        return 1;
    }

    let runtime = match crate::runtime::Runtime::start(&config_path, &run_dir.join("logs"), false) {
        Ok(runtime) => runtime,
        Err(err) => {
            println!("  [FAIL] 启动失败：{err}");
            return 1;
        }
    };

    let mut failures = 0;
    failures += if runtime.host.http_running() {
        0
    } else {
        println!("  [FAIL] HTTP 未运行");
        1
    };
    failures += if runtime.host.http_port() == 28080 {
        0
    } else {
        println!("  [FAIL] HTTP 端口不是 28080");
        1
    };
    failures += if runtime.registry.screens().len() == 1 {
        0
    } else {
        println!("  [FAIL] 屏幕数量不对");
        1
    };
    failures += if runtime.registry.screens()[0].has_geometry() {
        0
    } else {
        println!("  [FAIL] 几何未保存");
        1
    };
    failures += check_http_hardening(&runtime);
    match runtime.host.runtime_status() {
        Some(status) if status.search_task_count >= 1 => {}
        _ => {
            println!("  [FAIL] 搜索任务未在运行");
            failures += 1;
        }
    }

    failures += if runtime.stop() {
        0
    } else {
        println!("  [FAIL] 关闭时 SDK 清理未完成");
        1
    };

    failures += if !runtime.host.http_running() {
        0
    } else {
        println!("  [FAIL] 关闭后 HTTP 仍运行");
        1
    };
    failures += if crate::config::load_or_default(&config_path).screens.len() == 1 {
        0
    } else {
        println!("  [FAIL] 关闭后配置不可解析");
        1
    };
    println!("  [ok] 生命周期检查完成");
    failures
}

fn check_http_hardening(runtime: &crate::runtime::Runtime) -> i32 {
    println!("SDK HTTP 文件服务隔离：");
    let root = runtime.log_directory.parent().unwrap_or(Path::new("."));
    let sdk = runtime.registry.sdk();
    let export = Path::new(&sdk.export_root);
    let upload = Path::new(&sdk.upload_root);
    let inside = export.join("selfcheck-inside.txt");
    let outside = root.join("selfcheck-outside.txt");
    let inside_ok = std::fs::write(&inside, "inside-root").is_ok()
        && std::fs::write(&outside, "outside-root").is_ok();
    if !inside_ok {
        println!("  [FAIL] 准备 HTTP 隔离测试文件失败");
        return 1;
    }
    let port = runtime.host.http_port();
    let mut failures = 0;
    failures += expect_bool(
        "根目录内相对下载可用",
        http_request(port, "GET", "/selfcheck-inside.txt", &[])
            .is_some_and(|(status, body)| status == 200 && body.contains("inside-root")),
        true,
    );
    let absolute_path = format!("/{}", outside.to_string_lossy().replace('\\', "/"));
    failures += expect_bool(
        "绝对路径下载可用（SDK 发送所需）",
        http_request(port, "GET", &absolute_path, &[])
            .is_some_and(|(status, body)| status == 200 && body.contains("outside-root")),
        true,
    );
    let put_status = http_request(port, "PUT", "/selfcheck-evil.txt", b"evil");
    failures += expect_bool(
        "SDK HTTP 上传被拒绝",
        put_status.is_some_and(|(status, _)| status != 200),
        true,
    );
    failures += expect_bool(
        "上传暂存目录已被删除",
        !upload.join(".sdk-http-incoming").exists(),
        true,
    );
    let _ = std::fs::remove_file(&inside);
    let _ = std::fs::remove_file(&outside);
    failures
}

fn http_request(port: u16, method: &str, path: &str, body: &[u8]) -> Option<(u16, String)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).ok()?;
    if !body.is_empty() {
        stream.write_all(body).ok()?;
    }
    stream.flush().ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .unwrap_or(raw.len());
    let head_text = String::from_utf8_lossy(&raw[..split]);
    let status = head_text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())?;
    Some((status, String::from_utf8_lossy(&raw[split..]).to_string()))
}

/// 找 SDK 运行文件所在目录：优先 exe 同级（build.rs 把 `sdk/x64` 或 `sdk/linux-x64` 的文件复制到这里），
/// 再向上找 `sdk/<arch>`（直接从仓库跑 cargo 的情形）。
pub fn default_bin_directory() -> String {
    let library = if cfg!(windows) {
        "LedSDK.dll"
    } else {
        "libLedSDK.so.1"
    };
    let arch = if cfg!(windows) { "x64" } else { "linux-x64" };
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()));
    if let Some(dir) = &exe_dir {
        if dir.join(library).exists() {
            return dir.to_string_lossy().to_string();
        }
    }
    let mut current = exe_dir;
    for _ in 0..6 {
        let Some(dir) = current.clone() else { break };
        let candidate = dir.join("sdk").join(arch);
        if candidate.join(library).exists() {
            return candidate.to_string_lossy().to_string();
        }
        current = dir.parent().map(|parent| parent.to_path_buf());
    }
    format!("sdk/{}", arch)
}
