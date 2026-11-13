pub mod chat_repo;

use rusqlite::{params, Connection, Result};
use std::path::Path;

pub fn init_db(db_path: &Path) -> Result<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            source TEXT NOT NULL,
            content TEXT NOT NULL,
            model_used TEXT,
            tool_calls_json TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS model_configs (
            category TEXT NOT NULL,
            model_id TEXT NOT NULL,
            is_active INTEGER NOT NULL DEFAULT 0,
            is_installed INTEGER NOT NULL DEFAULT 0,
            local_path TEXT,
            config_json TEXT,
            PRIMARY KEY (category, model_id)
        );

        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    )?;

    seed_defaults(&conn, db_path.parent())?;
    Ok(conn)
}

fn default_local_storage_dir(data_dir: Option<&Path>) -> String {
    if Path::new("D:\\").exists() {
        "D:\\SkippyData".to_string()
    } else if let Some(dir) = data_dir {
        dir.join("local").to_string_lossy().to_string()
    } else {
        "SkippyData".to_string()
    }
}

fn seed_defaults(conn: &Connection, data_dir: Option<&Path>) -> Result<()> {
    let storage_dir = default_local_storage_dir(data_dir);
    let default_settings = [
        ("theme", "system"),
        ("wake_mode", "shortcut"),
        ("wake_word", "Skippy"),
        ("shortcut", "Alt+Space"),
        ("idle_unload_secs", "180"),
        ("audio_ducking", "true"),
        ("voice_reply_enabled", "true"),
        ("persona_mode", "butler"),
        ("local_storage_dir", storage_dir.as_str()),
        ("download_mirror", "modelscope"),
        ("github_mirror", ""),
        ("cloud_base_url", "https://api.deepseek.com/v1"),
        ("cloud_model", "deepseek-chat"),
        ("cloud_api_key", ""),
        ("ollama_base_url", "http://127.0.0.1:11434"),
        ("ollama_model", ""),
    ];

    for (k, v) in default_settings {
        conn.execute(
            "INSERT OR IGNORE INTO app_settings (key, value) VALUES (?1, ?2)",
            params![k, v],
        )?;
    }

    let default_models = [
        ("kws", "shortcut-only", 1, 1),
        ("kws", "sherpa-kws-zh-en", 0, 0),
        ("kws", "openwakeword-en", 0, 0),
        ("asr", "sensevoice-small-int8", 1, 0),
        ("asr", "paraformer-zh-streaming", 0, 0),
        ("asr", "qwen3-asr-0.6b-gpu", 0, 0),
        ("asr", "cloud-asr-api", 0, 1),
        ("llm", "cloud-openai-compat", 0, 1),
        ("llm", "llama-qwen3-4b-gguf", 0, 0),
        ("llm", "llama-qwen3-8b-gguf", 1, 0),
        ("llm", "llama-uitars-1.5-7b-gguf", 0, 0),
        ("llm", "ollama-external", 0, 0),
        ("tts", "edge-tts-online", 0, 1),
        ("tts", "winrt-native-tts", 1, 1),
        ("tts", "kokoro-82m-zh", 0, 0),
        ("tts", "cosyvoice-gpu-worker", 0, 0),
    ];

    for (cat, id, active, installed) in default_models {
        conn.execute(
            "INSERT OR IGNORE INTO model_configs (category, model_id, is_active, is_installed) VALUES (?1, ?2, ?3, ?4)",
            params![cat, id, active, installed],
        )?;
    }

    // 针对尚未配置过云端 Key 的已有本地库，一次性将默认大脑迁移为纯本地 Qwen3-8B GGUF
    let already_migrated: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'migrated_local_default_v1'",
            [],
            |r| r.get(0),
        )
        .ok();
    if already_migrated.is_none() {
        let cloud_active: i64 = conn
            .query_row(
                "SELECT is_active FROM model_configs WHERE category = 'llm' AND model_id = 'cloud-openai-compat'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let cloud_key: String = conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'cloud_api_key'",
                [],
                |r| r.get(0),
            )
            .unwrap_or_default();

        if cloud_active != 0 && cloud_key.trim().is_empty() {
            conn.execute("UPDATE model_configs SET is_active = 0 WHERE category = 'llm'", [])?;
            conn.execute(
                "UPDATE model_configs SET is_active = 1 WHERE category = 'llm' AND model_id = 'llama-qwen3-8b-gguf'",
                [],
            )?;
        }
        conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('migrated_local_default_v1', '1')",
            [],
        )?;
    }

    let tts_migrated: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'migrated_local_tts_v1'",
            [],
            |r| r.get(0),
        )
        .ok();
    if tts_migrated.is_none() {
        let edge_active: i64 = conn
            .query_row(
                "SELECT is_active FROM model_configs WHERE category = 'tts' AND model_id = 'edge-tts-online'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if edge_active != 0 {
            conn.execute("UPDATE model_configs SET is_active = 0 WHERE category = 'tts'", [])?;
            conn.execute(
                "UPDATE model_configs SET is_active = 1 WHERE category = 'tts' AND model_id = 'winrt-native-tts'",
                [],
            )?;
        }
        conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('migrated_local_tts_v1', '1')",
            [],
        )?;
    }

    let session_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
    if session_count == 0 {
        let now = chrono::Utc::now().to_rfc3339();
        let sid = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO sessions (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![sid, "欢迎使用 Skippy", now, now],
        )?;
        let mid = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO messages (id, session_id, role, source, content, model_used, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                mid,
                sid,
                "assistant",
                "system",
                "Skippy 第一期底座已就绪。\n\n- 点击右上角关闭按钮会销毁 GUI 渲染进程，仅保留 Rust 托盘后台低占用待机。\n- 使用托盘图标或全局快捷键 `Alt + Space` 可随时重建窗口。\n- 打开右上角「模型管理」即可查看按功能分类的唤醒 / 识别 / 大脑 / 语音合成模型与 5070 显存预算。",
                "system-init",
                now
            ],
        )?;
    }

    Ok(())
}
