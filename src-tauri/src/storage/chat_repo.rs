use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub source: String,
    pub content: String,
    pub model_used: Option<String>,
    pub tool_calls_json: Option<String>,
    pub created_at: String,
}

pub fn list_sessions(conn: &Connection) -> Result<Vec<ChatSession>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, created_at, updated_at FROM sessions ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ChatSession {
            id: row.get(0)?,
            title: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
        })
    })?;
    rows.collect()
}

pub fn create_session(conn: &Connection, title: &str) -> Result<ChatSession> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO sessions (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, title, now, now],
    )?;
    Ok(ChatSession {
        id,
        title: title.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn delete_session(conn: &Connection, session_id: &str) -> Result<()> {
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;
    Ok(())
}

pub fn list_messages(conn: &Connection, session_id: &str) -> Result<Vec<ChatMessage>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, source, content, model_used, tool_calls_json, created_at
         FROM messages WHERE session_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        Ok(ChatMessage {
            id: row.get(0)?,
            session_id: row.get(1)?,
            role: row.get(2)?,
            source: row.get(3)?,
            content: row.get(4)?,
            model_used: row.get(5)?,
            tool_calls_json: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?;
    rows.collect()
}

pub fn append_message(
    conn: &Connection,
    session_id: &str,
    role: &str,
    source: &str,
    content: &str,
    model_used: Option<&str>,
) -> Result<ChatMessage> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO messages (id, session_id, role, source, content, model_used, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, session_id, role, source, content, model_used, now],
    )?;
    conn.execute(
        "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
        params![now, session_id],
    )?;
    Ok(ChatMessage {
        id,
        session_id: session_id.to_string(),
        role: role.to_string(),
        source: source.to_string(),
        content: content.to_string(),
        model_used: model_used.map(|s| s.to_string()),
        tool_calls_json: None,
        created_at: now,
    })
}

pub fn get_all_settings(conn: &Connection) -> Result<std::collections::HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM app_settings")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut map = std::collections::HashMap::new();
    for item in rows {
        let (k, v) = item?;
        map.insert(k, v);
    }
    Ok(map)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn update_message_content(
    conn: &Connection,
    message_id: &str,
    content: &str,
    model_used: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE messages SET content = ?1, model_used = ?2 WHERE id = ?3",
        params![content, model_used, message_id],
    )?;
    Ok(())
}

pub fn update_session_title(conn: &Connection, session_id: &str, title: &str) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET title = ?1 WHERE id = ?2",
        params![title, session_id],
    )?;
    Ok(())
}

pub fn resolve_local_storage_dir(conn: &Connection, data_dir: &std::path::Path) -> std::path::PathBuf {
    let custom: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'local_storage_dir'",
            [],
            |r| r.get(0),
        )
        .ok();

    if let Some(raw) = custom {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return std::path::PathBuf::from(trimmed);
        }
    }

    if std::path::Path::new("D:\\").exists() {
        std::path::PathBuf::from("D:\\SkippyData")
    } else {
        data_dir.join("local")
    }
}
