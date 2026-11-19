use std::fs;
use std::path::{Path, PathBuf};

/// 构建期做两件事：
/// 1) 把 SDK 的链接目录告给链接器（Windows：LedSDK.lib；Linux：libLedSDK.so）；
/// 2) 把 SDK 运行文件复制到 `<target>/<profile>/`，保证运行时能与可执行文件同目录加载。
fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let sdk = sdk_directory(&manifest);

    // 两个平台都按 C ABI 链接 SDK：Windows 用 LedSDK.lib + LedSDK.dll，Linux 用 libLedSDK.so。
    // 其它平台没有 SDK（ffi.rs 里是桩实现），这里不链接也不搬运行文件。
    if sdk.exists() {
        println!("cargo:rustc-link-search=native={}", sdk.display());
        println!("cargo:rerun-if-changed={}", sdk.display());
        copy_runtime(&sdk);
        if target_os() == "linux" {
            // 链接期按 SONAME（libLedSDK.so.1）找库：把 rpath 指向可执行文件自身目录。
            println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
        }
    }

    // 配置模板跟随到输出目录：运行时读写 <exe目录>/config/appsettings.json。
    let config = manifest.join("config").join("appsettings.json");
    println!("cargo:rerun-if-changed=config/appsettings.json");
    if config.exists() {
        if let Some(profile_dir) = profile_directory() {
            let target = profile_dir.join("config");
            let _ = fs::create_dir_all(&target);
            let target = target.join("appsettings.json");
            if !is_up_to_date(&config, &target) {
                let _ = fs::copy(&config, &target);
            }
        }
    }

    tauri_build::build();
}

fn target_os() -> String {
    std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default()
}

/// Windows 用 `sdk/x64`，Linux 用 `sdk/linux-x64`；其它平台返回一个不存在的路径，
/// 让 `sdk.exists()` 为假（否则 macOS 会误把 Linux 的 ELF 交给链接器）。
fn sdk_directory(manifest: &Path) -> PathBuf {
    let arch = match target_os().as_str() {
        "windows" => "x64",
        "linux" => "linux-x64",
        _ => "__no_sdk_for_this_platform__",
    };
    manifest.join("sdk").join(arch)
}

fn profile_directory() -> Option<PathBuf> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").ok()?);
    // OUT_DIR 的布局随 rustc 版本变化：
    //   旧：<target>/<profile>/build/<pkg>-<hash>/out
    //   新（1.100+）：<target>/<profile>/build/<pkg>/<hash>/out（多一层）
    // 不依赖固定层数，向上找名为 build 的祖先，其父目录即 <target>/<profile>。
    for ancestor in out_dir.ancestors() {
        if ancestor.file_name().and_then(|n| n.to_str()) == Some("build") {
            return ancestor.parent().map(|p| p.to_path_buf());
        }
    }
    None
}

/// 只复制缺失或更旧的大文件，避免每次增量构建都搬 120 MB。
fn copy_runtime(sdk: &Path) {
    // 与 profile_directory() 共用同一套稳健的布局推断，避免 OUT_DIR 层级变化导致复制错目录。
    let profile_dir = match profile_directory() {
        Some(dir) => dir,
        None => return,
    };

    copy_dir(sdk, &profile_dir);
}

fn copy_dir(from: &Path, to: &Path) {
    let entries = match fs::read_dir(from) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if meta.is_dir() {
            let _ = fs::create_dir_all(&dst);
            copy_dir(&src, &dst);
            continue;
        }

        if is_up_to_date(&src, &dst) {
            // 归档还原保留 mtime 但丢了执行位时，is_up_to_date 会命中；
            // 这里仍要补一次 755，否则渲染器一直是 644 → 文本导出权限错误。
            set_executable(&dst, &src);
            continue;
        }

        let _ = fs::copy(&src, &dst);
        set_executable(&dst, &src);
    }
}

/// Linux 上 SDK 的文本渲染器是外部可执行文件：从 Windows 拷贝过来的仓库会丢掉执行位，
/// 复制到 target 后补回 755，否则 SDK 调用会报权限错误。
fn set_executable(dst: &Path, src: &Path) {
    if target_os() != "linux" {
        let _ = dst;
        return;
    }
    let is_renderer = src
        .file_name()
        .map(|name| name == "LedSDKTextRenderer")
        .unwrap_or(false);
    if !is_renderer {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dst, fs::Permissions::from_mode(0o755));
    }
}

fn is_up_to_date(src: &Path, dst: &Path) -> bool {
    let (s, d) = match (fs::metadata(src), fs::metadata(dst)) {
        (Ok(s), Ok(d)) => (s, d),
        _ => return false,
    };
    if s.len() != d.len() {
        return false;
    }
    match (s.modified(), d.modified()) {
        (Ok(st), Ok(dt)) => st <= dt,
        _ => false,
    }
}
