//! LedSDK 的 C ABI 绑定：结构体布局、常量、函数声明。
//!
//! 布局与 `sdk/ledsdk.h` 一一对应（x64 布局），`size`/`version` 由 [`init`] 统一填好。
//! 所有字符串都是 UTF-8 的 `const char*`，句柄是 `void*`。

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::{c_char, c_void};
use std::mem::size_of;

pub const STRUCT_VERSION: u32 = 1;

pub type Handle = *mut c_void;

pub const NULL_HANDLE: Handle = std::ptr::null_mut();

// ---------------------------------------------------------------------------
// 结果码 / 枚举常量
// ---------------------------------------------------------------------------

pub mod result {
    pub const OK: i32 = 0;
    pub const IN_PROGRESS: i32 = 1;
    pub const CANCELLED: i32 = 2;
    pub const INVALID_ARGUMENT: i32 = -1;
    pub const INVALID_STATE: i32 = -4;
    pub const NOT_SUPPORTED: i32 = -11;
    pub const BUFFER_TOO_SMALL: i32 = -17;
    pub const INTERNAL_ERROR: i32 = -100;
}

pub mod task_type {
    pub const SEARCH: u32 = 1;
    pub const EXPORT: u32 = 2;
    pub const SEND: u32 = 3;
}

pub mod task_state {
    pub const QUEUED: u32 = 0;
    pub const RUNNING: u32 = 1;
    pub const SUCCEEDED: u32 = 2;
    pub const FAILED: u32 = 3;
    pub const CANCELLED: u32 = 4;

    pub fn is_terminal(state: u32) -> bool {
        state >= SUCCEEDED
    }
}

/// 设备/地址条目的在线标志（`ledsdk_device_info_t.flags` / `ledsdk_device_address_t.flags`）。
pub mod device_flag {
    pub const ONLINE: u32 = 0x1;
    pub const OFFLINE: u32 = 0x2;
}

pub mod http_flag {
    pub const ALLOW_RANGE: u32 = 0x1;
    pub const ALLOW_ABSOLUTE_PATH: u32 = 0x8;
}

pub mod file_type {
    pub const LIST: u32 = 0x00;
}

pub mod media_type {
    pub const PICTURE: u32 = 1;
    pub const GIF: u32 = 2;
    pub const VIDEO: u32 = 3;
    pub const SINGLE_LINE_TEXT: u32 = 5;
    pub const MULTI_LINE_TEXT: u32 = 6;
}

pub mod play_mode {
    pub const LOOP: u32 = 0;
}

pub mod text_align {
    pub const HCENTER: u32 = 0x02;
    pub const VCENTER: u32 = 0x20;
}

pub mod text_color {
    pub const SINGLE: u32 = 0;
}

pub mod effect {
    pub const STATIC: u32 = 0;
}

// ---------------------------------------------------------------------------
// 结构体
// ---------------------------------------------------------------------------

pub type LogCallback = Option<unsafe extern "C" fn(level: u32, message: *const c_char, user: *mut c_void)>;
pub type TaskCallback =
    Option<unsafe extern "C" fn(task: Handle, event: *const LedsdkTaskEvent, user: *mut c_void)>;
pub type DeviceCallback =
    Option<unsafe extern "C" fn(search: Handle, device: *const LedsdkDeviceInfo, user: *mut c_void)>;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct LedsdkConfig {
    pub size: u32,
    pub version: u32,
    pub worker_count: u32,
    pub flags: u32,
    pub log_callback: LogCallback,
    pub log_user_data: *mut c_void,
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LedsdkHttpOptions {
    pub size: u32,
    pub version: u32,
    pub bind_address_utf8: *const c_char,
    pub port: u32,
    pub download_root_utf8: *const c_char,
    pub upload_root_utf8: *const c_char,
    pub max_upload_size: u64,
    pub request_timeout_ms: u32,
    pub flags: u32,
    pub auth_token_utf8: *const c_char,
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct LedsdkHttpStatus {
    pub size: u32,
    pub version: u32,
    pub running: u32,
    pub port: u32,
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct LedsdkSearchOptions {
    pub size: u32,
    pub version: u32,
    pub timeout_ms: u32,
    pub flags: u32,
    pub broadcast_port: u32,
    pub reserved: [u32; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LedsdkDeviceInfo {
    pub size: u32,
    pub version: u32,
    pub device_id_utf8: *const c_char,
    pub name_utf8: *const c_char,
    pub address_utf8: *const c_char,
    pub port: u32,
    pub device_type: u32,
    pub flags: u32,
    pub last_seen_ms: u64,
    pub online: u32,
    pub address_count: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LedsdkDeviceAddress {
    pub size: u32,
    pub version: u32,
    pub address_utf8: *const c_char,
    pub local_address_utf8: *const c_char,
    pub port: u32,
    pub flags: u32,
    pub last_seen_ms: u64,
    pub reserved: [u32; 2],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LedsdkPlayRule {
    pub size: u32,
    pub version: u32,
    pub mode: u32,
    pub count: u32,
    pub duration_seconds: u32,
    pub valid_from_utf8: *const c_char,
    pub valid_to_utf8: *const c_char,
    pub period_start_utf8: *const c_char,
    pub period_end_utf8: *const c_char,
    pub week_mask: u32,
    pub flags: u32,
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LedsdkFontOptions {
    pub size: u32,
    pub version: u32,
    pub family_utf8: *const c_char,
    pub point_size: u32,
    pub bold: u32,
    pub italic: u32,
    pub underline: u32,
    pub strikeout: u32,
    pub color_argb: u32,
    pub mode_3d: u32,
    pub outline: u32,
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LedsdkTextOptions {
    pub size: u32,
    pub version: u32,
    pub html_content_utf8: *const c_char,
    pub font: LedsdkFontOptions,
    pub kerning: u32,
    pub reserved_legacy_leading: u32,
    pub color_mode: u32,
    pub reserved_legacy_transform: [u32; 3],
    pub alignment: u32,
    pub mirror: u32,
    pub reserved: [u32; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LedsdkExportOptions {
    pub size: u32,
    pub version: u32,
    pub output_root_utf8: *const c_char,
    pub download_port: u32,
    pub flags: u32,
    pub usb_capacity: u64,
    pub license_file_utf8: *const c_char,
    pub screen_param_file_utf8: *const c_char,
    pub full_color_code_file_utf8: *const c_char,
    pub low_grey_code_file_utf8: *const c_char,
    pub transcode_cache_root_utf8: *const c_char,
    pub download_address_utf8: *const c_char,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LedsdkSendOptions {
    pub size: u32,
    pub version: u32,
    pub file_type: u32,
    pub retry_count: u32,
    pub retry_backoff_ms: u32,
    pub flags: u32,
    pub password_utf8: *const c_char,
    pub md5_hex_utf8: *const c_char,
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LedsdkTaskEvent {
    pub size: u32,
    pub version: u32,
    pub type_: u32,
    pub state: u32,
    pub progress: u32,
    pub result: i32,
    pub task_id: u64,
    pub message_utf8: *const c_char,
    pub data: *const c_void,
    pub data_size: u32,
    pub reserved: [u32; 2],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct LedsdkTaskStatus {
    pub size: u32,
    pub version: u32,
    pub type_: u32,
    pub state: u32,
    pub progress: u32,
    pub result: i32,
    pub task_id: u64,
    pub reserved: [u32; 2],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct LedsdkRuntimeStatus {
    pub size: u32,
    pub version: u32,
    pub running_services: u32,
    pub active_task_count: u32,
    pub search_task_count: u32,
    pub export_task_count: u32,
    pub send_task_count: u32,
    pub command_task_count: u32,
    pub reserved: [u32; 4],
}

/// 创建一个零初始化、`size`/`version` 已填好的结构体。
pub fn init<T>(fill: impl FnOnce(&mut T)) -> T {
    // SAFETY: 全部字段都是 POD（指针/整数），零值合法。
    let mut value: T = unsafe { std::mem::zeroed() };
    fill(&mut value);
    value
}

pub fn init_versioned<T>(fill: impl FnOnce(&mut T)) -> T {
    let size = size_of::<T>() as u32;
    let mut value: T = unsafe { std::mem::zeroed() };
    // SAFETY: 所有版本化结构体都以 size/version 开头。
    unsafe {
        let head = &mut value as *mut T as *mut u32;
        head.write(size);
        head.add(1).write(STRUCT_VERSION);
    }
    fill(&mut value);
    value
}

// ---------------------------------------------------------------------------
// 函数声明
// ---------------------------------------------------------------------------

// Windows / Linux x64：按 C ABI 链接 SDK（build.rs 负责 link-search）。
// 其它平台：同一批符号编译成桩函数，一律返回 NOT_SUPPORTED —— 代码可以编译、可启动，
// 但建树/导出/发送会明确失败（厂商只提供 Windows x64 与 Linux x64 版）。
macro_rules! sdk_api {
    ($( pub fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> i32; )*) => {
        #[cfg(any(windows, target_os = "linux"))]
        #[link(name = "LedSDK")]
        extern "C" {
            $( pub fn $name($($arg: $ty),*) -> i32; )*
        }

        $(
            #[cfg(not(any(windows, target_os = "linux")))]
            #[no_mangle]
            pub unsafe extern "C" fn $name($($arg: $ty),*) -> i32 {
                let _ = ($($arg,)*);
                result::NOT_SUPPORTED
            }
        )*
    };
}

sdk_api! {
    pub fn ledsdk_get_api_version(
        major: *mut u32,
        minor: *mut u32,
        patch: *mut u32,
        abi_version: *mut u32,
    ) -> i32;
    pub fn ledsdk_result_message(
        result: i32,
        buffer: *mut c_char,
        capacity: u32,
        required_size: *mut u32,
    ) -> i32;
    pub fn ledsdk_initialize(config: *const LedsdkConfig, out_context: *mut Handle) -> i32;
    pub fn ledsdk_shutdown(context: Handle) -> i32;
    pub fn ledsdk_get_last_error(
        context: Handle,
        buffer: *mut c_char,
        capacity: u32,
        required_size: *mut u32,
    ) -> i32;
    pub fn ledsdk_get_runtime_status(context: Handle, status: *mut LedsdkRuntimeStatus) -> i32;

    pub fn ledsdk_task_get_status(task: Handle, status: *mut LedsdkTaskStatus) -> i32;
    pub fn ledsdk_task_cancel(task: Handle) -> i32;
    pub fn ledsdk_task_cancel_all(context: Handle) -> i32;
    pub fn ledsdk_task_release(task: Handle) -> i32;

    pub fn ledsdk_http_create(
        context: Handle,
        options: *const LedsdkHttpOptions,
        out_http: *mut Handle,
    ) -> i32;
    pub fn ledsdk_http_start(http: Handle) -> i32;
    pub fn ledsdk_http_stop(http: Handle) -> i32;
    pub fn ledsdk_http_get_status(http: Handle, status: *mut LedsdkHttpStatus) -> i32;
    pub fn ledsdk_http_release(http: Handle) -> i32;

    pub fn ledsdk_search_create(
        context: Handle,
        options: *const LedsdkSearchOptions,
        device_callback: DeviceCallback,
        task_callback: TaskCallback,
        user_data: *mut c_void,
        out_search: *mut Handle,
    ) -> i32;
    pub fn ledsdk_search_start(search: Handle) -> i32;
    pub fn ledsdk_search_cancel(search: Handle) -> i32;
    pub fn ledsdk_search_get_device_count(search: Handle, count: *mut u32) -> i32;
    pub fn ledsdk_search_get_device(search: Handle, index: u32, device: *mut LedsdkDeviceInfo) -> i32;
    pub fn ledsdk_search_get_device_address_count(
        search: Handle,
        device_index: u32,
        count: *mut u32,
    ) -> i32;
    pub fn ledsdk_search_get_device_address(
        search: Handle,
        device_index: u32,
        address_index: u32,
        address: *mut LedsdkDeviceAddress,
    ) -> i32;
    pub fn ledsdk_search_release(search: Handle) -> i32;

    pub fn ledsdk_playlist_create(
        context: Handle,
        width: u32,
        height: u32,
        device_type: u32,
        flags: u32,
        out_playlist: *mut Handle,
    ) -> i32;
    pub fn ledsdk_program_create(
        playlist: Handle,
        name_utf8: *const c_char,
        background_color_utf8: *const c_char,
        out_program: *mut Handle,
    ) -> i32;
    pub fn ledsdk_playlist_add_program(
        playlist: Handle,
        program: Handle,
        rule: *const LedsdkPlayRule,
        out_order: *mut u32,
    ) -> i32;
    pub fn ledsdk_playlist_set_sync_window(
        playlist: Handle,
        enabled: u32,
        start_seconds: u32,
        end_seconds: u32,
    ) -> i32;
    pub fn ledsdk_playlist_validate(
        playlist: Handle,
        message: *mut c_char,
        capacity: u32,
        required_size: *mut u32,
    ) -> i32;
    pub fn ledsdk_playlist_set_display_attributes(
        playlist: Handle,
        description_utf8: *const c_char,
        rotate_angle: u32,
        discount_width: u32,
    ) -> i32;
    pub fn ledsdk_program_set_play_rule(program: Handle, rule: *const LedsdkPlayRule) -> i32;
    pub fn ledsdk_program_add_region(
        program: Handle,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        flags: u32,
        out_region: *mut Handle,
    ) -> i32;
    pub fn ledsdk_region_set_geometry(
        region: Handle,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> i32;
    pub fn ledsdk_region_set_attributes(
        region: Handle,
        name_utf8: *const c_char,
        alpha: u32,
        level: i32,
    ) -> i32;
    pub fn ledsdk_region_add_media(
        region: Handle,
        media_type: u32,
        source_path_utf8: *const c_char,
        out_media: *mut Handle,
    ) -> i32;
    pub fn ledsdk_media_set_attributes(
        media: Handle,
        name_utf8: *const c_char,
        source_path_utf8: *const c_char,
        content_utf8: *const c_char,
    ) -> i32;
    pub fn ledsdk_media_set_timing(
        media: Handle,
        duration_seconds: i32,
        repeat_times: u32,
        end_to_end: u32,
    ) -> i32;
    pub fn ledsdk_media_set_effects(
        media: Handle,
        in_effect: u32,
        in_speed: u32,
        out_effect: u32,
        out_speed: u32,
    ) -> i32;
    pub fn ledsdk_media_set_transform(media: Handle, rotate_angle: u32, hv_ratio: u32) -> i32;
    pub fn ledsdk_media_set_text_options(media: Handle, options: *const LedsdkTextOptions) -> i32;
    pub fn ledsdk_media_release(media: Handle) -> i32;
    pub fn ledsdk_region_release(region: Handle) -> i32;
    pub fn ledsdk_program_release(program: Handle) -> i32;
    pub fn ledsdk_playlist_release(playlist: Handle) -> i32;

    pub fn ledsdk_playlist_export(
        context: Handle,
        playlist: Handle,
        options: *const LedsdkExportOptions,
        callback: TaskCallback,
        user_data: *mut c_void,
        out_task: *mut Handle,
    ) -> i32;
    pub fn ledsdk_send_program(
        context: Handle,
        device_id_utf8: *const c_char,
        package_path_utf8: *const c_char,
        options: *const LedsdkSendOptions,
        callback: TaskCallback,
        user_data: *mut c_void,
        out_task: *mut Handle,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

pub fn is_success(result: i32) -> bool {
    result == result::OK || result == result::IN_PROGRESS
}

/// 结果码 → 文本（调用方缓冲区 + required_size 重分配）。
pub fn result_message(result: i32) -> String {
    read_into_buffer(|buf, cap, req| unsafe { ledsdk_result_message(result, buf, cap, req) })
}

/// 读取 context 上最近一次错误（同上）。
pub fn last_error(context: Handle) -> String {
    read_into_buffer(|buf, cap, req| unsafe { ledsdk_get_last_error(context, buf, cap, req) })
}

/// 通用「先探测所需长度、再读」的封装。
pub fn read_into_buffer(mut call: impl FnMut(*mut c_char, u32, *mut u32) -> i32) -> String {
    let mut required: u32 = 0;
    let mut buffer = vec![0u8; 1024];
    let mut rc = call(buffer.as_mut_ptr() as *mut c_char, buffer.len() as u32, &mut required);

    if rc == result::BUFFER_TOO_SMALL && required > buffer.len() as u32 {
        buffer = vec![0u8; required as usize];
        rc = call(buffer.as_mut_ptr() as *mut c_char, buffer.len() as u32, &mut required);
    }

    if !is_success(rc) {
        return format!("result={rc}");
    }

    let end = buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..end]).into_owned()
}
