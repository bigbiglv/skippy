use crate::llm::client::{
    build_system_prompt, emit_llm_stream, probe_ollama, stream_openai_chat, LlmStreamEvent,
    LlmTurn,
};
use crate::llm::downloader::{download_file_with_progress, emit_download_progress, DownloadProgressEvent};
use crate::llm::llama_supervisor::{
    install_llama_runtime as install_llama_runtime_impl, wait_for_llama_ready, LlamaSupervisor,
};
use crate::models::registry::{
    default_gguf_path, find_gguf_spec, gguf_download_url, list_catalog_with_state,
    resolve_model_gguf_path, set_active_model, set_model_local_path, ModelCatalogItem,
};
use crate::state::{AppState, RuntimeStatus, VoiceStage};
use crate::storage::chat_repo::{
    append_message, create_session, delete_session, get_all_settings, list_messages,
    list_sessions, resolve_local_storage_dir, set_setting, update_message_content,
    update_session_title, ChatMessage, ChatSession,
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

    let mut status = state.status.lock().map_err(|e| e.to_string())?;
    status.python_worker_running = worker_running;
    status.python_env = python_env;
    status.llama_runtime_ready = llama_ready;
    status.llama_server_running = llama_running;
    status.llama_loaded_model = llama_model;
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

#[derive(Debug, Serialize)]
pub struct SendMessageResult {
    pub user_message: ChatMessage,
    pub assistant_message: ChatMessage,
    pub session_title: Option<String>,
}

async fn ensure_llama_ready_for_model(
    app: &AppHandle,
    state: &State<'_, AppState>,
    storage_dir: &Path,
    model_id: &str,
    gguf_path: &Path,
) -> Result<String, String> {
    let (base_url, newly_spawned) = {
        let mut llama = state.llama.lock().map_err(|e| e.to_string())?;
        llama.spawn_for_model(storage_dir, model_id, gguf_path)?
    };

    if newly_spawned {
        let wait_res = wait_for_llama_ready(app, &base_url, || {
            state
                .llama
                .lock()
                .ok()
                .and_then(|mut l| l.take_exit_error())
        })
        .await;

        if let Err(err) = wait_res {
            if let Ok(mut l) = state.llama.lock() {
                l.stop();
            }
            if let Ok(mut st) = state.status.lock() {
                st.llama_server_running = false;
                st.llama_loaded_model = None;
            }
            return Err(err);
        }
    }

    if let Ok(mut st) = state.status.lock() {
        st.llama_runtime_ready = true;
        st.llama_server_running = true;
        st.llama_loaded_model = Some(model_id.to_string());
    }

    Ok(base_url)
}

#[tauri::command]
pub fn stop_generation(state: State<'_, AppState>) -> Result<(), String> {
    state.chat_cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn send_text_message(
    app: AppHandle,
    session_id: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<SendMessageResult, String> {
    state.chat_cancel.store(false, Ordering::Relaxed);

    let (
        storage_dir,
        settings,
        active_model,
        user_message,
        assistant_placeholder,
        history,
        new_session_title,
        prev_stage,
        prev_stage_label,
    ) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
        let settings = get_all_settings(&db).map_err(|e| e.to_string())?;
        let models = list_catalog_with_state(&db, &storage_dir).map_err(|e| e.to_string())?;

        let active_model = models
            .into_iter()
            .find(|m| m.category == "llm" && m.is_active)
            .ok_or_else(|| "尚未选择对话大脑模型，请先打开「模型管理」选择一个模型".to_string())?;

        let sessions = list_sessions(&db).map_err(|e| e.to_string())?;
        let current_title = sessions
            .into_iter()
            .find(|s| s.id == session_id)
            .map(|s| s.title)
            .unwrap_or_default();

        let auto_title = if current_title.starts_with("欢迎使用 Skippy")
            || current_title.starts_with("新对话")
        {
            let compact: String = content.trim().chars().take(18).collect();
            if !compact.is_empty() {
                let _ = update_session_title(&db, &session_id, &compact);
                Some(compact)
            } else {
                None
            }
        } else {
            None
        };

        let existing_messages = list_messages(&db, &session_id).map_err(|e| e.to_string())?;
        let user_message = append_message(&db, &session_id, "user", "pc_text", &content, None)
            .map_err(|e| e.to_string())?;

        let placeholder = append_message(
            &db,
            &session_id,
            "assistant",
            "pc_text",
            "",
            Some(&active_model.name),
        )
        .map_err(|e| e.to_string())?;

        let persona = settings
            .get("persona_mode")
            .map(String::as_str)
            .unwrap_or("butler");
        let mut turns = vec![LlmTurn {
            role: "system".to_string(),
            content: build_system_prompt(persona),
        }];
        let recent_start = existing_messages.len().saturating_sub(14);
        for m in &existing_messages[recent_start..] {
            if m.source == "system" || m.content.trim().is_empty() {
                continue;
            }
            turns.push(LlmTurn {
                role: m.role.clone(),
                content: m.content.clone(),
            });
        }
        turns.push(LlmTurn {
            role: "user".to_string(),
            content: content.clone(),
        });

        let mut st = state.status.lock().map_err(|e| e.to_string())?;
        let prev_stage = st.stage.clone();
        let prev_label = st.stage_label.clone();
        st.stage = VoiceStage::Thinking;
        st.stage_label = format!("正在准备 {}...", active_model.name);

        (
            storage_dir,
            settings,
            active_model,
            user_message,
            placeholder,
            turns,
            auto_title,
            prev_stage,
            prev_label,
        )
    };

    let msg_id = assistant_placeholder.id.clone();

    emit_llm_stream(
        &app,
        LlmStreamEvent {
            session_id: session_id.clone(),
            message_id: msg_id.clone(),
            delta: String::new(),
            full_content: String::new(),
            stage: "preparing".to_string(),
            done: false,
            cancelled: false,
            error: None,
        },
    );

    let stream_res: Result<(String, bool, String), String> = match active_model.engine.as_str() {
        "llama-server" => {
            let gguf_path = {
                let db = state.db.lock().map_err(|e| e.to_string())?;
                resolve_model_gguf_path(&db, &storage_dir, &active_model.id)
            };

            match gguf_path {
                None => Err(format!(
                    "尚未下载或导入「{}」权重文件（.gguf）。\n请点击右上角「模型管理」，下载权重或导入本机已有 .gguf 文件。",
                    active_model.name
                )),
                Some(path) => {
                    emit_llm_stream(
                        &app,
                        LlmStreamEvent {
                            session_id: session_id.clone(),
                            message_id: msg_id.clone(),
                            delta: String::new(),
                            full_content: String::new(),
                            stage: "loading_model".to_string(),
                            done: false,
                            cancelled: false,
                            error: None,
                        },
                    );

                    match ensure_llama_ready_for_model(
                        &app,
                        &state,
                        &storage_dir,
                        &active_model.id,
                        &path,
                    )
                    .await
                    {
                        Ok(base_url) => {
                            let endpoint = format!("{base_url}/v1/chat/completions");
                            stream_openai_chat(
                                &app,
                                &session_id,
                                &msg_id,
                                &endpoint,
                                None,
                                &active_model.id,
                                history,
                                &state.chat_cancel,
                            )
                            .await
                            .map(|(text, cancelled)| {
                                if let Ok(mut l) = state.llama.lock() {
                                    l.touch();
                                }
                                (text, cancelled, active_model.name.clone())
                            })
                        }
                        Err(e) => Err(e),
                    }
                }
            }
        }
        "ollama" => {
            let base_url = settings
                .get("ollama_base_url")
                .map(String::as_str)
                .unwrap_or("http://127.0.0.1:11434")
                .trim_end_matches('/');
            let configured_model = settings
                .get("ollama_model")
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            let model_name = if !configured_model.is_empty() {
                configured_model
            } else {
                let (ok, tags) = probe_ollama(base_url).await;
                if !ok {
                    return finish_failed_chat(
                        &app,
                        &state,
                        &session_id,
                        user_message,
                        assistant_placeholder,
                        new_session_title,
                        prev_stage,
                        prev_stage_label,
                        format!("无法连接到本机 Ollama（{base_url}），请确认 Ollama 已启动，或切换为 Skippy 内置本地 GGUF 模型。"),
                    );
                }
                tags.into_iter().next().unwrap_or_default()
            };

            if model_name.is_empty() {
                Err("本机 Ollama 中未找到可用模型，请先 `ollama pull` 模型或使用内置 GGUF 模型。".to_string())
            } else {
                let endpoint = format!("{base_url}/v1/chat/completions");
                stream_openai_chat(
                    &app,
                    &session_id,
                    &msg_id,
                    &endpoint,
                    None,
                    &model_name,
                    history,
                    &state.chat_cancel,
                )
                .await
                .map(|(text, cancelled)| (text, cancelled, format!("Ollama · {model_name}")))
            }
        }
        "cloud" => {
            let base_url = settings
                .get("cloud_base_url")
                .map(String::as_str)
                .unwrap_or("")
                .trim_end_matches('/');
            let api_key = settings
                .get("cloud_api_key")
                .map(String::as_str)
                .unwrap_or("");
            let cloud_model = settings
                .get("cloud_model")
                .map(String::as_str)
                .unwrap_or("");

            if base_url.is_empty() || api_key.trim().is_empty() || cloud_model.trim().is_empty() {
                Err("当前选中了云端 API，但尚未配置 API Key / Model ID；请打开「设置」填写，或在「模型管理」切换回本地 GGUF 模型。".to_string())
            } else {
                let endpoint = format!("{base_url}/chat/completions");
                stream_openai_chat(
                    &app,
                    &session_id,
                    &msg_id,
                    &endpoint,
                    Some(api_key),
                    cloud_model,
                    history,
                    &state.chat_cancel,
                )
                .await
                .map(|(text, cancelled)| (text, cancelled, cloud_model.to_string()))
            }
        }
        other => Err(format!("暂不支持的大脑引擎类型：{other}")),
    };

    if let Ok(mut st) = state.status.lock() {
        st.stage = prev_stage.clone();
        st.stage_label = prev_stage_label.clone();
    }

    match stream_res {
        Ok((text, cancelled, model_label)) => {
            let final_text = if text.trim().is_empty() && cancelled {
                "（已停止生成）".to_string()
            } else if text.trim().is_empty() {
                "（模型未返回内容）".to_string()
            } else {
                text
            };

            {
                let db = state.db.lock().map_err(|e| e.to_string())?;
                update_message_content(&db, &msg_id, &final_text, Some(&model_label))
                    .map_err(|e| e.to_string())?;
            }

            emit_llm_stream(
                &app,
                LlmStreamEvent {
                    session_id: session_id.clone(),
                    message_id: msg_id.clone(),
                    delta: String::new(),
                    full_content: final_text.clone(),
                    stage: "done".to_string(),
                    done: true,
                    cancelled,
                    error: None,
                },
            );

            let mut final_assistant = assistant_placeholder;
            final_assistant.content = final_text;
            final_assistant.model_used = Some(model_label);

            Ok(SendMessageResult {
                user_message,
                assistant_message: final_assistant,
                session_title: new_session_title,
            })
        }
        Err(err) => finish_failed_chat(
            &app,
            &state,
            &session_id,
            user_message,
            assistant_placeholder,
            new_session_title,
            prev_stage,
            prev_stage_label,
            err,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_failed_chat(
    app: &AppHandle,
    state: &State<'_, AppState>,
    session_id: &str,
    user_message: ChatMessage,
    mut assistant_placeholder: ChatMessage,
    session_title: Option<String>,
    prev_stage: VoiceStage,
    prev_stage_label: String,
    err: String,
) -> Result<SendMessageResult, String> {
    if let Ok(mut st) = state.status.lock() {
        st.stage = prev_stage;
        st.stage_label = prev_stage_label;
    }

    let friendly = format!("推理未完成：\n{err}");
    if let Ok(db) = state.db.lock() {
        let _ = update_message_content(&db, &assistant_placeholder.id, &friendly, Some("system-error"));
    }

    emit_llm_stream(
        app,
        LlmStreamEvent {
            session_id: session_id.to_string(),
            message_id: assistant_placeholder.id.clone(),
            delta: String::new(),
            full_content: friendly.clone(),
            stage: "error".to_string(),
            done: true,
            cancelled: false,
            error: Some(err),
        },
    );

    assistant_placeholder.content = friendly;
    assistant_placeholder.model_used = Some("system-error".to_string());

    Ok(SendMessageResult {
        user_message,
        assistant_message: assistant_placeholder,
        session_title,
    })
}

#[tauri::command]
pub fn select_active_model(
    category: String,
    model_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ModelCatalogItem>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
    set_active_model(&db, &category, &model_id).map_err(|e| e.to_string())?;

    if category == "kws" {
        let mut status = state.status.lock().map_err(|e| e.to_string())?;
        if model_id == "shortcut-only" {
            status.stage = VoiceStage::ShortcutOnly;
            status.stage_label = "待机低占用（快捷键唤醒）".to_string();
        } else {
            status.stage = VoiceStage::IdleListening;
            status.stage_label = "待机监听中（Step 3 接入音频流）".to_string();
        }
    }

    list_catalog_with_state(&db, &storage_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_app_setting(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<HashMap<String, String>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    set_setting(&db, &key, &value).map_err(|e| e.to_string())?;

    if key == "local_storage_dir" {
        let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
        let llama_ready = LlamaSupervisor::find_binary(&storage_dir).is_some();
        if let Ok(mut st) = state.status.lock() {
            st.local_storage_dir = storage_dir.to_string_lossy().to_string();
            st.llama_runtime_ready = llama_ready;
        }
    }

    get_all_settings(&db).map_err(|e| e.to_string())
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

    if let Err(err) = download_file_with_progress(
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

    ensure_llama_ready_for_model(&app, &state, &storage_dir, &target_model_id, &gguf_path).await?;

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
