use crate::llm::downloader::{download_file_with_progress, emit_download_progress, DownloadProgressEvent};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use tauri::AppHandle;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub const SHERPA_VERSION: &str = "v1.13.8";
pub const SHERPA_RUNTIME_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.8/sherpa-onnx-v1.13.8-win-x64-shared-MD-Release.tar.bz2";
pub const SILERO_VAD_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad_v5.onnx";

#[derive(Debug, Clone, Copy)]
pub struct VoiceAssetSpec {
    pub model_id: &'static str,
    pub folder_name: &'static str,
    pub github_url: &'static str,
    pub archive_name: &'static str,
}

const VOICE_ASSETS: &[VoiceAssetSpec] = &[
    VoiceAssetSpec {
        model_id: "sherpa-kws-zh-en",
        folder_name: "kws-zh-en",
        github_url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/sherpa-onnx-kws-zipformer-zh-en-3M-2025-12-20.tar.bz2",
        archive_name: "sherpa-onnx-kws-zipformer-zh-en-3M-2025-12-20.tar.bz2",
    },
    VoiceAssetSpec {
        model_id: "sensevoice-small-int8",
        folder_name: "sensevoice-int8",
        github_url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2",
        archive_name: "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2",
    },
    VoiceAssetSpec {
        model_id: "paraformer-zh-streaming",
        folder_name: "paraformer-zh-int8",
        github_url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-paraformer-zh-int8-2025-10-07.tar.bz2",
        archive_name: "sherpa-onnx-paraformer-zh-int8-2025-10-07.tar.bz2",
    },
];

pub fn find_voice_asset_spec(model_id: &str) -> Option<&'static VoiceAssetSpec> {
    VOICE_ASSETS.iter().find(|v| v.model_id == model_id)
}

pub fn sherpa_runtime_dir(storage_dir: &Path) -> PathBuf {
    storage_dir.join("runtime").join("sherpa-onnx")
}

pub fn voice_models_root(storage_dir: &Path) -> PathBuf {
    storage_dir.join("models").join("voice")
}

pub fn vad_model_path(storage_dir: &Path) -> PathBuf {
    voice_models_root(storage_dir)
        .join("vad")
        .join("silero_vad_v5.onnx")
}

pub fn find_sherpa_c_api_dll(storage_dir: &Path) -> Option<PathBuf> {
    find_file_recursive(&sherpa_runtime_dir(storage_dir), 3, |name| {
        name.eq_ignore_ascii_case("sherpa-onnx-c-api.dll")
    })
}

pub struct KwsModelPaths {
    pub root: PathBuf,
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
    pub tokens: PathBuf,
    pub keywords_file: PathBuf,
}

pub fn find_kws_paths(storage_dir: &Path, wake_word: &str) -> Option<KwsModelPaths> {
    let dir = voice_models_root(storage_dir).join("kws-zh-en");
    if !dir.exists() {
        return None;
    }

    let encoder = find_file_recursive(&dir, 2, |n| n.starts_with("encoder") && n.ends_with(".onnx"))?;
    let decoder = find_file_recursive(&dir, 2, |n| n.starts_with("decoder") && n.ends_with(".onnx"))?;
    let joiner = find_file_recursive(&dir, 2, |n| n.starts_with("joiner") && n.ends_with(".onnx"))?;
    let tokens = find_file_recursive(&dir, 2, |n| n == "tokens.txt")?;
    let base_kw = find_file_recursive(&dir, 2, |n| n == "keywords.txt");
    let keywords_file = prepare_keywords_file(&dir, &tokens, base_kw.as_deref(), wake_word);

    Some(KwsModelPaths {
        root: dir,
        encoder,
        decoder,
        joiner,
        tokens,
        keywords_file,
    })
}

pub struct AsrModelPaths {
    pub model_id: String,
    pub model_onnx: PathBuf,
    pub tokens: PathBuf,
}

pub fn find_asr_paths(storage_dir: &Path, model_id: &str) -> Option<AsrModelPaths> {
    let folder = match model_id {
        "paraformer-zh-streaming" => "paraformer-zh-int8",
        _ => "sensevoice-int8",
    };
    let dir = voice_models_root(storage_dir).join(folder);
    if !dir.exists() {
        return None;
    }
    let model_onnx = find_file_recursive(&dir, 2, |n| n == "model.int8.onnx")
        .or_else(|| find_file_recursive(&dir, 2, |n| n.ends_with(".onnx")))?;
    let tokens = find_file_recursive(&dir, 2, |n| n == "tokens.txt")?;
    Some(AsrModelPaths {
        model_id: model_id.to_string(),
        model_onnx,
        tokens,
    })
}

pub fn is_voice_model_installed(storage_dir: &Path, model_id: &str) -> bool {
    match model_id {
        "sherpa-kws-zh-en" => find_kws_paths(storage_dir, "Skippy").is_some(),
        "sensevoice-small-int8" => find_asr_paths(storage_dir, "sensevoice-small-int8").is_some(),
        "paraformer-zh-streaming" => find_asr_paths(storage_dir, "paraformer-zh-streaming").is_some(),
        _ => false,
    }
}

fn prepare_keywords_file(
    model_dir: &Path,
    tokens_path: &Path,
    bundled_kw: Option<&Path>,
    wake_word: &str,
) -> PathBuf {
    let custom_path = model_dir.join("skippy_keywords.txt");
    let token_set: HashSet<String> = fs::read_to_string(tokens_path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .map(|s| s.to_string())
        .collect();

    let mut lines: Vec<String> = Vec::new();
    let trimmed = wake_word.trim();
    if !trimmed.is_empty() {
        if let Some(tokenized) = tokenize_english_word_for_bpe(trimmed, &token_set) {
            lines.push(format!("{tokenized} @{trimmed}"));
        }
    }

    if let Some(base) = bundled_kw {
        if let Ok(raw) = fs::read_to_string(base) {
            for l in raw.lines() {
                let t = l.trim();
                if !t.is_empty() && lines.len() < 8 {
                    lines.push(t.to_string());
                }
            }
        }
    }

    if let Some(first_fallback) = lines.first().cloned() {
        let _ = &first_fallback;
        let _ = fs::write(&custom_path, lines.join("\n"));
        return custom_path;
    }

    bundled_kw
        .map(|p| p.to_path_buf())
        .unwrap_or(custom_path)
}

fn tokenize_english_word_for_bpe(word: &str, token_set: &HashSet<String>) -> Option<String> {
    let cleaned: String = word
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_uppercase();
    if cleaned.is_empty() {
        return None;
    }

    // BPE 词首常带有 U+2581 (▁) 前缀，逐段进行最大正向匹配
    let mut remaining = format!("\u{2581}{cleaned}");
    let mut pieces: Vec<String> = Vec::new();
    while !remaining.is_empty() {
        let chars: Vec<char> = remaining.chars().collect();
        let mut matched: Option<String> = None;
        for len in (1..=chars.len()).rev() {
            let candidate: String = chars[..len].iter().collect();
            if token_set.contains(&candidate) {
                matched = Some(candidate);
                remaining = chars[len..].iter().collect();
                break;
            }
        }
        if let Some(m) = matched {
            pieces.push(m);
        } else {
            return None;
        }
    }

    Some(pieces.join(" "))
}

fn find_file_recursive(dir: &Path, max_depth: usize, pred: impl Fn(&str) -> bool + Copy) -> Option<PathBuf> {
    if !dir.exists() {
        return None;
    }
    let entries = fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                if pred(name) {
                    return Some(p);
                }
            }
        } else if p.is_dir() && max_depth > 0 {
            subdirs.push(p);
        }
    }
    for sub in subdirs {
        if let Some(found) = find_file_recursive(&sub, max_depth - 1, pred) {
            return Some(found);
        }
    }
    None
}

fn rewrite_github_url(url: &str, mirror_prefix: &str) -> String {
    let trimmed = mirror_prefix.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "direct" {
        return url.to_string();
    }
    format!("{trimmed}/{url}")
}

fn extract_tar_archive(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dest_dir).map_err(|e| format!("创建解压目录失败: {e}"))?;
    let mut cmd = Command::new("tar.exe");
    cmd.arg("-xf").arg(archive_path).arg("-C").arg(dest_dir);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let out = cmd
        .output()
        .map_err(|e| format!("调用 tar.exe 解压失败: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("解压 {} 失败: {stderr}", archive_path.display()));
    }
    Ok(())
}

pub async fn install_sherpa_runtime(
    app: &AppHandle,
    storage_dir: &Path,
    mirror_prefix: &str,
    cancel_flag: &AtomicBool,
) -> Result<PathBuf, String> {
    let target_id = "sherpa-runtime";
    let runtime_dir = sherpa_runtime_dir(storage_dir);
    fs::create_dir_all(&runtime_dir).map_err(|e| format!("创建 Sherpa 运行目录失败: {e}"))?;

    let archive_path = runtime_dir.join("_sherpa_runtime.tar.bz2");
    let url = rewrite_github_url(SHERPA_RUNTIME_URL, mirror_prefix);
    download_file_with_progress(
        app,
        target_id,
        &format!("下载 Sherpa-ONNX ({SHERPA_VERSION}) 运行库"),
        &url,
        &archive_path,
        cancel_flag,
    )
    .await?;

    emit_download_progress(
        app,
        DownloadProgressEvent {
            target_id: target_id.to_string(),
            stage: "extracting".to_string(),
            downloaded_bytes: 0,
            total_bytes: 0,
            percent: 99,
            message: "正在解压 Sherpa-ONNX DLL...".to_string(),
            done: false,
            error: None,
        },
    );

    extract_tar_archive(&archive_path, &runtime_dir)?;
    let _ = fs::remove_file(&archive_path);

    let dll = find_sherpa_c_api_dll(storage_dir)
        .ok_or_else(|| "解压后未找到 sherpa-onnx-c-api.dll".to_string())?;

    emit_download_progress(
        app,
        DownloadProgressEvent {
            target_id: target_id.to_string(),
            stage: "done".to_string(),
            downloaded_bytes: 1,
            total_bytes: 1,
            percent: 100,
            message: format!("Sherpa-ONNX ({SHERPA_VERSION}) 就绪"),
            done: true,
            error: None,
        },
    );

    Ok(dll)
}

pub async fn ensure_silero_vad(
    app: &AppHandle,
    storage_dir: &Path,
    mirror_prefix: &str,
    cancel_flag: &AtomicBool,
) -> Result<PathBuf, String> {
    let path = vad_model_path(storage_dir);
    if path.exists() {
        return Ok(path);
    }
    let url = rewrite_github_url(SILERO_VAD_URL, mirror_prefix);
    download_file_with_progress(
        app,
        "silero-vad",
        "下载 Silero VAD 端点检测模型",
        &url,
        &path,
        cancel_flag,
    )
    .await?;
    Ok(path)
}

pub async fn download_voice_model(
    app: &AppHandle,
    storage_dir: &Path,
    model_id: &str,
    mirror_prefix: &str,
    cancel_flag: &AtomicBool,
) -> Result<(), String> {
    let spec = find_voice_asset_spec(model_id)
        .ok_or_else(|| format!("语音模型 {model_id} 暂未配置内置直连下载源"))?;

    let dest_dir = voice_models_root(storage_dir).join(spec.folder_name);
    fs::create_dir_all(&dest_dir).map_err(|e| format!("创建语音模型目录失败: {e}"))?;

    let archive_path = dest_dir.join(spec.archive_name);
    let url = rewrite_github_url(spec.github_url, mirror_prefix);
    download_file_with_progress(
        app,
        model_id,
        &format!("下载 {}", spec.archive_name),
        &url,
        &archive_path,
        cancel_flag,
    )
    .await?;

    emit_download_progress(
        app,
        DownloadProgressEvent {
            target_id: model_id.to_string(),
            stage: "extracting".to_string(),
            downloaded_bytes: 0,
            total_bytes: 0,
            percent: 99,
            message: "正在解压语音模型...".to_string(),
            done: false,
            error: None,
        },
    );

    extract_tar_archive(&archive_path, &dest_dir)?;
    let _ = fs::remove_file(&archive_path);

    if find_sherpa_c_api_dll(storage_dir).is_none() {
        install_sherpa_runtime(app, storage_dir, mirror_prefix, cancel_flag).await?;
    }
    if !vad_model_path(storage_dir).exists() {
        let _ = ensure_silero_vad(app, storage_dir, mirror_prefix, cancel_flag).await;
    }

    emit_download_progress(
        app,
        DownloadProgressEvent {
            target_id: model_id.to_string(),
            stage: "done".to_string(),
            downloaded_bytes: 1,
            total_bytes: 1,
            percent: 100,
            message: format!("{} 安装完成", spec.model_id),
            done: true,
            error: None,
        },
    );

    Ok(())
}
