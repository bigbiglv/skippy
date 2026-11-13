use serde::Serialize;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgressEvent {
    pub target_id: String,
    pub stage: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub percent: u8,
    pub message: String,
    pub done: bool,
    pub error: Option<String>,
}

pub fn emit_download_progress(app: &AppHandle, payload: DownloadProgressEvent) {
    let _ = app.emit("download-progress", payload);
}

pub async fn download_file_with_progress(
    app: &AppHandle,
    target_id: &str,
    stage_label: &str,
    url: &str,
    dest_path: &Path,
    cancel_flag: &AtomicBool,
) -> Result<(), String> {
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }

    let part_path = dest_path.with_extension(format!(
        "{}.part",
        dest_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin")
    ));

    let client = reqwest::Client::builder()
        .user_agent("Skippy-Desktop/0.1")
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("初始化 HTTP 客户端失败: {e}"))?;

    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("请求下载失败 ({url}): {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("下载返回 HTTP 状态 {}", resp.status()));
    }

    let total_bytes = resp.content_length().unwrap_or(0);
    let mut file = File::create(&part_path).map_err(|e| format!("创建临时文件失败: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut last_emit = Instant::now();

    emit_download_progress(
        app,
        DownloadProgressEvent {
            target_id: target_id.to_string(),
            stage: "downloading".to_string(),
            downloaded_bytes: 0,
            total_bytes,
            percent: 0,
            message: stage_label.to_string(),
            done: false,
            error: None,
        },
    );

    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("读取下载数据流失败: {e}"))?
    {
        if cancel_flag.load(Ordering::Relaxed) {
            drop(file);
            let _ = fs::remove_file(&part_path);
            return Err("下载已取消".to_string());
        }

        file.write_all(&chunk)
            .map_err(|e| format!("写入文件失败: {e}"))?;
        downloaded += chunk.len() as u64;

        if last_emit.elapsed().as_millis() >= 200 {
            let percent = if total_bytes > 0 {
                ((downloaded as f64 / total_bytes as f64) * 100.0).min(99.0) as u8
            } else {
                0
            };
            emit_download_progress(
                app,
                DownloadProgressEvent {
                    target_id: target_id.to_string(),
                    stage: "downloading".to_string(),
                    downloaded_bytes: downloaded,
                    total_bytes,
                    percent,
                    message: stage_label.to_string(),
                    done: false,
                    error: None,
                },
            );
            last_emit = Instant::now();
        }
    }

    file.flush().map_err(|e| format!("刷新磁盘缓存失败: {e}"))?;
    drop(file);

    if dest_path.exists() {
        let _ = fs::remove_file(dest_path);
    }
    fs::rename(&part_path, dest_path).map_err(|e| format!("重命名下载文件失败: {e}"))?;

    emit_download_progress(
        app,
        DownloadProgressEvent {
            target_id: target_id.to_string(),
            stage: "downloading".to_string(),
            downloaded_bytes: downloaded,
            total_bytes: total_bytes.max(downloaded),
            percent: 100,
            message: stage_label.to_string(),
            done: false,
            error: None,
        },
    );

    Ok(())
}
