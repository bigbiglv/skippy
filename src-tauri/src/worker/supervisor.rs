use crate::process_guard::{attach_child_to_job, free_stale_port};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonEnvInfo {
    pub python_available: bool,
    pub python_version: Option<String>,
    pub python_path: Option<String>,
    pub venv_ready: bool,
    pub worker_script_found: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerGpuInfo {
    pub available: bool,
    pub name: Option<String>,
    pub vram_used_mb: u32,
    pub vram_total_mb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHealth {
    pub status: String,
    pub pid: u32,
    pub uptime_secs: f64,
    pub loaded_models: Vec<String>,
    pub gpu: WorkerGpuInfo,
}

pub struct WorkerSupervisor {
    child: Option<Child>,
    port: u16,
    last_active: Option<Instant>,
    worker_dir: Option<PathBuf>,
}

impl WorkerSupervisor {
    pub fn new(port: u16) -> Self {
        Self {
            child: None,
            port,
            last_active: None,
            worker_dir: find_worker_dir(),
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn is_running(&mut self) -> bool {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(None) => return true,
                _ => {
                    self.child = None;
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

    pub fn detect_env(&self) -> PythonEnvInfo {
        let worker_dir = self.worker_dir.as_deref();
        let venv_python = worker_dir.map(|d| d.join(".venv").join("Scripts").join("python.exe"));
        let venv_ready = venv_python.as_ref().is_some_and(|p| p.exists());
        let script_found = worker_dir.is_some_and(|d| d.join("server.py").exists());

        let candidate = if venv_ready {
            venv_python.unwrap()
        } else {
            PathBuf::from("python")
        };

        let mut cmd = Command::new(&candidate);
        cmd.arg("--version");
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);

        match cmd.output() {
            Ok(out) if out.status.success() => {
                let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let ver = if ver.is_empty() {
                    String::from_utf8_lossy(&out.stderr).trim().to_string()
                } else {
                    ver
                };
                PythonEnvInfo {
                    python_available: !ver.is_empty(),
                    python_version: Some(ver),
                    python_path: Some(candidate.to_string_lossy().to_string()),
                    venv_ready,
                    worker_script_found: script_found,
                }
            }
            _ => PythonEnvInfo {
                python_available: false,
                python_version: None,
                python_path: None,
                venv_ready,
                worker_script_found: script_found,
            },
        }
    }

    pub fn spawn_if_needed(&mut self) -> Result<String, String> {
        if self.is_running() {
            self.touch();
            return Ok(self.base_url());
        }

        let worker_dir = self
            .worker_dir
            .clone()
            .ok_or_else(|| "未找到 worker/server.py 目录".to_string())?;
        if !worker_dir.join("server.py").exists() {
            return Err("未找到 worker/server.py".to_string());
        }

        let venv_python = worker_dir.join(".venv").join("Scripts").join("python.exe");
        let python_bin = if venv_python.exists() {
            venv_python
        } else {
            PathBuf::from("python")
        };

        free_stale_port(self.port, "python");

        let mut cmd = Command::new(&python_bin);
        cmd.current_dir(&worker_dir)
            .args([
                "-m",
                "uvicorn",
                "server:app",
                "--host",
                "127.0.0.1",
                "--port",
                &self.port.to_string(),
                "--log-level",
                "warning",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let child = cmd
            .spawn()
            .map_err(|e| format!("启动 Python Worker 失败: {e}"))?;
        attach_child_to_job(&child);

        self.child = Some(child);
        self.last_active = Some(Instant::now());
        Ok(self.base_url())
    }

    pub fn stop(&mut self) -> bool {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.last_active = None;
            return true;
        }
        self.last_active = None;
        false
    }
}

impl Drop for WorkerSupervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

pub async fn wait_for_health(base_url: &str) -> Result<WorkerHealth, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| e.to_string())?;
    let health_url = format!("{base_url}/health");

    for _ in 0..25 {
        if let Ok(resp) = client.get(&health_url).send().await {
            if resp.status().is_success() {
                if let Ok(parsed) = resp.json::<WorkerHealth>().await {
                    return Ok(parsed);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    Err("Python Worker 启动超时，请确认已运行 worker 依赖安装（fastapi / uvicorn）".to_string())
}

fn find_worker_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("worker"));
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join("worker"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("worker"));
            for ancestor in exe_dir.ancestors().take(4) {
                candidates.push(ancestor.join("worker"));
            }
        }
    }
    candidates
        .into_iter()
        .find(|p| is_valid_worker_dir(p))
}

fn is_valid_worker_dir(path: &Path) -> bool {
    path.is_dir() && path.join("server.py").is_file()
}
