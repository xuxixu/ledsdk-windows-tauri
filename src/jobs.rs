//! 上传作业队列：单消费者串行执行（SDK 要求同一句柄不并发修改、导出期间不能动节目树）。

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use log::{error, info, warn};

use crate::config::ScreenRegistry;
use crate::host::SdkHost;
use crate::models::{now_iso, JobState, JobStatus, UploadJobRequest};
use crate::pipeline::{JobError, JobProcessor};

/// UI 只展示最近这么多条；更早的从内存淘汰（磁盘日志仍有记录）。
const MAX_HISTORY: usize = 200;
/// 关闭时等待 worker 退出的上限。
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

struct Job {
    id: String,
    request: UploadJobRequest,
    staged: PathBuf,
}

enum Message {
    Run(Job),
    Stop,
}

struct QueueInner {
    /// 按入队顺序保存的作业状态，长度上限 MAX_HISTORY。
    jobs: Mutex<VecDeque<JobStatus>>,
    tx: Sender<Message>,
    /// worker 已退出。
    finished: Mutex<bool>,
    finished_cv: Condvar,
    /// stop() 一开始就置位：此后 submit 立即拒绝，避免作业排在 Stop 之后永久 Queued。
    closed: AtomicBool,
}

#[derive(Clone)]
pub struct JobQueue {
    inner: Arc<QueueInner>,
}

impl JobQueue {
    pub fn spawn(host: Arc<SdkHost>, registry: Arc<ScreenRegistry>) -> Self {
        let (tx, rx) = mpsc::channel::<Message>();
        let inner = Arc::new(QueueInner {
            jobs: Mutex::new(VecDeque::new()),
            tx,
            finished: Mutex::new(false),
            finished_cv: Condvar::new(),
            closed: AtomicBool::new(false),
        });
        let worker = inner.clone();
        std::thread::spawn(move || worker_loop(worker, rx, host, registry));
        Self { inner }
    }

    pub fn submit(&self, request: UploadJobRequest, staged: PathBuf) -> JobStatus {
        let id = new_id();
        let now = now_iso();

        // 队列正在关闭（或与 Stop 竞态）时立即拒绝，不能排到 Stop 之后永久 Queued。
        if self.inner.closed.load(Ordering::SeqCst) {
            warn!("作业 {id}：队列正在关闭，拒绝入队。");
            let _ = std::fs::remove_file(&staged);
            return JobStatus {
                job_id: id,
                state: JobState::Failed,
                progress: 0,
                message: "服务正在关闭，拒绝新作业。".to_string(),
                device_id: request.device_id,
                file_name: request.file_name,
                file_type: request.file_type,
                package_path: None,
                error: Some("服务正在关闭，拒绝新作业。".to_string()),
                created_utc: now.clone(),
                updated_utc: now,
            };
        }

        let status = JobStatus {
            job_id: id.clone(),
            state: JobState::Queued,
            progress: 0,
            message: "已入队，等待处理。".to_string(),
            device_id: request.device_id.clone(),
            file_name: request.file_name.clone(),
            file_type: request.file_type.clone(),
            package_path: None,
            error: None,
            created_utc: now.clone(),
            updated_utc: now,
        };

        if let Ok(mut jobs) = self.inner.jobs.lock() {
            jobs.push_back(status.clone());
            while jobs.len() > MAX_HISTORY {
                jobs.pop_front();
            }
        }

        info!(
            "入队：作业={id} 设备={} 文件={} 类型={} 窗口=[{},{},{},{}]",
            request.device_id,
            request.file_name,
            request.file_type,
            request.x,
            request.y,
            request.width,
            request.height
        );

        let job = Job {
            id: id.clone(),
            request,
            staged: staged.clone(),
        };
        if self.inner.tx.send(Message::Run(job)).is_err() {
            self.update(&id, |row| {
                row.state = JobState::Failed;
                row.message = "作业队列已关闭。".to_string();
                row.error = Some("作业队列已关闭。".to_string());
            });
            // 队列已关闭时没人会处理这个作业，暂存文件由提交方负责清理。
            if let Err(err) = std::fs::remove_file(&staged) {
                warn!("作业 {id}：队列已关闭，清理暂存文件失败：{err}");
            }
        }
        self.status(&id).unwrap_or(status)
    }

    pub fn status(&self, id: &str) -> Option<JobStatus> {
        let jobs = self.inner.jobs.lock().ok()?;
        jobs.iter().find(|row| row.job_id == id).cloned()
    }

    pub fn recent(&self, limit: usize) -> Vec<JobStatus> {
        let Ok(jobs) = self.inner.jobs.lock() else {
            return Vec::new();
        };
        let take = limit.min(jobs.len());
        jobs.iter().rev().take(take).cloned().collect()
    }

    /// 停止队列并等待 worker 退出：返回 true 表示 worker 已结束，
    /// 调用方可以安全销毁 SDK；false 表示仍有作业在跑，应放弃清理直接退出。
    pub fn stop(&self) -> bool {
        // 先置 closed：此后并发 submit 立即拒绝，不会排在 Stop 之后。
        self.inner.closed.store(true, Ordering::SeqCst);
        let _ = self.inner.tx.send(Message::Stop);
        let guard = self
            .inner
            .finished
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let (guard, _) = self
            .inner
            .finished_cv
            .wait_timeout_while(guard, STOP_TIMEOUT, |finished| !*finished)
            .unwrap_or_else(|err| err.into_inner());
        if !*guard {
            warn!(
                "作业队列未在 {} 秒内退出（仍在处理作业），本次跳过 SDK 清理。",
                STOP_TIMEOUT.as_secs()
            );
        }
        *guard
    }

    fn update(&self, id: &str, change: impl FnOnce(&mut JobStatus)) {
        update_row(&self.inner, id, change);
    }
}

/// 更新一条作业状态（提交方与 worker 共用）。
fn update_row(inner: &QueueInner, id: &str, change: impl FnOnce(&mut JobStatus)) {
    if let Ok(mut jobs) = inner.jobs.lock() {
        if let Some(row) = jobs.iter_mut().find(|row| row.job_id == id) {
            change(row);
            row.updated_utc = now_iso();
        }
    }
}

fn worker_loop(
    inner: Arc<QueueInner>,
    rx: Receiver<Message>,
    host: Arc<SdkHost>,
    registry: Arc<ScreenRegistry>,
) {
    while let Ok(message) = rx.recv() {
        match message {
            Message::Stop => break,
            Message::Run(job) => {
                let queue = QueueInnerHandle { inner: inner.clone() };
                // 单线程 worker：一次 panic 就会让队列永久停摆（后续作业停在 Queued），
                // 所以每个作业都兜住，失败就落到该作业自己身上。
                let id = job.id.clone();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_job(&queue, &host, &registry, job);
                }));
                if let Err(panic) = result {
                    let detail = panic_message(&panic);
                    error!("作业 {id}：处理时崩溃（{detail}），已标记失败。");
                    queue.update(&id, |row| {
                        row.state = JobState::Failed;
                        row.message = format!("内部错误（作业处理崩溃）：{detail}");
                        row.error = Some(row.message.clone());
                    });
                }
            }
        }
    }

    // 通知 stop()：worker 已退出，之后不会再调 SDK。
    if let Ok(mut finished) = inner.finished.lock() {
        *finished = true;
    }
    inner.finished_cv.notify_all();
}

struct QueueInnerHandle {
    inner: Arc<QueueInner>,
}

impl QueueInnerHandle {
    fn update(&self, id: &str, change: impl FnOnce(&mut JobStatus)) {
        update_row(&self.inner, id, change);
    }
}

fn run_job(
    queue: &QueueInnerHandle,
    host: &SdkHost,
    registry: &ScreenRegistry,
    job: Job,
) {
    let id = job.id.clone();
    queue.update(&id, |row| {
        row.state = JobState::Running;
        row.progress = 5;
        row.message = "开始处理。".to_string();
    });
    info!("作业 {id}：开始处理");

    let settings = registry.sdk();
    let devices = host.snapshot_devices();
    let processor = JobProcessor::new(host, registry, settings, devices);

    let result = {
        let queue_ref = &*queue;
        processor.process(&id, &job.request, &job.staged, |percent, message| {
            queue_ref.update(&id, |row| {
                row.progress = percent;
                row.message = message.to_string();
            });
        })
    };

    match result {
        Ok(done) => {
            let message = if done.sent {
                "已发送".to_string()
            } else {
                "已导出（未发送）".to_string()
            };
            queue.update(&id, |row| {
                row.state = JobState::Succeeded;
                row.progress = 100;
                row.message = message;
                row.package_path = Some(done.package_path.clone());
            });
            info!(
                "作业 {id}：成功（{}），产物 {}（导出目录 {}，设备 {}）",
                if done.sent { "已发送" } else { "仅导出" },
                done.package_path,
                done.export_directory,
                if done.device_id.is_empty() { "（未指定）" } else { done.device_id.as_str() }
            );
        }
        Err(err) => {
            let text = err.to_string();
            queue.update(&id, |row| {
                row.state = JobState::Failed;
                row.message = text.clone();
                row.error = Some(text.clone());
            });
            match err {
                JobError::Validation(_) => warn!("作业 {id}：{text}"),
                JobError::Sdk(inner) => error!("作业 {id}：{inner}"),
                JobError::Io(detail) => error!("作业 {id}：{detail}"),
            }
        }
    }

    if let Err(err) = std::fs::remove_file(&job.staged) {
        // 暂存文件清理失败不影响作业结论。
        warn!("作业 {id}：清理暂存文件失败：{err}");
    }
}

/// 生成一个不重复的短 id（时间纳秒 + 进程内计数）。
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:016x}{seq:04x}")
}

/// 把 panic 载荷转成一句话，用于作业失败原因。
fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = panic.downcast_ref::<&str>() {
        return (*text).to_string();
    }
    if let Some(text) = panic.downcast_ref::<String>() {
        return text.clone();
    }
    "未知 panic".to_string()
}