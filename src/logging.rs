//! 极简按天滚动文件日志，每天一个目录：
//! `logs/2026-11-18/app.log`，行格式 `2026-11-18 10:04:12.345 [INF] 类别 : 消息`
//!
//! 另外提供全局异常兜底：Rust 侧 panic 与前端 JS 未捕获异常写进同一天目录的
//! `logs/2026-11-18/errors.log`，与常规日志分文件，方便直接定位崩溃。

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};

/// 异常文件所在目录（`init` 时记录，供 panic hook 使用）。
static ERRORS_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();

pub struct FileLogger {
    directory: PathBuf,
    minimum: LevelFilter,
    inner: Mutex<Option<(String, File)>>,
}

impl FileLogger {
    pub fn new(directory: impl AsRef<Path>, minimum: LevelFilter) -> Self {
        let directory = directory.as_ref().to_path_buf();
        let _ = fs::create_dir_all(&directory);
        Self {
            directory,
            minimum,
            inner: Mutex::new(None),
        }
    }

    fn write_line(&self, level: Level, target: &str, message: &str) {
        let now =
            time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
        let day = format!(
            "{:04}-{:02}-{:02}",
            now.year(),
            now.month() as u8,
            now.day()
        );
        let stamp = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
            now.year(),
            now.month() as u8,
            now.day(),
            now.hour(),
            now.minute(),
            now.second(),
            now.millisecond()
        );

        let line = format!("{stamp} [{}] {target} : {message}\n", tag(level));

        // 锁中毒（持锁期间 panic）不能把之后的日志全丢掉：恢复内部值继续写。
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());

        let rotate = match guard.as_ref() {
            Some((current, _)) => current != &day,
            None => true,
        };
        if rotate {
            let day_directory = self.directory.join(&day);
            // 升级场景：旧版本把 logs/<日期> 写成了文件，这里建目录会失败。
            // 与其静默丢掉全部日志，退回单文件 logs/app-<日期>.log。
            let path = if fs::create_dir_all(&day_directory).is_ok() {
                day_directory.join("app.log")
            } else {
                self.directory.join(format!("app-{day}.log"))
            };
            match OpenOptions::new().create(true).append(true).open(&path) {
                Ok(file) => *guard = Some((day, file)),
                Err(_) => return,
            }
        }

        if let Some((_, file)) = guard.as_mut() {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }
}

fn tag(level: Level) -> &'static str {
    match level {
        Level::Error => "ERR",
        Level::Warn => "WRN",
        Level::Info => "INF",
        Level::Debug => "DBG",
        Level::Trace => "TRC",
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.minimum
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        self.write_line(record.level(), record.target(), &record.args().to_string());
    }

    fn flush(&self) {}
}

pub fn init(directory: impl AsRef<Path>, minimum: LevelFilter) -> &'static FileLogger {
    let directory = directory.as_ref().to_path_buf();
    let logger: &'static FileLogger = Box::leak(Box::new(FileLogger::new(&directory, minimum)));
    // 重复初始化直接忽略：全局 logger 只装一次。
    let _ = log::set_logger(logger);
    log::set_max_level(minimum);
    let _ = ERRORS_DIRECTORY.set(directory);
    install_panic_hook();
    logger
}

/// 把一条异常追加到 `logs/<日期>/errors.log`。
///
/// 刻意不走 `log::error!`：hook 可能在 logger 的锁里触发 panic，再抢同一把锁会死锁。
/// 这里每次单独 open + append，O_APPEND 保证并发写不会互相覆盖。
pub fn record_exception(directory: &Path, kind: &str, text: &str) {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let day = format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        now.month() as u8,
        now.day()
    );
    let day_directory = directory.join(&day);
    // 与 write_line 同样的升级兜底：目录建不出来时退回单文件 errors-<日期>.log。
    let path = if fs::create_dir_all(&day_directory).is_ok() {
        day_directory.join("errors.log")
    } else {
        directory.join(format!("errors-{day}.log"))
    };
    let stamp = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.millisecond()
    );
    let body = format!("{stamp} [{kind}] {text}\n");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(body.as_bytes());
        let _ = file.flush();
    }
}

/// 记录到 `init` 指定的日志目录；还没初始化时退回 stderr。
pub fn record_exception_global(kind: &str, text: &str) {
    match ERRORS_DIRECTORY.get() {
        Some(directory) => record_exception(directory, kind, text),
        None => eprintln!("[{kind}] {text}"),
    }
}

/// 装全局 panic hook：任何线程的 panic 都先写 errors 文件，再交给上一个 hook。
fn install_panic_hook() {
    static INSTALLED: Mutex<bool> = Mutex::new(false);
    if let Ok(mut done) = INSTALLED.lock() {
        if *done {
            return;
        }
        *done = true;
    }

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = if let Some(text) = info.payload().downcast_ref::<&str>() {
            (*text).to_string()
        } else if let Some(text) = info.payload().downcast_ref::<String>() {
            text.clone()
        } else {
            "未知 panic 载荷".to_string()
        };
        let location = info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "未知位置".to_string());
        let thread = std::thread::current()
            .name()
            .unwrap_or("<未命名线程>")
            .to_string();
        // force_capture 不受 RUST_BACKTRACE 影响，发行版里也能拿到调用栈。
        let backtrace = std::backtrace::Backtrace::force_capture();
        let text = format!("未捕获异常：{payload}\n线程：{thread}\n位置：{location}\n{backtrace}");
        record_exception_global("PANIC", &text);
        previous(info);
    }));
}
