//! 跨模块共享的数据类型。

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::ffi;

/// 搜索快照里的一台设备（含「命中网卡」的本机地址）。
#[derive(Clone, Debug)]
pub struct DeviceSnapshot {
    pub device_id: String,
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub device_type: u32,
    pub online: bool,
    pub local_address: String,
}

/// 任务终态（导出/发送）。
#[derive(Clone, Debug)]
pub struct Outcome {
    pub state: u32,
    pub result: i32,
    pub message: String,
}

impl Outcome {
    pub fn succeeded(&self) -> bool {
        self.state == ffi::task_state::SUCCEEDED
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "PascalCase")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub job_id: String,
    pub state: JobState,
    pub progress: i32,
    pub message: String,
    pub device_id: String,
    pub file_name: String,
    pub file_type: String,
    pub package_path: Option<String>,
    pub error: Option<String>,
    pub created_utc: String,
    pub updated_utc: String,
}

/// 上传请求体（multipart 的 `request` 字段）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadJobRequest {
    pub file_name: String,
    pub file_type: String,
    pub device_id: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn now_iso() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}
