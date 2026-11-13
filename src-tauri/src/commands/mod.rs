use crate::models::registry::{list_catalog_with_state, set_active_model, ModelCatalogItem};
use crate::state::{AppState, RuntimeStatus, VoiceStage};
use crate::storage::chat_repo::{
    append_message, create_session, delete_session, get_all_settings, list_messages,
    list_sessions, set_setting, ChatMessage, ChatSession,
};
use serde::Serialize;
use std::collections::HashMap;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct AppSnapshot {
    pub status: RuntimeStatus,
    pub sessions: Vec<ChatSession>,
    pub models: Vec<ModelCatalogItem>,
    pub settings: HashMap<String, String>,
    pub data_dir: String,
}

#[tauri::command]
pub fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let sessions = list_sessions(&db).map_err(|e| e.to_string())?;
    let models = list_catalog_with_state(&db).map_err(|e| e.to_string())?;
    let settings = get_all_settings(&db).map_err(|e| e.to_string())?;
    let status = state.status.lock().map_err(|e| e.to_string())?.clone();
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
}

#[tauri::command]
pub fn send_text_message(
    session_id: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<SendMessageResult, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let models = list_catalog_with_state(&db).map_err(|e| e.to_string())?;
    let active_llm = models
        .iter()
        .find(|m| m.category == "llm" && m.is_active)
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "未配置模型".to_string());

    let user_message = append_message(&db, &session_id, "user", "pc_text", &content, None)
        .map_err(|e| e.to_string())?;

    // Step 1 骨架占位回复：Step 2 将替换为真实 Ollama / OpenAI-compatible SSE 流式推理
    let reply = format!(
        "已收到你的消息：「{}」\n\n当前为 Step 1 底座验证模式（已持久化至 SQLite）。Step 2 将接入 `{}` 进行流式对话响应。",
        content, active_llm
    );
    let assistant_message = append_message(
        &db,
        &session_id,
        "assistant",
        "pc_text",
        &reply,
        Some(&active_llm),
    )
    .map_err(|e| e.to_string())?;

    Ok(SendMessageResult {
        user_message,
        assistant_message,
    })
}

#[tauri::command]
pub fn select_active_model(
    category: String,
    model_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ModelCatalogItem>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
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

    list_catalog_with_state(&db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_app_setting(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<HashMap<String, String>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    set_setting(&db, &key, &value).map_err(|e| e.to_string())?;
    get_all_settings(&db).map_err(|e| e.to_string())
}
