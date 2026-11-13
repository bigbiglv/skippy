use crate::llm::downloader::{download_file_with_progress, emit_download_progress, DownloadProgressEvent};
use crate::process_guard::{attach_child_to_job, free_stale_port};
use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
use tauri::AppHandle;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

pub struct LlamaSupervisor {
    child: Option<Child>,
    port: u16,
    loaded_model_id: Option<String>,
    loaded_model_path: Option<PathBuf>,
    last_active: Option<Instant>,
}

impl LlamaSupervisor {
    pub fn new(port: u16) -> Self {
        Self {
            child: None,
            port,
            loaded_model_id: None,
            loaded_model_path: None,
            last_active: None,
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn loaded_model_id(&self) -> Option<String> {
        self.loaded_model_id.clone()
    }

    pub fn is_running(&mut self) -> bool {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(None) => return true,
                _ => {
                    self.child = None;
                    self.loaded_model_id = None;
                    self.loaded_model_path = None;
                    self.last_active = None;
                }
            }
        }
        false
    }

    pub fn touch(&mut self) {
        if self.child.is_some() {
            self.last_active = Some(Instant::now());
        }
    }

    pub fn idle_elapsed_secs(&self) -> Option<u64> {
        self.last_active.map(|t| t.elapsed().as_secs())
    }

    pub fn runtime_dir(storage_dir: &Path) -> PathBuf {
        storage_dir.join("runtime").join("llama-server")
    }

    pub fn find_binary(storage_dir: &Path) -> Option<PathBuf> {
        let dir = Self::runtime_dir(storage_dir);
        let direct = dir.join("llama-server.exe");
        if direct.exists() {
            return Some(direct);
        }

        // 某些解压包可能多一层子目录
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("llama-server.exe");
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        None
    }

    pub fn spawn_for_model(
        &mut self,
        storage_dir: &Path,
        model_id: &str,
        gguf_path: &Path,
    ) -> Result<(String, bool), String> {
        if !gguf_path.exists() {
            return Err(format!("找不到模型文件：{}", gguf_path.display()));
        }

        if self.is_running() {
            if self.loaded_model_path.as_deref() == Some(gguf_path) {
                self.touch();
                return Ok((self.base_url(), false));
            }
            self.stop();
        }

        let bin = Self::find_binary(storage_dir).ok_or_else(|| {
            "尚未安装内置 llama-server 运行时，请在「模型管理」点击「一键安装引擎」".to_string()
        })?;

        free_stale_port(self.port, "llama-server");

        let mut cmd = Command::new(&bin);
        if let Some(bin_dir) = bin.parent() {
            cmd.current_dir(bin_dir);
        }
        cmd.arg("-m")
            .arg(gguf_path)
            .args([
                "--host",
                "127.0.0.1",
                "--port",
                &self.port.to_string(),
                "-ngl",
                "99",
                "-c",
                "8192",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let child = cmd
            .spawn()
            .map_err(|e| format!("启动 llama-server 失败 ({}): {e}", bin.display()))?;
        attach_child_to_job(&child);

        self.child = Some(child);
        self.loaded_model_id = Some(model_id.to_string());
        self.loaded_model_path = Some(gguf_path.to_path_buf());
        self.last_active = Some(Instant::now());

        Ok((self.base_url(), true))
    }

    pub fn take_exit_error(&mut self) -> Option<String> {
        let child = self.child.as_mut()?;
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut err_buf = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut err_buf);
                }
                self.child = None;
                self.loaded_model_id = None;
                self.loaded_model_path = None;
                self.last_active = None;
                let tail: String = err_buf
                    .lines()
                    .rev()
                    .take(8)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                Some(format!("llama-server 提前退出 ({status})：\n{tail}"))
            }
            _ => None,
        }
    }

    pub fn stop(&mut self) -> bool {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.loaded_model_id = None;
            self.loaded_model_path = None;
            self.last_active = None;
            return true;
        }
        false
    }
}

impl Drop for LlamaSupervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

pub async fn wait_for_llama_ready(
    app: &AppHandle,
    base_url: &str,
    check_exit: impl Fn() -> Option<String>,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| format!("创建健康检查客户端失败: {e}"))?;

    let health_url = format!("{base_url}/health");
    let deadline = Instant::now() + Duration::from_secs(90);

    while Instant::now() < deadline {
        if let Some(err) = check_exit() {
            return Err(err);
        }

        if let Ok(resp) = client.get(&health_url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        let _ = app;
        tokio::time::sleep(Duration::from_millis(350)).await;
    }

    Err("等待 llama-server 加载模型超时（90秒）".to_string())
}

fn rewrite_github_url(url: &str, mirror_prefix: &str) -> String {
    let trimmed = mirror_prefix.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "direct" {
        return url.to_string();
    }
    format!("{trimmed}/{url}")
}

fn extract_zip_native(zip_path: &Path, dest_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dest_dir).map_err(|e| format!("创建解压目录失败: {e}"))?;
    let mut cmd = Command::new("tar.exe");
    cmd.arg("-xf").arg(zip_path).arg("-C").arg(dest_dir);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let out = cmd
        .output()
        .map_err(|e| format!("调用 tar.exe 解压失败: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("解压 {} 失败: {err}", zip_path.display()));
    }
    Ok(())
}

pub async fn install_llama_runtime(
    app: &AppHandle,
    storage_dir: &Path,
    mirror_prefix: &str,
    cancel_flag: &AtomicBool,
) -> Result<String, String> {
    let target_id = "llama-runtime";
    let runtime_dir = LlamaSupervisor::runtime_dir(storage_dir);
    fs::create_dir_all(&runtime_dir).map_err(|e| format!("创建运行时目录失败: {e}"))?;

    emit_download_progress(
        app,
        DownloadProgressEvent {
            target_id: target_id.to_string(),
            stage: "resolving".to_string(),
            downloaded_bytes: 0,
            total_bytes: 0,
            percent: 0,
            message: "正在查询最新版 llama.cpp CUDA 12.4 运行时...".to_string(),
            done: false,
            error: None,
        },
    );

    let client = reqwest::Client::builder()
        .user_agent("Skippy-Desktop/0.1")
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("初始化 HTTP 客户端失败: {e}"))?;

    let releases: Vec<GithubRelease> = client
        .get("https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10")
        .send()
        .await
        .map_err(|e| format!("获取 llama.cpp 版本列表失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析 llama.cpp 版本 JSON 失败: {e}"))?;

    let mut matched: Option<(String, String, Option<String>)> = None;
    for rel in releases {
        let main_zip = rel.assets.iter().find(|a| {
            a.name.starts_with("llama-") && a.name.ends_with("bin-win-cuda-12.4-x64.zip")
        });
        if let Some(bin_asset) = main_zip {
            let cudart_asset = rel
                .assets
                .iter()
                .find(|a| a.name == "cudart-llama-bin-win-cuda-12.4-x64.zip")
                .map(|a| a.browser_download_url.clone());
            matched = Some((
                rel.tag_name,
                bin_asset.browser_download_url.clone(),
                cudart_asset,
            ));
            break;
        }
    }

    let (tag, bin_url, cudart_url) =
        matched.ok_or_else(|| "未在最近发布中找到 Windows CUDA 12.4 运行时包".to_string())?;

    if let Some(url) = cudart_url {
        let zip_path = runtime_dir.join("_cudart.zip");
        let real_url = rewrite_github_url(&url, mirror_prefix);
        download_file_with_progress(
            app,
            target_id,
            &format!("下载 CUDA 运行库 ({tag}) [1/2]"),
            &real_url,
            &zip_path,
            cancel_flag,
        )
        .await?;

        emit_download_progress(
            app,
            DownloadProgressEvent {
                target_id: target_id.to_string(),
                stage: "extracting".to_string(),
                downloaded_bytes: 0,
                total_bytes: 0,
                percent: 99,
                message: "正在解压 CUDA 运行库...".to_string(),
                done: false,
                error: None,
            },
        );
        extract_zip_native(&zip_path, &runtime_dir)?;
        let _ = fs::remove_file(&zip_path);
    }

    let bin_zip_path = runtime_dir.join("_llama_bin.zip");
    let real_bin_url = rewrite_github_url(&bin_url, mirror_prefix);
    download_file_with_progress(
        app,
        target_id,
        &format!("下载 llama-server ({tag}) [2/2]"),
        &real_bin_url,
        &bin_zip_path,
        cancel_flag,
    )
    .await?;

    emit_download_progress(
        app,
        DownloadProgressEvent {
            target_id: target_id.to_string(),
            stage: "extracting".to_string(),
            downloaded_bytes: 0,
            total_bytes: 0,
            percent: 99,
            message: "正在解压 llama-server 主程序...".to_string(),
            done: false,
            error: None,
        },
    );
    extract_zip_native(&bin_zip_path, &runtime_dir)?;
    let _ = fs::remove_file(&bin_zip_path);

    let binary = LlamaSupervisor::find_binary(storage_dir)
        .ok_or_else(|| "解压完成后未找到 llama-server.exe".to_string())?;

    emit_download_progress(
        app,
        DownloadProgressEvent {
            target_id: target_id.to_string(),
            stage: "done".to_string(),
            downloaded_bytes: 1,
            total_bytes: 1,
            percent: 100,
            message: format!("llama-server ({tag}) 安装完成"),
            done: true,
            error: None,
        },
    );

    Ok(binary.to_string_lossy().to_string())
}
