use crate::llm::client::{
    build_system_prompt, emit_llm_stream, probe_ollama, stream_openai_chat, LlmStreamEvent,
    LlmTurn,
};
use crate::llm::llama_supervisor::wait_for_llama_ready;
use crate::models::registry::{list_catalog_with_state, resolve_model_gguf_path};
use crate::state::{AppState, VoiceStage};
use crate::storage::chat_repo::{
    append_message, get_all_settings, list_messages, list_sessions, resolve_local_storage_dir,
    update_message_content, update_session_title, ChatMessage,
};
use crate::voice::tts::TtsPlayer;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct SendMessageResult {
    pub user_message: ChatMessage,
    pub assistant_message: ChatMessage,
    pub session_title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatTurnStartedEvent {
    pub session_id: String,
    pub user_message: ChatMessage,
    pub assistant_message: ChatMessage,
    pub session_title: Option<String>,
}

struct TtsSentenceSplitter<'a> {
    tts: &'a TtsPlayer,
    gen_id: u64,
    in_think: bool,
    tag_buf: String,
    sentence_buf: String,
}

impl<'a> TtsSentenceSplitter<'a> {
    fn new(tts: &'a TtsPlayer, gen_id: u64) -> Self {
        Self {
            tts,
            gen_id,
            in_think: false,
            tag_buf: String::new(),
            sentence_buf: String::new(),
        }
    }

    fn feed(&mut self, piece: &str) {
        for ch in piece.chars() {
            if !self.tag_buf.is_empty() || ch == '<' {
                self.tag_buf.push(ch);
                if self.tag_buf == "<think>" {
                    self.in_think = true;
                    self.tag_buf.clear();
                    continue;
                }
                if self.tag_buf == "</think>" {
                    self.in_think = false;
                    self.tag_buf.clear();
                    continue;
                }
                if "<think>".starts_with(&self.tag_buf) || "</think>".starts_with(&self.tag_buf) {
                    continue;
                }
                // 不是 think 标签，把缓冲字符归还普通输出
                let flushed = std::mem::take(&mut self.tag_buf);
                if !self.in_think {
                    for fc in flushed.chars() {
                        self.push_visible_char(fc);
                    }
                }
                continue;
            }

            if !self.in_think {
                self.push_visible_char(ch);
            }
        }
    }

    fn push_visible_char(&mut self, ch: char) {
        self.sentence_buf.push(ch);
        let is_boundary = matches!(ch, '。' | '！' | '？' | '；' | '\n' | '!' | '?');
        if is_boundary && self.sentence_buf.chars().count() >= 4 {
            let chunk = std::mem::take(&mut self.sentence_buf);
            self.tts.enqueue_sentence(self.gen_id, chunk);
        } else if self.sentence_buf.chars().count() >= 48 && matches!(ch, '，' | ',' | '：' | ':') {
            let chunk = std::mem::take(&mut self.sentence_buf);
            self.tts.enqueue_sentence(self.gen_id, chunk);
        }
    }

    fn finish(&mut self) {
        if !self.in_think && !self.tag_buf.is_empty() {
            self.sentence_buf.push_str(&self.tag_buf);
            self.tag_buf.clear();
        }
        let tail = std::mem::take(&mut self.sentence_buf);
        if !tail.trim().is_empty() {
            self.tts.enqueue_sentence(self.gen_id, tail);
        }
    }
}

pub async fn ensure_llama_ready_for_model(
    app: &AppHandle,
    storage_dir: &Path,
    model_id: &str,
    gguf_path: &Path,
) -> Result<String, String> {
    let state = app.state::<AppState>();
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

pub async fn execute_chat_turn(
    app: &AppHandle,
    session_id: &str,
    content: &str,
    source: &str,
    speak_reply: bool,
) -> Result<SendMessageResult, String> {
    let state = app.state::<AppState>();
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
            || current_title.starts_with("语音对话")
        {
            let compact: String = content.trim().chars().take(18).collect();
            if !compact.is_empty() {
                let _ = update_session_title(&db, session_id, &compact);
                Some(compact)
            } else {
                None
            }
        } else {
            None
        };

        let existing_messages = list_messages(&db, session_id).map_err(|e| e.to_string())?;
        let user_message = append_message(&db, session_id, "user", source, content, None)
            .map_err(|e| e.to_string())?;

        let placeholder = append_message(
            &db,
            session_id,
            "assistant",
            source,
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
            content: content.to_string(),
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

    let _ = app.emit(
        "chat-turn-started",
        ChatTurnStartedEvent {
            session_id: session_id.to_string(),
            user_message: user_message.clone(),
            assistant_message: assistant_placeholder.clone(),
            session_title: new_session_title.clone(),
        },
    );

    emit_llm_stream(
        app,
        LlmStreamEvent {
            session_id: session_id.to_string(),
            message_id: msg_id.clone(),
            delta: String::new(),
            full_content: String::new(),
            stage: "preparing".to_string(),
            done: false,
            cancelled: false,
            error: None,
        },
    );

    let speak_enabled = speak_reply
        && settings
            .get("voice_reply_enabled")
            .map(|v| v != "false")
            .unwrap_or(true);

    let tts_gen = state.tts.current_generation();
    let mut splitter = if speak_enabled {
        Some(TtsSentenceSplitter::new(&state.tts, tts_gen))
    } else {
        None
    };

    let mut delta_cb = |piece: &str| {
        if let Some(sp) = splitter.as_mut() {
            sp.feed(piece);
        }
    };
    let cb_opt: Option<&mut (dyn FnMut(&str) + Send)> = if speak_enabled {
        Some(&mut delta_cb)
    } else {
        None
    };

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
                        app,
                        LlmStreamEvent {
                            session_id: session_id.to_string(),
                            message_id: msg_id.clone(),
                            delta: String::new(),
                            full_content: String::new(),
                            stage: "loading_model".to_string(),
                            done: false,
                            cancelled: false,
                            error: None,
                        },
                    );

                    match ensure_llama_ready_for_model(app, &storage_dir, &active_model.id, &path)
                        .await
                    {
                        Ok(base_url) => {
                            let endpoint = format!("{base_url}/v1/chat/completions");
                            stream_openai_chat(
                                app,
                                session_id,
                                &msg_id,
                                &endpoint,
                                None,
                                &active_model.id,
                                history,
                                &state.chat_cancel,
                                cb_opt,
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
                        app,
                        session_id,
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
                    app,
                    session_id,
                    &msg_id,
                    &endpoint,
                    None,
                    &model_name,
                    history,
                    &state.chat_cancel,
                    cb_opt,
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
                    app,
                    session_id,
                    &msg_id,
                    &endpoint,
                    Some(api_key),
                    cloud_model,
                    history,
                    &state.chat_cancel,
                    cb_opt,
                )
                .await
                .map(|(text, cancelled)| (text, cancelled, cloud_model.to_string()))
            }
        }
        other => Err(format!("暂不支持的大脑引擎类型：{other}")),
    };

    if let Some(sp) = splitter.as_mut() {
        sp.finish();
    }

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
                app,
                LlmStreamEvent {
                    session_id: session_id.to_string(),
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
            app,
            session_id,
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
    session_id: &str,
    user_message: ChatMessage,
    mut assistant_placeholder: ChatMessage,
    session_title: Option<String>,
    prev_stage: VoiceStage,
    prev_stage_label: String,
    err: String,
) -> Result<SendMessageResult, String> {
    let state = app.state::<AppState>();
    if let Ok(mut st) = state.status.lock() {
        st.stage = prev_stage;
        st.stage_label = prev_stage_label;
    }

    let friendly = format!("推理未完成：\n{err}");
    if let Ok(db) = state.db.lock() {
        let _ = update_message_content(
            &db,
            &assistant_placeholder.id,
            &friendly,
            Some("system-error"),
        );
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
