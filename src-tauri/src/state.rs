use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

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
    pub python_worker_running: bool,
    pub gpu_vram_active_mb: u32,
}

impl Default for RuntimeStatus {
    fn default() -> Self {
        Self {
            stage: VoiceStage::ShortcutOnly,
            stage_label: "待机低占用（快捷键唤醒）".to_string(),
            ollama_connected: false,
            python_worker_running: false,
            gpu_vram_active_mb: 0,
        }
    }
}

pub struct AppState {
    pub db: Mutex<Connection>,
    pub data_dir: PathBuf,
    pub status: Mutex<RuntimeStatus>,
    pub should_exit: AtomicBool,
}
