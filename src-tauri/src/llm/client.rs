use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
pub struct LlmTurn {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmStreamEvent {
    pub session_id: String,
    pub message_id: String,
    pub delta: String,
    pub full_content: String,
    pub stage: String,
    pub done: bool,
    pub cancelled: bool,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Option<Vec<OllamaModelTag>>,
}

#[derive(Debug, Deserialize)]
struct OllamaModelTag {
    name: String,
}

pub fn emit_llm_stream(app: &AppHandle, payload: LlmStreamEvent) {
    let _ = app.emit("llm-stream", payload);
}

pub fn build_system_prompt(persona_mode: &str) -> String {
    match persona_mode {
        "skippy_love" => {
            "你是 Skippy，一名部署在用户本地 Windows 电脑上的个人 AI 助手（名字灵感源自《赛博朋克 2077》里的智能手枪四季宝）。\
请用中文回答问题，风格可以略带机灵、幽默与轻微吐槽感，但回答务必准确、条理清晰、不拖泥带水。".to_string()
        }
        _ => {
            "你是 Skippy，一名部署在用户本地 Windows 电脑上的个人 AI 助手。\
请默认使用清晰、简洁、准确的中文回答用户的问题。".to_string()
        }
    }
}

pub async fn probe_ollama(base_url: &str) -> (bool, Vec<String>) {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return (false, Vec::new());
    }

    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_millis(1200))
        .build()
    else {
        return (false, Vec::new());
    };

    let url = format!("{trimmed}/api/tags");
    let Ok(resp) = client.get(&url).send().await else {
        return (false, Vec::new());
    };
    if !resp.status().is_success() {
        return (false, Vec::new());
    }

    match resp.json::<OllamaTagsResponse>().await {
        Ok(parsed) => {
            let names = parsed
                .models
                .unwrap_or_default()
                .into_iter()
                .map(|m| m.name)
                .collect();
            (true, names)
        }
        Err(_) => (true, Vec::new()),
    }
}

pub async fn stream_openai_chat(
    app: &AppHandle,
    session_id: &str,
    message_id: &str,
    endpoint_url: &str,
    api_key: Option<&str>,
    model_name: &str,
    messages: Vec<LlmTurn>,
    cancel_flag: &AtomicBool,
    mut on_answer_delta: Option<&mut (dyn FnMut(&str) + Send)>,
) -> Result<(String, bool), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("创建推理请求客户端失败: {e}"))?;

    let mut req = client.post(endpoint_url).json(&json!({
        "model": model_name,
        "messages": messages,
        "stream": true,
        "temperature": 0.7,
    }));

    if let Some(key) = api_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            req = req.bearer_auth(trimmed);
        }
    }

    let mut resp = req
        .send()
        .await
        .map_err(|e| format!("连接模型服务失败 ({endpoint_url}): {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("模型接口返回错误 {status}: {body}"));
    }

    let mut raw_buf: Vec<u8> = Vec::new();
    let mut accumulated = String::new();
    let mut in_thinking = false;
    let mut cancelled = false;

    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("读取推理流失败: {e}"))?
    {
        if cancel_flag.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        raw_buf.extend_from_slice(&chunk);

        while let Some(pos) = raw_buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = raw_buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes).trim().to_string();
            if line.is_empty() || !line.starts_with("data:") {
                continue;
            }
            let data = line.trim_start_matches("data:").trim();
            if data == "[DONE]" {
                break;
            }

            let Ok(val) = serde_json::from_str::<serde_json::Value>(data) else {
                continue;
            };

            let delta_obj = &val["choices"][0]["delta"];
            let reasoning_piece = delta_obj["reasoning_content"]
                .as_str()
                .unwrap_or_default();
            let content_piece = delta_obj["content"].as_str().unwrap_or_default();

            let mut out_delta = String::new();
            if !reasoning_piece.is_empty() {
                if !in_thinking {
                    in_thinking = true;
                    out_delta.push_str("<think>");
                }
                out_delta.push_str(reasoning_piece);
            }

            if !content_piece.is_empty() {
                if in_thinking {
                    in_thinking = false;
                    out_delta.push_str("</think>\n\n");
                }
                out_delta.push_str(content_piece);
                if let Some(cb) = on_answer_delta.as_deref_mut() {
                    cb(content_piece);
                }
            }

            if !out_delta.is_empty() {
                accumulated.push_str(&out_delta);
                emit_llm_stream(
                    app,
                    LlmStreamEvent {
                        session_id: session_id.to_string(),
                        message_id: message_id.to_string(),
                        delta: out_delta,
                        full_content: accumulated.clone(),
                        stage: "streaming".to_string(),
                        done: false,
                        cancelled: false,
                        error: None,
                    },
                );
            }
        }
    }

    if in_thinking {
        let close_tag = "</think>";
        accumulated.push_str(close_tag);
    }

    Ok((accumulated, cancelled))
}
