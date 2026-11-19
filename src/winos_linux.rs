//! 非 Windows 平台的占位实现。
//!
//! Windows 用 Job Object 让子进程随主进程退出、用 HKCU Run 做开机自启；
//! 本文件为 Linux / macOS 提供同签名占位，不执行实际操作，只给出提示。

use log::info;

/// 非 Windows 平台没有 Windows 作业对象，也不需要靠它约束 SDK 子进程。
pub fn assign_current_process() -> bool {
    false
}

/// 开机自启在非 Windows 上未实现，仅给出平台对应的提示。
pub fn sync_autostart(enabled: bool) {
    if !enabled {
        return;
    }
    #[cfg(target_os = "macos")]
    info!("配置 AutoStart=true，但 macOS 版未实现开机自启；可用 launchd LaunchAgent 替代。");
    #[cfg(not(target_os = "macos"))]
    info!("配置 AutoStart=true，但 Linux 版未实现开机自启；可用 systemd user unit 替代。");
}
