//! 按「有效天数」清理过期日志与导出产物。
//!
//! 导出目录里 `.zhl` 是节目清单（内含资源 URL），`.zhd`/`.zhp`/resources 是它的内容。
//! 所以先删过期清单，再把「保留中的清单」全文拼起来，只删没被任何保留清单引用的资源。

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use log::{info, warn};

pub fn clean(
    export_root: &Path,
    upload_root: &Path,
    log_directory: &Path,
    retention_days: i32,
) -> usize {
    if retention_days <= 0 {
        info!("保留天数为 {retention_days}（<=0），跳过启动清理。");
        return 0;
    }
    let now = SystemTime::now();
    let cutoff = now
        .checked_sub(Duration::from_secs(retention_days as u64 * 86_400))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let mut deleted = 0usize;
    let mut bytes = 0u64;

    match clean_plain_directory(log_directory, cutoff) {
        Ok((files, size)) => {
            deleted += files;
            bytes += size;
        }
        Err(err) => warn!("清理日志目录 {} 失败：{err}", log_directory.display()),
    }

    match clean_export_directory(export_root, cutoff) {
        Ok((files, size)) => {
            deleted += files;
            bytes += size;
        }
        Err(err) => warn!("清理导出目录 {} 失败：{err}", export_root.display()),
    }

    // 上传暂存：作业正常结束会自己删，但强杀/断电会留下孤儿文件。
    match clean_upload_directory(upload_root, cutoff) {
        Ok((files, size)) => {
            deleted += files;
            bytes += size;
        }
        Err(err) => warn!("清理上传目录 {} 失败：{err}", upload_root.display()),
    }

    info!(
        "启动清理：保留 {retention_days} 天内文件，删除 {deleted} 个过期文件，释放 {}。",
        format_size(bytes)
    );
    deleted
}

fn clean_plain_directory(directory: &Path, cutoff: SystemTime) -> std::io::Result<(usize, u64)> {
    if !is_safe_directory(directory) {
        return Ok((0, 0));
    }
    let mut count = 0;
    let mut bytes = 0;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            // 日志按天一个目录（logs/yyyy-MM-dd/）：先清掉里面的过期文件；清空后目录一起删
            // （目录自身的 mtime 是「最后一次写入」，不能拿它判断过期）。
            let (inner_count, inner_bytes) = clean_plain_directory(&path, cutoff)?;
            count += inner_count;
            bytes += inner_bytes;
            if is_empty_directory(&path) {
                let _ = std::fs::remove_dir(&path);
            }
            continue;
        }
        if !file_type.is_file() || !is_expired(&path, cutoff) {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if try_delete(&path) {
            count += 1;
            bytes += size;
        }
    }
    Ok((count, bytes))
}

fn is_empty_directory(directory: &Path) -> bool {
    std::fs::read_dir(directory)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}

/// 上传暂存的归属标记：只有程序自己建的目录才有，用来防 UploadRoot 配错删掉别人的文件。
const UPLOAD_MARKER: &str = ".ledsdk-upload";

/// 上传暂存目录清理：与日志目录规则相同，但要求目录「空」或「带标记」才算我们的。
fn clean_upload_directory(directory: &Path, cutoff: SystemTime) -> std::io::Result<(usize, u64)> {
    if !is_safe_directory(directory) {
        return Ok((0, 0));
    }
    let marker = directory.join(UPLOAD_MARKER);
    if !marker.is_file() && !is_empty_directory(directory) {
        warn!(
            "跳过上传目录清理：{} 非空且没有 {UPLOAD_MARKER} 标记，可能不是上传暂存目录。",
            directory.display()
        );
        return Ok((0, 0));
    }
    // 刷新标记的 mtime，否则它自己会在若干天后被当成过期文件删掉，下次就认不出目录了。
    let _ = std::fs::write(&marker, b"");
    clean_plain_directory(directory, cutoff)
}

fn clean_export_directory(directory: &Path, cutoff: SystemTime) -> std::io::Result<(usize, u64)> {
    if !is_safe_directory(directory) {
        return Ok((0, 0));
    }
    // 只有「像导出目录」的目录才清理：至少有一个 .zhl，或存在我们的标记文件。
    // 否则一旦 ExportRoot 配错（指向下载目录、程序目录），启动就会删掉里面的旧文件。
    if !looks_like_export_directory(directory) {
        warn!(
            "跳过导出目录清理：{} 里没有 .zhl 也没有 .ledsdk-export 标记，可能不是导出目录。",
            directory.display()
        );
        return Ok((0, 0));
    }

    let mut count = 0;
    let mut bytes = 0;
    let mut retained_text = String::new();
    let mut zhl_files = Vec::new();
    collect_zhl_files(directory, &mut zhl_files);
    for path in zhl_files {
        if is_expired(&path, cutoff) {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if try_delete(&path) {
                count += 1;
                bytes += size;
            }
        } else if let Ok(text) = std::fs::read(&path) {
            retained_text.push_str(&String::from_utf8_lossy(&text));
            // 多个清单直接拼接会把上一份末尾和下一份开头粘成一个假文件名。
            retained_text.push('\n');
        }
    }

    // 递归删除未被保留清单引用的旧文件——只动已知产物类型（.zhd/.zhp/resources 下的文件）。
    let mut stack = vec![directory.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            let in_resources = current.ends_with("resources");
            let is_artifact = match path.extension().and_then(|e| e.to_str()) {
                Some("zhd") | Some("zhp") => true,
                Some("zhl") => false,
                _ => in_resources,
            };
            if !is_artifact {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            // .zhd/.zhp/resources 里都只看「是否被保留清单引用」：备份还原或增量导出时，
            // 被保留 .zhl 引用的 .zhd/.zhp mtime 可能很旧，单纯按时间删会破坏节目包。
            let keep = if is_artifact {
                mentions_token(&retained_text, &name)
            } else {
                !is_expired(&path, cutoff)
            };
            if keep {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if try_delete(&path) {
                count += 1;
                bytes += size;
            }
        }
    }

    Ok((count, bytes))
}

/// 导出目录的标志：存在任一 `*.zhl` 或 `.ledsdk-export` 标记文件。
fn looks_like_export_directory(directory: &Path) -> bool {
    if directory.join(".ledsdk-export").is_file() {
        return true;
    }
    let mut zhl_files = Vec::new();
    collect_zhl_files(directory, &mut zhl_files);
    !zhl_files.is_empty()
}

/// 递归收集 `.zhl`，跳过符号链接目录，避免清理时跟随链接离开导出目录。
fn collect_zhl_files(directory: &Path, out: &mut Vec<PathBuf>) {
    let mut stack = vec![directory.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("zhl"))
            {
                out.push(path);
            }
        }
    }
}

/// 只把独立出现的文件名视为引用，避免 `a.zhd` 误匹配 `aa.zhd` 这类子串。
fn mentions_token(text: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    text.match_indices(name).any(|(index, _)| {
        let before = text[..index].chars().next_back();
        let after = text[index + name.len()..].chars().next();
        is_token_boundary(before) && is_token_boundary(after)
    })
}

fn is_token_boundary(value: Option<char>) -> bool {
    match value {
        None => true,
        Some(value) => {
            value.is_whitespace()
                || matches!(
                    value,
                    '/' | '\\'
                        | '"'
                        | '\''
                        | '='
                        | ','
                        | ';'
                        | '?'
                        | '&'
                        | '<'
                        | '>'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | ':'
                        | '|'
                        | '#'
                )
        }
    }
}

fn is_expired(path: &Path, cutoff: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|modified| modified < cutoff)
        .unwrap_or(false)
}

fn try_delete(path: &Path) -> bool {
    match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(err) => {
            warn!("删除 {} 失败：{err}", path.display());
            false
        }
    }
}

/// 只清理真实存在的普通目录；盘符根 / 共享根一律拒绝。
fn is_safe_directory(directory: &Path) -> bool {
    if directory.as_os_str().is_empty() || !directory.is_absolute() || !directory.is_dir() {
        return false;
    }
    let full: PathBuf = directory.to_path_buf();
    full.parent().is_some()
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_matching_avoids_substrings() {
        assert!(mentions_token("keep.png a.zhd", "a.zhd"));
        assert!(!mentions_token("aa.zhd", "a.zhd"));
    }

    #[test]
    fn referenced_artifact_survives_cleanup() {
        let root = std::env::temp_dir().join("ledsdk-retention-test");
        let export = root.join("export");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&export).unwrap();
        std::fs::write(export.join("keep.zhl"), "a.zhd").unwrap();
        std::fs::write(export.join("a.zhd"), "kept").unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(40 * 86_400);
        let file = std::fs::File::options()
            .write(true)
            .open(export.join("a.zhd"))
            .unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();
        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 86_400);
        let _ = clean_export_directory(&export, cutoff).unwrap();
        assert!(export.join("a.zhd").exists());
    }
}
