//! Windows 专用能力：开机自启（HKCU Run）与「子进程随主进程退出」（作业对象）。

use std::ffi::c_void;
use std::sync::OnceLock;

use log::{error, info, warn};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "LedSdkTauri";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
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
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|value| *value != 0)
        .collect();
    Some(String::from_utf16_lossy(&raw))
}