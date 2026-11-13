use crate::llm::client::probe_ollama;
use crate::llm::downloader::{emit_download_progress, DownloadProgressEvent};
use crate::llm::llama_supervisor::{install_llama_runtime as install_llama_runtime_impl, LlamaSupervisor};
use crate::llm::runner::{ensure_llama_ready_for_model, execute_chat_turn, SendMessageResult};
use crate::models::registry::{
    default_gguf_path, find_gguf_spec, gguf_download_url, list_catalog_with_state,
    resolve_model_gguf_path, set_active_model, set_model_local_path, ModelCatalogItem,
};
use crate::state::{AppState, RuntimeStatus};
use crate::storage::chat_repo::{
    create_session, delete_session, get_all_settings, list_messages, list_sessions,
    resolve_local_storage_dir, set_setting, ChatMessage, ChatSession,
};
use crate::voice::assets::{
    download_voice_model as download_voice_model_impl, ensure_silero_vad, find_sherpa_c_api_dll,
    install_sherpa_runtime, voice_models_root,
};
use crate::worker::supervisor::{wait_for_health, WorkerHealth};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Serialize)]
pub struct AppSnapshot {
    pub status: RuntimeStatus,
    pub sessions: Vec<ChatSession>,
    pub models: Vec<ModelCatalogItem>,
    pub settings: HashMap<String, String>,
    pub data_dir: String,
}

fn sync_status_snapshot(
    state: &State<'_, AppState>,
    storage_dir: &Path,
    ollama_connected: bool,
    ollama_models: Vec<String>,
) -> Result<RuntimeStatus, String> {
    let (worker_running, python_env) = {
        let mut worker = state.worker.lock().map_err(|e| e.to_string())?;
        (worker.is_running(), worker.detect_env())
    };
    let (llama_running, llama_model) = {
        let mut llama = state.llama.lock().map_err(|e| e.to_string())?;
        let running = llama.is_running();
        let loaded = if running { llama.loaded_model_id() } else { None };
        (running, loaded)
    };
    let llama_ready = LlamaSupervisor::find_binary(storage_dir).is_some();
    let voice_ready = find_sherpa_c_api_dll(storage_dir).is_some();

    let mut status = state.status.lock().map_err(|e| e.to_string())?;
    status.python_worker_running = worker_running;
    status.python_env = python_env;
    status.llama_runtime_ready = llama_ready;
    status.llama_server_running = llama_running;
    status.llama_loaded_model = llama_model;
    status.voice_runtime_ready = voice_ready;
    status.local_storage_dir = storage_dir.to_string_lossy().to_string();
    status.ollama_connected = ollama_connected;
    status.ollama_models = ollama_models;
    Ok(status.clone())
}

#[tauri::command]
pub async fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let (sessions, models, settings, storage_dir) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
        let sessions = list_sessions(&db).map_err(|e| e.to_string())?;
        let models = list_catalog_with_state(&db, &storage_dir).map_err(|e| e.to_string())?;
        let settings = get_all_settings(&db).map_err(|e| e.to_string())?;
        (sessions, models, settings, storage_dir)
    };

    let ollama_url = settings
        .get("ollama_base_url")
        .cloned()
        .unwrap_or_else(|| "http://127.0.0.1:11434".to_string());
    let (ollama_connected, ollama_models) = probe_ollama(&ollama_url).await;
    let status = sync_status_snapshot(&state, &storage_dir, ollama_connected, ollama_models)?;

    Ok(AppSnapshot {
        status,
        sessions,
        models,
        settings,
        data_dir: state.data_dir.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn create_chat_session(
    title: String,
    state: State<'_, AppState>,
) -> Result<ChatSession, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    create_session(&db, &title).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_chat_session(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    delete_session(&db, &session_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_session_messages(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ChatMessage>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    list_messages(&db, &session_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn stop_generation(state: State<'_, AppState>) -> Result<(), String> {
    state.chat_cancel.store(true, Ordering::Relaxed);
    state.tts.stop_speaking();
    Ok(())
}

#[tauri::command]
pub async fn send_text_message(
    app: AppHandle,
    session_id: String,
    content: String,
) -> Result<SendMessageResult, String> {
    execute_chat_turn(&app, &session_id, &content, "pc_text", false).await
}

#[tauri::command]
pub fn start_voice_capture(session_id: Option<String>, state: State<'_, AppState>) -> Result<(), String> {
    state.voice.start_manual(session_id);
    Ok(())
}

#[tauri::command]
pub fn stop_voice_capture(state: State<'_, AppState>) -> Result<(), String> {
    state.voice.stop_manual();
    Ok(())
}

#[tauri::command]
pub fn stop_speaking(state: State<'_, AppState>) -> Result<(), String> {
    state.voice.barge_in_stop();
    Ok(())
}

#[tauri::command]
pub fn select_active_model(
    category: String,
    model_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ModelCatalogItem>, String> {
    let items = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
        set_active_model(&db, &category, &model_id).map_err(|e| e.to_string())?;
        list_catalog_with_state(&db, &storage_dir).map_err(|e| e.to_string())?
    };

    if category == "kws" || category == "asr" || category == "tts" {
        state.voice.reload_config();
    }

    Ok(items)
}

#[tauri::command]
pub fn update_app_setting(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<HashMap<String, String>, String> {
    let settings = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        set_setting(&db, &key, &value).map_err(|e| e.to_string())?;

        if key == "local_storage_dir" {
            let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
            let llama_ready = LlamaSupervisor::find_binary(&storage_dir).is_some();
            let voice_ready = find_sherpa_c_api_dll(&storage_dir).is_some();
            if let Ok(mut st) = state.status.lock() {
                st.local_storage_dir = storage_dir.to_string_lossy().to_string();
                st.llama_runtime_ready = llama_ready;
                st.voice_runtime_ready = voice_ready;
            }
        }

        get_all_settings(&db).map_err(|e| e.to_string())?
    };

    if matches!(key.as_str(), "wake_word" | "local_storage_dir" | "voice_reply_enabled") {
        state.voice.reload_config();
    }

    Ok(settings)
}

#[tauri::command]
pub async fn install_llama_runtime(
    app: AppHandle,
    mirror: Option<String>,
    state: State<'_, AppState>,
) -> Result<RuntimeStatus, String> {
    state.download_cancel.store(false, Ordering::Relaxed);

    let (storage_dir, default_mirror) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let settings = get_all_settings(&db).map_err(|e| e.to_string())?;
        let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
        let default_mirror = settings.get("github_mirror").cloned().unwrap_or_default();
        (storage_dir, default_mirror)
    };

    let mirror_prefix = mirror.unwrap_or(default_mirror);
    if let Err(err) = install_llama_runtime_impl(
        &app,
        &storage_dir,
        &mirror_prefix,
        &state.download_cancel,
    )
    .await
    {
        emit_download_progress(
            &app,
            DownloadProgressEvent {
                target_id: "llama-runtime".to_string(),
                stage: "error".to_string(),
                downloaded_bytes: 0,
                total_bytes: 0,
                percent: 0,
                message: err.clone(),
                done: true,
                error: Some(err.clone()),
            },
        );
        return Err(err);
    }

    let status = state.status.lock().map_err(|e| e.to_string())?;
    let ollama_ok = status.ollama_connected;
    let ollama_models = status.ollama_models.clone();
    drop(status);
    sync_status_snapshot(&state, &storage_dir, ollama_ok, ollama_models)
}

#[tauri::command]
pub async fn install_voice_suite(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppSnapshot, String> {
    state.download_cancel.store(false, Ordering::Relaxed);

    let (storage_dir, mirror_prefix) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let settings = get_all_settings(&db).map_err(|e| e.to_string())?;
        let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
        let mirror_prefix = settings.get("github_mirror").cloned().unwrap_or_default();
        (storage_dir, mirror_prefix)
    };

    if find_sherpa_c_api_dll(&storage_dir).is_none() {
        install_sherpa_runtime(&app, &storage_dir, &mirror_prefix, &state.download_cancel).await?;
    }
    ensure_silero_vad(&app, &storage_dir, &mirror_prefix, &state.download_cancel).await?;
    download_voice_model_impl(
        &app,
        &storage_dir,
        "sherpa-kws-zh-en",
        &mirror_prefix,
        &state.download_cancel,
    )
    .await?;
    download_voice_model_impl(
        &app,
        &storage_dir,
        "sensevoice-small-int8",
        &mirror_prefix,
        &state.download_cancel,
    )
    .await?;

    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        set_active_model(&db, "kws", "sherpa-kws-zh-en").map_err(|e| e.to_string())?;
        set_active_model(&db, "asr", "sensevoice-small-int8").map_err(|e| e.to_string())?;
    }
    state.voice.reload_config();

    get_app_snapshot(state).await
}

#[tauri::command]
pub async fn download_voice_model(
    app: AppHandle,
    model_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ModelCatalogItem>, String> {
    state.download_cancel.store(false, Ordering::Relaxed);

    let (storage_dir, mirror_prefix) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let settings = get_all_settings(&db).map_err(|e| e.to_string())?;
        let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
        let mirror_prefix = settings.get("github_mirror").cloned().unwrap_or_default();
        (storage_dir, mirror_prefix)
    };

    if let Err(err) = download_voice_model_impl(
        &app,
        &storage_dir,
        &model_id,
        &mirror_prefix,
        &state.download_cancel,
    )
    .await
    {
        emit_download_progress(
            &app,
            DownloadProgressEvent {
                target_id: model_id.clone(),
                stage: "error".to_string(),
                downloaded_bytes: 0,
                total_bytes: 0,
                percent: 0,
                message: err.clone(),
                done: true,
                error: Some(err.clone()),
            },
        );
        return Err(err);
    }

    state.voice.reload_config();
    let db = state.db.lock().map_err(|e| e.to_string())?;
    list_catalog_with_state(&db, &storage_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn download_gguf_model(
    app: AppHandle,
    model_id: String,
    mirror: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ModelCatalogItem>, String> {
    state.download_cancel.store(false, Ordering::Relaxed);

    let spec = find_gguf_spec(&model_id)
        .ok_or_else(|| format!("模型 {model_id} 暂未配置内置直连下载源，请使用「导入本地 .gguf」"))?;

    let (storage_dir, chosen_mirror) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let settings = get_all_settings(&db).map_err(|e| e.to_string())?;
        let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
        let chosen_mirror = mirror
            .filter(|m| !m.trim().is_empty())
            .or_else(|| settings.get("download_mirror").cloned())
            .unwrap_or_else(|| "modelscope".to_string());
        (storage_dir, chosen_mirror)
    };

    let dest_path = default_gguf_path(&storage_dir, spec.filename);
    let url = gguf_download_url(spec, &chosen_mirror);

    if let Err(err) = crate::llm::downloader::download_file_with_progress(
        &app,
        &model_id,
        &format!("正在下载 {}", spec.filename),
        &url,
        &dest_path,
        &state.download_cancel,
    )
    .await
    {
        emit_download_progress(
            &app,
            DownloadProgressEvent {
                target_id: model_id.clone(),
                stage: "error".to_string(),
                downloaded_bytes: 0,
                total_bytes: 0,
                percent: 0,
                message: err.clone(),
                done: true,
                error: Some(err.clone()),
            },
        );
        return Err(err);
    }

    emit_download_progress(
        &app,
        DownloadProgressEvent {
            target_id: model_id.clone(),
            stage: "done".to_string(),
            downloaded_bytes: 1,
            total_bytes: 1,
            percent: 100,
            message: format!("{} 下载完成", spec.filename),
            done: true,
            error: None,
        },
    );

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let path_str = dest_path.to_string_lossy().to_string();
    set_model_local_path(&db, "llm", &model_id, Some(&path_str), true).map_err(|e| e.to_string())?;
    list_catalog_with_state(&db, &storage_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>) -> Result<(), String> {
    state.download_cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn import_local_gguf(
    model_id: String,
    file_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<ModelCatalogItem>, String> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() || !Path::new(trimmed).is_file() {
        return Err("所选 GGUF 文件不存在".to_string());
    }

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
    set_model_local_path(&db, "llm", &model_id, Some(trimmed), true).map_err(|e| e.to_string())?;
    set_active_model(&db, "llm", &model_id).map_err(|e| e.to_string())?;
    list_catalog_with_state(&db, &storage_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_local_model(
    model_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ModelCatalogItem>, String> {
    {
        let mut llama = state.llama.lock().map_err(|e| e.to_string())?;
        if llama.loaded_model_id().as_deref() == Some(model_id.as_str()) {
            llama.stop();
            if let Ok(mut st) = state.status.lock() {
                st.llama_server_running = false;
                st.llama_loaded_model = None;
            }
        }
    }

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
    if let Some(existing) = resolve_model_gguf_path(&db, &storage_dir, &model_id) {
        let managed_root = storage_dir.join("models").join("gguf");
        if existing.starts_with(&managed_root) {
            let _ = fs::remove_file(&existing);
        }
    } else if model_id == "sherpa-kws-zh-en"
        || model_id == "sensevoice-small-int8"
        || model_id == "paraformer-zh-streaming"
    {
        let folder = match model_id.as_str() {
            "sherpa-kws-zh-en" => "kws-zh-en",
            "paraformer-zh-streaming" => "paraformer-zh-int8",
            _ => "sensevoice-int8",
        };
        let target = voice_models_root(&storage_dir).join(folder);
        if target.exists() {
            let _ = fs::remove_dir_all(&target);
        }
        state.voice.reload_config();
    }
    set_model_local_path(&db, "llm", &model_id, None, false).map_err(|e| e.to_string())?;
    list_catalog_with_state(&db, &storage_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_llama_server(
    app: AppHandle,
    model_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<RuntimeStatus, String> {
    let (storage_dir, target_model_id, gguf_path) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
        let models = list_catalog_with_state(&db, &storage_dir).map_err(|e| e.to_string())?;
        let target = match model_id {
            Some(id) => id,
            None => models
                .into_iter()
                .find(|m| m.category == "llm" && m.is_active && m.engine == "llama-server")
                .map(|m| m.id)
                .ok_or_else(|| "请先选择一个内置本地 GGUF 模型".to_string())?,
        };
        let path = resolve_model_gguf_path(&db, &storage_dir, &target)
            .ok_or_else(|| format!("模型 {target} 尚未下载或导入 .gguf 文件"))?;
        (storage_dir, target, path)
    };

    ensure_llama_ready_for_model(&app, &storage_dir, &target_model_id, &gguf_path).await?;

    let status = state.status.lock().map_err(|e| e.to_string())?;
    let ollama_ok = status.ollama_connected;
    let ollama_models = status.ollama_models.clone();
    drop(status);
    sync_status_snapshot(&state, &storage_dir, ollama_ok, ollama_models)
}

#[tauri::command]
pub fn stop_llama_server(state: State<'_, AppState>) -> Result<RuntimeStatus, String> {
    {
        let mut llama = state.llama.lock().map_err(|e| e.to_string())?;
        llama.stop();
    }
    let mut status = state.status.lock().map_err(|e| e.to_string())?;
    status.llama_server_running = false;
    status.llama_loaded_model = None;
    if !status.python_worker_running {
        status.gpu_vram_active_mb = 0;
    }
    Ok(status.clone())
}

#[tauri::command]
pub async fn pick_local_gguf_file() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        let script = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Windows.Forms
$dlg = New-Object System.Windows.Forms.OpenFileDialog
$dlg.Title = '选择本地 GGUF 模型文件'
$dlg.Filter = 'GGUF 模型 (*.gguf)|*.gguf|所有文件 (*.*)|*.*'
if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    Write-Output $dlg.FileName
}
"#;
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-STA", "-Command", script]);
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = cmd.output().map_err(|e| format!("打开文件选择器失败: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(text))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn pick_storage_directory(initial: Option<String>) -> Result<Option<String>, String> {
    let init_dir = initial
        .filter(|p| PathBuf::from(p).exists())
        .unwrap_or_else(|| "D:\\".to_string());
    tokio::task::spawn_blocking(move || {
        let escaped = init_dir.replace('\'', "''");
        let script = format!(
            r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Windows.Forms
$dlg = New-Object System.Windows.Forms.FolderBrowserDialog
$dlg.Description = '选择 Skippy 模型与推理引擎存放目录（推荐 D 盘）'
$dlg.SelectedPath = '{escaped}'
if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{
    Write-Output $dlg.SelectedPath
}}
"#
        );
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-STA", "-Command", &script]);
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = cmd.output().map_err(|e| format!("打开目录选择器失败: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(text))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn start_python_worker(state: State<'_, AppState>) -> Result<WorkerHealth, String> {
    let base_url = {
        let mut worker = state.worker.lock().map_err(|e| e.to_string())?;
        worker.spawn_if_needed()?
    };

    let health = match wait_for_health(&base_url).await {
        Ok(h) => h,
        Err(e) => {
            if let Ok(mut worker) = state.worker.lock() {
                worker.stop();
            }
            return Err(e);
        }
    };

    let mut status = state.status.lock().map_err(|e| e.to_string())?;
    status.python_worker_running = true;
    status.gpu_vram_active_mb = health.gpu.vram_used_mb;
    Ok(health)
}

#[tauri::command]
pub fn stop_python_worker(state: State<'_, AppState>) -> Result<RuntimeStatus, String> {
    {
        let mut worker = state.worker.lock().map_err(|e| e.to_string())?;
        worker.stop();
    }
    let mut status = state.status.lock().map_err(|e| e.to_string())?;
    status.python_worker_running = false;
    if !status.llama_server_running {
        status.gpu_vram_active_mb = 0;
    }
    Ok(status.clone())
}
