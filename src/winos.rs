//! Windows 专用能力：开机自启（HKCU Run）与「子进程随主进程退出」（作业对象）。

use std::ffi::c_void;
use std::path::Path;
use std::sync::OnceLock;

use log::{error, info, warn};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_SUCCESS,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentProcess, Sleep};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, IsIconic, MessageBoxW, SetForegroundWindow, ShowWindow, IDYES, MB_ICONERROR,
    MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO, SW_RESTORE, SW_SHOW,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "LedSdkTauri";
const SHORTCUT_NAME: &str = "LedSdk.lnk";
const SINGLE_INSTANCE_MUTEX: &str = "LedSdkTauriSingleInstance";
const MAIN_WINDOW_TITLE: &str = "LedSdk 屏幕节目上传服务";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 单实例保护：已有实例时尝试恢复并前置它的主窗口，返回 false 让新进程退出。
///
/// 托盘模式下关闭窗口只是隐藏；用户再次点桌面图标时，如果直接启动第二个进程，
/// SDK 的 HTTP 服务会因端口被首个进程占用而启动失败，表现为窗口一闪就消失。
pub fn ensure_single_instance() -> bool {
    let name = wide(SINGLE_INSTANCE_MUTEX);
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        warn!(
            "创建单实例互斥体失败，继续启动：{}",
            std::io::Error::last_os_error()
        );
        return true;
    }
    if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
        // 句柄故意不关闭：进程退出时由系统释放，互斥体随之消失。
        return true;
    }
    unsafe { CloseHandle(handle) };

    // 首个实例可能刚启动、窗口尚未创建，短暂重试后再提示。
    let title = wide(MAIN_WINDOW_TITLE);
    for _ in 0..10 {
        let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
        if !hwnd.is_null() {
            unsafe {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                } else {
                    ShowWindow(hwnd, SW_SHOW);
                }
                SetForegroundWindow(hwnd);
            }
            return false;
        }
        unsafe { Sleep(100) };
    }

    show_message(
        "LedSdk 已在运行",
        "LedSdk 已在运行，请从系统托盘图标打开界面。",
        false,
    );
    false
}

/// 显示启动失败等错误提示：GUI 版没有控制台，必须让用户看到失败原因。
pub fn show_error_dialog(title: &str, message: &str) {
    show_message(title, message, true);
}

fn show_message(title: &str, message: &str, is_error: bool) {
    let title = wide(title);
    let message = wide(message);
    let icon = if is_error {
        MB_ICONERROR
    } else {
        MB_ICONINFORMATION
    };
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | icon,
        );
    }
}

/// 询问是否退出到托盘模式：true 表示隐藏到托盘，false 表示直接退出。
pub fn confirm_yes_no(title: &str, message: &str) -> bool {
    let title = wide(title);
    let message = wide(message);
    let result = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        )
    };
    result == IDYES
}

/// 把当前进程放进「关闭即杀」的作业对象：崩溃 / 被强杀时 ffmpeg、TextRenderer 一并结束。
pub fn assign_current_process() -> bool {
    static JOB: OnceLock<usize> = OnceLock::new();
    let job = *JOB.get_or_init(|| unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            error!("创建作业对象失败：{}", std::io::Error::last_os_error());
            return 0;
        }

        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
        let rc = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const c_void,
            size,
        );
        if rc == 0 {
            error!("设置作业对象失败：{}", std::io::Error::last_os_error());
            CloseHandle(job);
            return 0;
        }

        if AssignProcessToJobObject(job, GetCurrentProcess()) == 0 {
            error!("加入作业对象失败：{}", std::io::Error::last_os_error());
            CloseHandle(job);
            return 0;
        }

        // 句柄故意不关闭：进程结束时由系统关闭，从而触发 KILL_ON_JOB_CLOSE。
        job as usize
    });
    job != 0
}

/// 同步开机自启项：与配置一致就不动，不一致才写 / 删。
pub fn sync_autostart(enabled: bool) {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            warn!("读取当前可执行文件路径失败，跳过开机自启：{err}");
            return;
        }
    };

    let mut key: HKEY = std::ptr::null_mut();
    let sub_key = wide(RUN_KEY);
    let rc = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            sub_key.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_READ | KEY_SET_VALUE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        )
    };
    if rc != ERROR_SUCCESS {
        error!("打开开机自启注册表项失败：错误码 {rc}");
        return;
    }

    let name = wide(VALUE_NAME);
    let desired = format!("\"{}\"", exe.display());

    if enabled {
        let current = read_value(key, &name);
        if current.as_deref() != Some(desired.as_str()) {
            let data = wide(&desired);
            let rc = unsafe {
                RegSetValueExW(
                    key,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr() as *const u8,
                    (data.len() * 2) as u32,
                )
            };
            if rc == ERROR_SUCCESS {
                info!("已登记开机自启动：{VALUE_NAME} = {desired}");
            } else {
                error!("写入开机自启失败：错误码 {rc}");
            }
        } else {
            info!("开机自启动项已是最新：{VALUE_NAME} = {desired}");
        }
    } else if read_value(key, &name).is_some() {
        let rc = unsafe { RegDeleteValueW(key, name.as_ptr()) };
        if rc == ERROR_SUCCESS {
            info!("已按配置关闭开机自启动（删除 HKCU Run 的 {VALUE_NAME}）。");
        } else {
            error!("删除开机自启项失败：错误码 {rc}");
        }
    } else {
        info!("开机自启动未启用（配置 AutoStart=false，HKCU Run 无 {VALUE_NAME}），无需处理。");
    }

    unsafe { RegCloseKey(key) };
}

fn read_value(key: HKEY, name: &[u16]) -> Option<String> {
    let mut kind = 0u32;
    let mut size = 0u32;
    let rc = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if rc != ERROR_SUCCESS || size == 0 {
        return None;
    }
    let mut buffer = vec![0u8; size as usize];
    let rc = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            buffer.as_mut_ptr(),
            &mut size,
        )
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let raw: Vec<u16> = buffer
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|value| *value != 0)
        .collect();
    Some(String::from_utf16_lossy(&raw))
}

/// 在用户桌面创建/更新快捷方式。返回 true 表示快捷方式存在且指向当前 exe。
pub fn sync_desktop_shortcut(enabled: bool) -> bool {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            warn!("读取当前可执行文件路径失败，跳过桌面快捷方式：{err}");
            return false;
        }
    };
    let Some(desktop) = desktop_directory() else {
        warn!("无法定位桌面目录，跳过桌面快捷方式。");
        return false;
    };
    let shortcut = desktop.join(SHORTCUT_NAME);
    if !enabled {
        if shortcut.exists() {
            match read_shortcut_target(&shortcut) {
                Ok(target) if !target.as_os_str().is_empty() && same_path(&target, &exe) => {
                    match std::fs::remove_file(&shortcut) {
                        Ok(()) => info!("已按配置删除桌面快捷方式：{}", shortcut.display()),
                        Err(err) => warn!("删除桌面快捷方式失败 {}：{err}", shortcut.display()),
                    }
                }
                Ok(target) if !target.as_os_str().is_empty() => {
                    info!(
                        "桌面快捷方式指向其他程序，保留：{} → {}",
                        shortcut.display(),
                        target.display()
                    );
                }
                Ok(_) => warn!("桌面快捷方式目标为空，保留：{}", shortcut.display()),
                Err(err) => {
                    warn!("读取桌面快捷方式失败，保留 {}：{err}", shortcut.display());
                }
            }
        }
        return !shortcut.exists();
    }
    match write_shortcut(&shortcut, &exe) {
        Ok(()) => {
            info!("桌面快捷方式已就绪：{}", shortcut.display());
            true
        }
        Err(err) => {
            warn!("创建桌面快捷方式失败 {}：{err}", shortcut.display());
            false
        }
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = left.to_string_lossy();
    let right = right.to_string_lossy();
    left.eq_ignore_ascii_case(&right)
}

fn read_shortcut_target(path: &Path) -> windows::core::Result<std::path::PathBuf> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let initialized = hr.is_ok();
    if hr.is_err() && hr.0 != windows::Win32::Foundation::RPC_E_CHANGED_MODE.0 {
        return Err(windows::core::Error::from(hr));
    }
    let _guard = ComGuard(initialized);

    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
    let persist: IPersistFile = link.cast()?;
    let path_text = HSTRING::from(path.as_os_str());
    unsafe { persist.Load(&path_text, STGM_READ)? };

    let mut buffer = [0u16; 1024];
    unsafe { link.GetPath(&mut buffer, std::ptr::null_mut(), 0)? };
    let len = buffer
        .iter()
        .position(|&value| value == 0)
        .unwrap_or(buffer.len());
    if len == 0 {
        return Ok(std::path::PathBuf::new());
    }
    Ok(std::path::PathBuf::from(String::from_utf16_lossy(
        &buffer[..len],
    )))
}

fn desktop_directory() -> Option<std::path::PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    let path = unsafe { SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None) }.ok()?;
    let text = unsafe { path.to_string().ok() };
    unsafe { CoTaskMemFree(Some(path.as_ptr() as *const c_void)) };
    text.map(std::path::PathBuf::from)
}

fn write_shortcut(path: &Path, exe: &Path) -> windows::core::Result<()> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    // RPC_E_CHANGED_MODE：当前线程已用其他模型初始化 COM，仍可继续使用。
    let initialized = hr.is_ok();
    if hr.is_err() && hr.0 != windows::Win32::Foundation::RPC_E_CHANGED_MODE.0 {
        return Err(windows::core::Error::from(hr));
    }
    let _guard = ComGuard(initialized);

    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
    let exe_text = HSTRING::from(exe.as_os_str());
    let workdir = exe.parent().unwrap_or_else(|| Path::new("."));
    let workdir_text = HSTRING::from(workdir.as_os_str());
    let description = HSTRING::from("LedSdk 屏幕节目上传服务");
    unsafe {
        link.SetPath(&exe_text)?;
        link.SetWorkingDirectory(&workdir_text)?;
        link.SetDescription(&description)?;
        link.SetIconLocation(&exe_text, 0)?;
        let persist: windows::Win32::System::Com::IPersistFile = link.cast()?;
        let path_text = HSTRING::from(path.as_os_str());
        persist.Save(&path_text, true)?;
    }
    Ok(())
}
