use crate::voice::assets::{
    find_asr_paths, find_voice_asset_spec, is_voice_model_installed, kws_model_dir,
};
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCatalogItem {
    pub id: String,
    pub category: String,
    pub name: String,
    pub engine: String,
    pub compute: String,
    pub est_vram_mb: u32,
    pub disk_size_mb: u32,
    pub description: String,
    pub tags: Vec<String>,
    pub requires_python: bool,
    pub requires_ollama: bool,
    pub is_active: bool,
    pub is_installed: bool,
    pub local_path: Option<String>,
    pub downloadable: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct GgufDownloadSpec {
    pub model_id: &'static str,
    pub repo: &'static str,
    pub filename: &'static str,
}

const GGUF_SPECS: &[GgufDownloadSpec] = &[
    GgufDownloadSpec {
        model_id: "llama-qwen3-4b-gguf",
        repo: "Qwen/Qwen3-4B-GGUF",
        filename: "Qwen3-4B-Q4_K_M.gguf",
    },
    GgufDownloadSpec {
        model_id: "llama-qwen3-8b-gguf",
        repo: "Qwen/Qwen3-8B-GGUF",
        filename: "Qwen3-8B-Q4_K_M.gguf",
    },
];

pub fn find_gguf_spec(model_id: &str) -> Option<&'static GgufDownloadSpec> {
    GGUF_SPECS.iter().find(|s| s.model_id == model_id)
}

pub fn gguf_download_url(spec: &GgufDownloadSpec, mirror: &str) -> String {
    match mirror {
        "hf-mirror" => format!(
            "https://hf-mirror.com/{}/resolve/main/{}",
            spec.repo, spec.filename
        ),
        "huggingface" => format!(
            "https://huggingface.co/{}/resolve/main/{}",
            spec.repo, spec.filename
        ),
        _ => format!(
            "https://modelscope.cn/models/{}/resolve/master/{}",
            spec.repo, spec.filename
        ),
    }
}

pub fn default_gguf_path(storage_dir: &Path, filename: &str) -> PathBuf {
    storage_dir.join("models").join("gguf").join(filename)
}

struct BuiltinDef {
    id: &'static str,
    category: &'static str,
    name: &'static str,
    engine: &'static str,
    compute: &'static str,
    est_vram_mb: u32,
    disk_size_mb: u32,
    description: &'static str,
    tags: &'static [&'static str],
    requires_python: bool,
    requires_ollama: bool,
}

const CATALOG: &[BuiltinDef] = &[
    // KWS 语音唤醒
    BuiltinDef {
        id: "shortcut-only",
        category: "kws",
        name: "仅快捷键唤醒 (Alt+Space)",
        engine: "native",
        compute: "cpu",
        est_vram_mb: 0,
        disk_size_mb: 0,
        description: "关闭麦克风常驻监听，不亮任务栏麦克风隐私图标，0 额外占用。",
        tags: &["0占用", "无麦克风权限"],
        requires_python: false,
        requires_ollama: false,
    },
    BuiltinDef {
        id: "sherpa-kws-zh-en",
        category: "kws",
        name: "sherpa-onnx Zipformer KWS",
        engine: "sherpa-onnx",
        compute: "cpu",
        est_vram_mb: 0,
        disk_size_mb: 12,
        description: "支持中文/中英混合自定义唤醒词（如 密探阿泰），常驻 CPU 约 1%。",
        tags: &["推荐", "纯Rust", "中英唤醒"],
        requires_python: false,
        requires_ollama: false,
    },
    BuiltinDef {
        id: "openwakeword-en",
        category: "kws",
        name: "openWakeWord (英文轻量)",
        engine: "onnx",
        compute: "cpu",
        est_vram_mb: 0,
        disk_size_mb: 8,
        description: "社区轻量英文唤醒模型，适合纯英文短词唤醒。",
        tags: &["英文", "轻量"],
        requires_python: false,
        requires_ollama: false,
    },
    // ASR 语音识别
    BuiltinDef {
        id: "sensevoice-small-int8",
        category: "asr",
        name: "SenseVoice-Small (INT8)",
        engine: "sherpa-onnx",
        compute: "cpu",
        est_vram_mb: 0,
        disk_size_mb: 230,
        description: "阿里开源极速非自回归识别，7800X3D 纯 CPU 几十毫秒出字，中英混杂抗噪强。",
        tags: &["默认推荐", "纯Rust", "0显存"],
        requires_python: false,
        requires_ollama: false,
    },
    BuiltinDef {
        id: "paraformer-zh-streaming",
        category: "asr",
        name: "Paraformer-zh Streaming",
        engine: "sherpa-onnx",
        compute: "cpu",
        est_vram_mb: 0,
        disk_size_mb: 220,
        description: "工业级真流式中文识别，边说话边实时上屏。",
        tags: &["流式上屏", "纯Rust"],
        requires_python: false,
        requires_ollama: false,
    },
    BuiltinDef {
        id: "qwen3-asr-0.6b-gpu",
        category: "asr",
        name: "Qwen3-ASR 0.6B (GPU Worker)",
        engine: "python-worker",
        compute: "gpu",
        est_vram_mb: 1600,
        disk_size_mb: 1200,
        description: "高精度歌名/专有名词识别，唤醒后按需加载至 5070 显存。",
        tags: &["高精度", "需Python"],
        requires_python: true,
        requires_ollama: false,
    },
    BuiltinDef {
        id: "cloud-asr-api",
        category: "asr",
        name: "云端兼容 ASR 接口",
        engine: "cloud",
        compute: "cloud",
        est_vram_mb: 0,
        disk_size_mb: 0,
        description: "调用兼容 OpenAI Whisper 的云端接口，本地零存储与算力消耗。",
        tags: &["云端", "0本地占用"],
        requires_python: false,
        requires_ollama: false,
    },
    // LLM 对话与操控大脑
    BuiltinDef {
        id: "cloud-openai-compat",
        category: "llm",
        name: "云端 OpenAI 兼容 API",
        engine: "cloud",
        compute: "cloud",
        est_vram_mb: 0,
        disk_size_mb: 0,
        description: "支持 DeepSeek / 通义千问 / OpenAI / Gemini 等兼容接口，即配即用。",
        tags: &["即开即用", "云端"],
        requires_python: false,
        requires_ollama: false,
    },
    BuiltinDef {
        id: "llama-qwen3-4b-gguf",
        category: "llm",
        name: "内置本地: Qwen3-4B (GGUF Q4)",
        engine: "llama-server",
        compute: "gpu",
        est_vram_mb: 3400,
        disk_size_mb: 2800,
        description: "由 Skippy 内置 llama-server 按需拉起（免装 Ollama），用完超时杀进程 100% 释放显存。",
        tags: &["内置托管", "本地极速", "免装Ollama"],
        requires_python: false,
        requires_ollama: false,
    },
    BuiltinDef {
        id: "llama-qwen3-8b-gguf",
        category: "llm",
        name: "内置本地: Qwen3-8B (GGUF Q4)",
        engine: "llama-server",
        compute: "gpu",
        est_vram_mb: 6200,
        disk_size_mb: 5200,
        description: "综合对话与指令推理甜点模型，由内置 llama-server 托管运行在 RTX 5070。",
        tags: &["内置托管", "推荐本地", "免装Ollama"],
        requires_python: false,
        requires_ollama: false,
    },
    BuiltinDef {
        id: "llama-uitars-1.5-7b-gguf",
        category: "llm",
        name: "内置视觉: UI-TARS-1.5-7B (GGUF)",
        engine: "llama-server",
        compute: "gpu",
        est_vram_mb: 7600,
        disk_size_mb: 5800,
        description: "二期专用屏幕视觉操控模型，由内置 llama-server 多模态按需加载。",
        tags: &["内置托管", "二期控电脑", "视觉"],
        requires_python: false,
        requires_ollama: false,
    },
    BuiltinDef {
        id: "ollama-external",
        category: "llm",
        name: "连接本机已有 Ollama (可选复用)",
        engine: "ollama",
        compute: "gpu",
        est_vram_mb: 6000,
        disk_size_mb: 0,
        description: "若本机已安装 Ollama，可直接复用其中已下载的模型，不重复占用硬盘。",
        tags: &["可选复用", "需Ollama"],
        requires_python: false,
        requires_ollama: true,
    },
    // TTS 语音合成
    BuiltinDef {
        id: "edge-tts-online",
        category: "tts",
        name: "Edge-TTS 在线合成",
        engine: "edge-tts",
        compute: "cloud",
        est_vram_mb: 0,
        disk_size_mb: 0,
        description: "微软神经网络音色，自然度高且完全不占本地显存与磁盘（需联网）。",
        tags: &["默认推荐", "0本地占用"],
        requires_python: false,
        requires_ollama: false,
    },
    BuiltinDef {
        id: "winrt-native-tts",
        category: "tts",
        name: "Windows 原生语音 (WinRT)",
        engine: "winrt",
        compute: "cpu",
        est_vram_mb: 0,
        disk_size_mb: 0,
        description: "调用系统内置语音库，0 体积纯离线兜底。",
        tags: &["离线兜底", "0体积"],
        requires_python: false,
        requires_ollama: false,
    },
    BuiltinDef {
        id: "kokoro-82m-zh",
        category: "tts",
        name: "Kokoro-82M (v1.1-zh)",
        engine: "onnx",
        compute: "cpu",
        est_vram_mb: 0,
        disk_size_mb: 180,
        description: "轻量纯离线开源神经网络发音，CPU 即可实时合成。",
        tags: &["纯离线", "轻量"],
        requires_python: false,
        requires_ollama: false,
    },
    BuiltinDef {
        id: "cosyvoice-gpu-worker",
        category: "tts",
        name: "CosyVoice / Index-TTS (声音克隆)",
        engine: "python-worker",
        compute: "gpu",
        est_vram_mb: 2800,
        disk_size_mb: 3500,
        description: "旗舰级中文拟人韵律与零样本声音克隆，按需运行在 5070 显存。",
        tags: &["拟真克隆", "需Python"],
        requires_python: true,
        requires_ollama: false,
    },
];

pub fn resolve_model_gguf_path(
    conn: &Connection,
    storage_dir: &Path,
    model_id: &str,
) -> Option<PathBuf> {
    let db_path: Option<String> = conn
        .query_row(
            "SELECT local_path FROM model_configs WHERE category = 'llm' AND model_id = ?1",
            params![model_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    if let Some(p) = db_path {
        let candidate = PathBuf::from(&p);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    if let Some(spec) = find_gguf_spec(model_id) {
        let default_path = default_gguf_path(storage_dir, spec.filename);
        if default_path.exists() {
            return Some(default_path);
        }
    }

    None
}

pub fn list_catalog_with_state(conn: &Connection, storage_dir: &Path) -> Result<Vec<ModelCatalogItem>> {
    let mut stmt = conn.prepare(
        "SELECT category, model_id, is_active, is_installed, local_path FROM model_configs",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)? != 0,
            row.get::<_, i64>(3)? != 0,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;

    let mut state_map: HashMap<(String, String), (bool, bool, Option<String>)> = HashMap::new();
    for r in rows {
        let (cat, id, active, installed, local_path) = r?;
        state_map.insert((cat, id), (active, installed, local_path));
    }

    let items = CATALOG
        .iter()
        .map(|def| {
            let (is_active, db_installed, db_path) = state_map
                .get(&(def.category.to_string(), def.id.to_string()))
                .cloned()
                .unwrap_or((false, false, None));

            let spec = find_gguf_spec(def.id);
            let voice_spec = find_voice_asset_spec(def.id);
            let downloadable = spec.is_some() || voice_spec.is_some();

            let (is_installed, local_path) = if def.engine == "llama-server" {
                let valid_db = db_path.as_ref().filter(|p| Path::new(p).exists()).cloned();
                let disk_default = spec
                    .map(|s| default_gguf_path(storage_dir, s.filename))
                    .filter(|p| p.exists())
                    .map(|p| p.to_string_lossy().to_string());

                let resolved = valid_db.or(disk_default);
                (resolved.is_some(), resolved)
            } else if def.engine == "sherpa-onnx" {
                let installed = is_voice_model_installed(storage_dir, def.id);
                let resolved_path = if installed {
                    if def.category == "kws" {
                        kws_model_dir(storage_dir).map(|p| p.to_string_lossy().to_string())
                    } else {
                        find_asr_paths(storage_dir, def.id)
                            .and_then(|p| p.model_onnx.parent().map(|d| d.to_string_lossy().to_string()))
                    }
                } else {
                    None
                };
                (installed, resolved_path)
            } else {
                (db_installed, db_path)
            };

            ModelCatalogItem {
                id: def.id.to_string(),
                category: def.category.to_string(),
                name: def.name.to_string(),
                engine: def.engine.to_string(),
                compute: def.compute.to_string(),
                est_vram_mb: def.est_vram_mb,
                disk_size_mb: def.disk_size_mb,
                description: def.description.to_string(),
                tags: def.tags.iter().map(|t| (*t).to_string()).collect(),
                requires_python: def.requires_python,
                requires_ollama: def.requires_ollama,
                is_active,
                is_installed,
                local_path,
                downloadable,
            }
        })
        .collect();

    Ok(items)
}

pub fn set_active_model(conn: &Connection, category: &str, model_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE model_configs SET is_active = 0 WHERE category = ?1",
        params![category],
    )?;
    conn.execute(
        "INSERT INTO model_configs (category, model_id, is_active, is_installed)
         VALUES (?1, ?2, 1, 0)
         ON CONFLICT(category, model_id) DO UPDATE SET is_active = 1",
        params![category, model_id],
    )?;
    Ok(())
}

pub fn set_model_local_path(
    conn: &Connection,
    category: &str,
    model_id: &str,
    local_path: Option<&str>,
    is_installed: bool,
) -> Result<()> {
    let installed_int = if is_installed { 1 } else { 0 };
    conn.execute(
        "INSERT INTO model_configs (category, model_id, is_active, is_installed, local_path)
         VALUES (?1, ?2, 0, ?3, ?4)
         ON CONFLICT(category, model_id) DO UPDATE SET
            is_installed = excluded.is_installed,
            local_path = excluded.local_path",
        params![category, model_id, installed_int, local_path],
    )?;
    Ok(())
}
