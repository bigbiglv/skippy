use crate::llm::llama_supervisor::LlamaSupervisor;
use crate::worker::supervisor::{PythonEnvInfo, WorkerSupervisor};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceStage {
    ShortcutOnly,
    IdleListening,
    Recording,
    Thinking,
    Speaking,
    Sleeping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub stage: VoiceStage,
    pub stage_label: String,
    pub ollama_connected: bool,
    pub ollama_models: Vec<String>,
    pub python_worker_running: bool,
    pub python_env: PythonEnvInfo,
    pub llama_runtime_ready: bool,
    pub llama_server_running: bool,
    pub llama_loaded_model: Option<String>,
    pub local_storage_dir: String,
    pub gpu_vram_active_mb: u32,
}

impl RuntimeStatus {
    pub fn with_env(
        python_env: PythonEnvInfo,
        llama_runtime_ready: bool,
        local_storage_dir: String,
    ) -> Self {
        Self {
            stage: VoiceStage::ShortcutOnly,
            stage_label: "待机低占用（快捷键唤醒）".to_string(),
            ollama_connected: false,
            ollama_models: Vec::new(),
            python_worker_running: false,
            python_env,
            llama_runtime_ready,
            llama_server_running: false,
            llama_loaded_model: None,
            local_storage_dir,
            gpu_vram_active_mb: 0,
        }
    }
}

pub struct AppState {
    pub db: Mutex<Connection>,
    pub data_dir: PathBuf,
    pub status: Mutex<RuntimeStatus>,
    pub worker: Mutex<WorkerSupervisor>,
    pub llama: Mutex<LlamaSupervisor>,
    pub chat_cancel: Arc<AtomicBool>,
    pub download_cancel: Arc<AtomicBool>,
    pub should_exit: AtomicBool,
}
