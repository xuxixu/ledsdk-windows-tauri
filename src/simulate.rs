//! 模拟测试：3 台设备 × 文字/图片/视频/GIF，走完整的「上传 → 建树 → 校验 → 导出」链路，
//! 不需要真实屏幕（ExportOnly=true）。运行：`ledsdk-tauri.exe --simulate`

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::config::{AppHostSettings, AppSettings, ScreenProfile, ScreenSource, SdkSettings, UploadApiSettings};

const API_ADDR: &str = "127.0.0.1:5199";
const DOWNLOAD_PORT: u16 = 28081;
/// 模拟测试固定启用 token：顺带覆盖鉴权中间件（无/错 token 必须 401）。
const SIM_TOKEN: &str = "sim-token";

pub fn run() -> i32 {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let run_dir = std::env::temp_dir()
        .join("ledsdk-tauri-simulate")
        .join(format!("run-{stamp}"));
    if let Err(err) = std::fs::create_dir_all(&run_dir) {
        println!("创建模拟目录失败：{err}");
        return 1;
    }
    println!("模拟目录：{}", run_dir.display());

    let bin_directory = crate::selfcheck::default_bin_directory();
    let assets = match generate_assets(Path::new(&bin_directory), &run_dir) {
        Ok(assets) => assets,
        Err(err) => {
            println!("生成测试素材失败：{err}");
            return 1;
        }
    };

    let config_path = run_dir.join("appsettings.json");
    let settings = simulate_settings(&bin_directory, &run_dir);
    if let Err(err) = crate::config::save(&config_path, &settings) {
        println!("写配置失败：{err}");
        return 1;
    }

    let runtime = match crate::runtime::Runtime::start(&config_path, &run_dir.join("logs"), false) {
        Ok(runtime) => runtime,
        Err(err) => {
            println!("启动失败：{err}");
            return 1;
        }
    };

    let state = crate::server::ApiState {
        host: runtime.host.clone(),
        registry: runtime.registry.clone(),
        jobs: runtime.jobs.clone(),
        listen_address: "127.0.0.1".to_string(),
        port: 5199,
        token: Some(SIM_TOKEN.to_string()),
    };
    let http_shutdown = runtime.http_shutdown.clone();
    std::thread::spawn(move || {
        let tokio_runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(err) => {
                println!("创建异步运行时失败：{err}");
                return;
            }
        };
        if let Err(err) = tokio_runtime.block_on(crate::server::serve(state, http_shutdown)) {
            println!("上传接口异常退出：{err}");
        }
    });

    if let Err(err) = wait_port(API_ADDR, Duration::from_secs(20)) {
        println!("等待上传接口失败：{err}");
        runtime.stop();
        return 1;
    }

    let mut passed = 0;
    let mut failed = 0;
    let cases: [(&str, &str, PathBuf); 4] = [
        ("text", "text", assets.text.clone()),
        ("picture", "picture", assets.picture.clone()),
        ("video", "video", assets.video.clone()),
        ("gif", "gif", assets.gif.clone()),
    ];
    let screens = [(256, 256), (320, 240), (128, 128)];
    let names = ["sim-device-a", "sim-device-b", "sim-device-c"];

    for (index, device_id) in names.iter().enumerate() {
        let (width, height) = screens[index];
        for (kind, file_type, path) in &cases {
            match run_case(device_id, kind, file_type, path, width, height, &run_dir) {
                Ok(checks) => {
                    passed += checks;
                    println!("  [ok] {device_id} {kind} {width}x{height}");
                }
                Err(err) => {
                    failed += 1;
                    println!("  [FAIL] {device_id} {kind}：{err}");
                }
            }
        }
    }

    // 未知设备必须被拒绝。
    match post_program("no-such-device", "picture", &assets.picture, 0, 0, 64, 64, &run_dir) {
        Ok((status, _)) if status == 400 => {
            passed += 1;
            println!("  [ok] 未知设备返回 400");
        }
        Ok((status, body)) => {
            failed += 1;
            println!("  [FAIL] 未知设备返回 {status}：{body}");
        }
        Err(err) => {
            failed += 1;
            println!("  [FAIL] 未知设备请求失败：{err}");
        }
    }

    // 鉴权回归：配置了 token 时，写接口无 token / 错 token 都必须 401。
    for (label, result) in [
        (
            "无 token",
            request_anonymous(
                API_ADDR,
                "POST",
                "/api/program",
                Some("multipart/form-data; boundary=x"),
                b"",
            ),
        ),
        (
            "错 token",
            request_bad_token(
                API_ADDR,
                "POST",
                "/api/program",
                Some("multipart/form-data; boundary=x"),
                b"",
            ),
        ),
    ] {
        match result {
            Ok((401, _)) => passed += 1,
            Ok((status, body)) => {
                failed += 1;
                println!("  [FAIL] {label} 返回 {status}，期望 401：{body}");
            }
            Err(err) => {
                failed += 1;
                println!("  [FAIL] {label} 请求失败：{err}");
            }
        }
    }

    // 屏幕清单接口必须列出 3 台设备。
    match request(API_ADDR, "GET", "/api/screens", None, &[]) {
        Ok((200, body)) => {
            let listed = body.matches("\"deviceId\"").count();
            if listed == 3 {
                passed += 1;
            } else {
                failed += 1;
                println!("  [FAIL] /api/screens 列出 {listed} 台，期望 3 台");
            }
        }
        Ok((status, _)) => {
            failed += 1;
            println!("  [FAIL] /api/screens 返回 {status}");
        }
        Err(err) => {
            failed += 1;
            println!("  [FAIL] /api/screens 请求失败：{err}");
        }
    }

    // 大文件上传（> 2 MB）：axum 默认 2 MB 上限会直接 400，必须被 DefaultBodyLimit 放宽。
    match run_case("sim-device-a", "big", "picture", &assets.big, 256, 256, &run_dir) {
        Ok(checks) => {
            passed += checks;
            let size = std::fs::metadata(&assets.big).map(|m| m.len()).unwrap_or(0);
            println!("  [ok] 大文件上传（{:.1} MB）", size as f64 / 1024.0 / 1024.0);
        }
        Err(err) => {
            failed += 1;
            println!("  [FAIL] 大文件上传：{err}");
        }
    }

    // 作业处理中关闭：必须有限时间内返回，不能卡在 SDK 原生调用里。
    let mut stopped = false;
    match post_program("sim-device-a", "video", &assets.video, 0, 0, 256, 256, &run_dir) {
        Ok((202, _)) => {
            let started = Instant::now();
            let cleaned = runtime.stop();
            let elapsed = started.elapsed();
            // 不能只看耗时 < 6s：原实现超时就跳过 SDK 清理、约 5 秒返回也会"通过"。
            // 必须同时确认 SDK 真的关掉了（HTTP 服务已停）。
            if elapsed < Duration::from_secs(6) && cleaned && !runtime.host.http_running() {
                passed += 1;
                println!(
                    "  [ok] 作业处理中关闭耗时 {:.1}s（SDK 已清理）",
                    elapsed.as_secs_f64()
                );
            } else {
                failed += 1;
                println!(
                    "  [FAIL] 作业处理中关闭耗时 {:.1}s，cleaned={cleaned}，http_running={}",
                    elapsed.as_secs_f64(),
                    runtime.host.http_running()
                );
            }
            stopped = true;
        }
        Ok((status, body)) => {
            failed += 1;
            println!("  [FAIL] 关闭测试上传返回 {status}：{body}");
        }
        Err(err) => {
            failed += 1;
            println!("  [FAIL] 关闭测试上传失败：{err}");
        }
    }

    println!(
        "SIMULATE {}（{passed} 项通过，{failed} 项失败）",
        if failed == 0 { "PASSED" } else { "FAILED" }
    );

    if !stopped {
        runtime.stop();
    }
    if failed == 0 {
        0
    } else {
        1
    }
}

fn simulate_settings(bin_directory: &str, run_dir: &Path) -> AppSettings {
    let make = |name: &str, device_id: &str, width: i32, height: i32, device_type: u32| ScreenProfile {
        name: name.to_string(),
        device_id: device_id.to_string(),
        ip: String::new(),
        port: 20003,
        device_type,
        width,
        height,
        rotate: 0,
        local_address: String::new(),
        last_seen: None,
        online: false,
        source: ScreenSource::Manual,
    };

    AppSettings {
        sdk: SdkSettings {
            bin_directory: bin_directory.to_string(),
            bind_address: "127.0.0.1".to_string(),
            download_port: DOWNLOAD_PORT,
            export_root: run_dir.join("export").to_string_lossy().to_string(),
            upload_root: run_dir.join("upload").to_string_lossy().to_string(),
            transcode_cache_root: run_dir.join("cache").to_string_lossy().to_string(),
            export_only: true,
            ..Default::default()
        },
        upload_api: UploadApiSettings {
            listen_address: "127.0.0.1".to_string(),
            port: 5199,
            token: String::new(),
        },
        app: AppHostSettings {
            auto_start: false,
            retention_days: 0,
        },
        screens: vec![
            make("sim-a", "sim-device-a", 256, 256, 51),
            make("sim-b", "sim-device-b", 320, 240, 52),
            make("sim-c", "sim-device-c", 128, 128, 50),
        ],
    }
}

struct Assets {
    text: PathBuf,
    picture: PathBuf,
    video: PathBuf,
    gif: PathBuf,
    /// 故意大于 2 MB（axum 默认请求体上限），用来验证大文件上传不会被 400 掉。
    big: PathBuf,
}

fn generate_assets(bin_directory: &Path, run_dir: &Path) -> Result<Assets, String> {
    let assets_dir = run_dir.join("assets");
    std::fs::create_dir_all(&assets_dir).map_err(|err| err.to_string())?;

    let text = assets_dir.join("text.txt");
    std::fs::write(&text, "LedSdk Rust 模拟测试\n第二行文字").map_err(|err| err.to_string())?;

    // Windows 用包里自带的 ffmpeg.exe；Linux 按 SDK 文档从 PATH 找系统 ffmpeg。
    let ffmpeg = if cfg!(windows) {
        let bundled = bin_directory.join("ffmpeg.exe");
        if !bundled.exists() {
            return Err(format!("未找到 {}", bundled.display()));
        }
        std::path::PathBuf::from(bundled)
    } else {
        std::path::PathBuf::from("ffmpeg")
    };

    let picture = assets_dir.join("picture.png");
    let video = assets_dir.join("video.mp4");
    let gif = assets_dir.join("anim.gif");
    let big = assets_dir.join("big.png");

    run_ffmpeg(
        &ffmpeg,
        &["-y", "-f", "lavfi", "-i", "color=c=blue:size=200x120", "-frames:v", "1", &picture.to_string_lossy()],
    )?;
    run_ffmpeg(
        &ffmpeg,
        &[
            "-y", "-f", "lavfi", "-i", "testsrc2=size=256x256:rate=15:duration=2",
            "-f", "lavfi", "-i", "sine=frequency=440:duration=2",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest",
            &video.to_string_lossy(),
        ],
    )?;
    run_ffmpeg(
        &ffmpeg,
        &[
            "-y", "-f", "lavfi", "-i", "testsrc2=size=256x256:rate=10",
            "-frames:v", "20", &gif.to_string_lossy(),
        ],
    )?;
    // testsrc2 本身有噪声，1600x1200 的 PNG 通常 4 MB 以上，确保超过 2 MB 的默认上限。
    run_ffmpeg(
        &ffmpeg,
        &[
            "-y", "-f", "lavfi", "-i", "testsrc2=size=1600x1200:rate=1,noise=alls=100:allf=t+u",
            "-frames:v", "1", &big.to_string_lossy(),
        ],
    )?;
    let big_size = std::fs::metadata(&big).map(|m| m.len()).unwrap_or(0);
    if big_size < 3 * 1024 * 1024 {
        return Err(format!("大文件素材只有 {big_size} 字节，不够验证 >2 MB 上传"));
    }

    Ok(Assets {
        text,
        picture,
        video,
        gif,
        big,
    })
}

fn run_ffmpeg(ffmpeg: &Path, args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new(ffmpeg)
        .args(args)
        .output()
        .map_err(|err| format!("执行 ffmpeg 失败：{err}"))?;
    if !output.status.success() {
        return Err(format!(
            "ffmpeg 失败：{}",
            String::from_utf8_lossy(&output.stderr).lines().last().unwrap_or("")
        ));
    }
    Ok(())
}

fn run_case(
    device_id: &str,
    kind: &str,
    file_type: &str,
    path: &Path,
    width: i32,
    height: i32,
    run_dir: &Path,
) -> Result<usize, String> {
    let (status, body) = post_program(device_id, file_type, path, 0, 0, width, height, run_dir)?;
    if status != 202 {
        return Err(format!("上传返回 {status}：{body}"));
    }
    let job_id = json_field(&body, "jobId").ok_or_else(|| format!("响应里没有 jobId：{body}"))?;

    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        if Instant::now() > deadline {
            return Err(format!("作业 {job_id} 超时"));
        }
        let (status, body) = request(API_ADDR, "GET", &format!("/api/jobs/{job_id}"), None, &[])
            .map_err(|err| err.to_string())?;
        if status != 200 {
            return Err(format!("查询作业返回 {status}"));
        }
        let state = json_field(&body, "state").unwrap_or_default();
        match state.as_str() {
            "Succeeded" => {
                let mut checks = 1;
                let package = json_field(&body, "packagePath").ok_or("成功作业缺少 packagePath")?;
                let package_path = PathBuf::from(package);
                if !package_path.exists() {
                    return Err(format!("节目包不存在：{}", package_path.display()));
                }
                checks += 1;
                let bytes = std::fs::read(&package_path).map_err(|err| err.to_string())?;
                let text = String::from_utf8_lossy(&bytes);
                if !text.contains(&format!(":{DOWNLOAD_PORT}/")) {
                    return Err("节目包内没有下载端口 URL".to_string());
                }
                checks += 1;
                if text.contains("127.0.0.1") || text.contains("0.0.0.0") {
                    return Err("节目包内出现回环地址".to_string());
                }
                checks += 1;
                if !has_md5(&text) {
                    return Err("节目包内没有 32 位 MD5".to_string());
                }
                checks += 1;
                let _ = (kind, run_dir);
                return Ok(checks);
            }
            "Failed" => {
                let error = json_field(&body, "error").unwrap_or(body);
                return Err(format!("作业失败：{error}"));
            }
            _ => std::thread::sleep(Duration::from_millis(300)),
        }
    }
}

fn has_md5(text: &str) -> bool {
    let bytes = text.as_bytes();
    let is_hex = |b: &u8| b.is_ascii_hexdigit();
    bytes
        .windows(32)
        .any(|w| w.iter().all(is_hex) && !w.iter().any(|b| b.is_ascii_lowercase()))
}

fn json_field(body: &str, field: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    match value.get(field)? {
        Value::String(text) => Some(text.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn post_program(
    device_id: &str,
    file_type: &str,
    path: &Path,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    _run_dir: &Path,
) -> Result<(u16, String), String> {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    // 局部名避免遮蔽同名函数 request。
    let request_json = json!({
        "fileName": file_name,
        "fileType": file_type,
        "deviceId": device_id,
        "x": x,
        "y": y,
        "width": width,
        "height": height,
    })
    .to_string();
    let bytes = std::fs::read(path).map_err(|err| format!("读取素材失败：{err}"))?;

    let boundary = format!("----ledsdk{:x}", crate::jobs::new_id().len());
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"request\"\r\n\r\n").as_bytes());
    body.extend_from_slice(request_json.as_bytes());
    body.extend_from_slice(
        format!(
            "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    request(
        API_ADDR,
        "POST",
        "/api/program",
        Some(&format!("multipart/form-data; boundary={boundary}")),
        &body,
    )
    .map_err(|err| err.to_string())
}

fn request(
    address: &str,
    method: &str,
    path: &str,
    content_type: Option<&str>,
    body: &[u8],
) -> std::io::Result<(u16, String)> {
    send(address, method, path, content_type, body, Some(SIM_TOKEN))
}

/// 不带 Authorization 的请求：用来验证 401。
fn request_anonymous(
    address: &str,
    method: &str,
    path: &str,
    content_type: Option<&str>,
    body: &[u8],
) -> std::io::Result<(u16, String)> {
    send(address, method, path, content_type, body, None)
}

/// 错误的 token：同样必须被拒。
fn request_bad_token(
    address: &str,
    method: &str,
    path: &str,
    content_type: Option<&str>,
    body: &[u8],
) -> std::io::Result<(u16, String)> {
    send(address, method, path, content_type, body, Some("wrong-token"))
}

fn send(
    address: &str,
    method: &str,
    path: &str,
    content_type: Option<&str>,
    body: &[u8],
    token: Option<&str>,
) -> std::io::Result<(u16, String)> {
    let mut stream = std::net::TcpStream::connect(address)?;
    stream.set_read_timeout(Some(Duration::from_secs(300)))?;

    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n");
    if let Some(token) = token {
        head.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if let Some(content_type) = content_type {
        head.push_str(&format!(
            "Content-Type: {content_type}\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    head.push_str("\r\n");

    stream.write_all(head.as_bytes())?;
    if !body.is_empty() {
        stream.write_all(body)?;
    }
    stream.flush()?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;

    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|index| index + 4)
        .unwrap_or(raw.len());
    let head_text = String::from_utf8_lossy(&raw[..split]).to_string();
    let body_text = String::from_utf8_lossy(&raw[split..]).to_string();
    let status = head_text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    Ok((status, body_text))
}

fn wait_port(address: &str, timeout: Duration) -> std::io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if std::net::TcpStream::connect(address).is_ok() {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("{address} 在 {timeout:?} 内没有开始监听"),
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}