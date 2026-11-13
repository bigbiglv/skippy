use crate::llm::runner::execute_chat_turn;
use crate::models::registry::list_catalog_with_state;
use crate::state::{AppState, VoiceStage};
use crate::storage::chat_repo::{create_session, get_all_settings, list_sessions, resolve_local_storage_dir};
use crate::voice::assets::{
    find_asr_paths, find_kws_paths, find_sherpa_c_api_dll, vad_model_path,
};
use crate::voice::sherpa::{AsrKind, SherpaKws, SherpaOfflineAsr, SherpaRuntime, SherpaVad};
use crate::voice::tts::TtsPlayer;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use serde::Serialize;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct VoiceStatusEvent {
    pub stage: VoiceStage,
    pub stage_label: String,
    pub recording: bool,
    pub kws_active: bool,
    pub transcript: Option<String>,
    pub error: Option<String>,
}

enum VoiceCmd {
    ReloadConfig,
    StartManual { session_id: Option<String> },
    StopManual,
    StopSpeaking,
    Shutdown,
}

#[derive(Clone)]
pub struct VoiceController {
    tx: Sender<VoiceCmd>,
}

impl VoiceController {
    pub fn spawn(app: AppHandle, tts: TtsPlayer) -> Self {
        let (tx, rx) = mpsc::channel::<VoiceCmd>();
        thread::spawn(move || voice_supervisor_loop(app, tts, rx));
        Self { tx }
    }

    pub fn reload_config(&self) {
        let _ = self.tx.send(VoiceCmd::ReloadConfig);
    }

    pub fn start_manual(&self, session_id: Option<String>) {
        let _ = self.tx.send(VoiceCmd::StartManual { session_id });
    }

    pub fn stop_manual(&self) {
        let _ = self.tx.send(VoiceCmd::StopManual);
    }

    pub fn barge_in_stop(&self) {
        let _ = self.tx.send(VoiceCmd::StopSpeaking);
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(VoiceCmd::Shutdown);
    }
}

struct ActiveMicStream {
    _stream: Stream,
}

struct RuntimeBundle {
    storage_dir: PathBuf,
    kws_id: String,
    asr_id: String,
    wake_word: String,
}

fn read_runtime_bundle(app: &AppHandle) -> Option<RuntimeBundle> {
    let state = app.state::<AppState>();
    let db = state.db.lock().ok()?;
    let storage_dir = resolve_local_storage_dir(&db, &state.data_dir);
    let settings = get_all_settings(&db).ok()?;
    let models = list_catalog_with_state(&db, &storage_dir).ok()?;

    let kws_id = models
        .iter()
        .find(|m| m.category == "kws" && m.is_active)
        .map(|m| m.id.clone())
        .unwrap_or_else(|| "shortcut-only".to_string());
    let asr_id = models
        .iter()
        .find(|m| m.category == "asr" && m.is_active)
        .map(|m| m.id.clone())
        .unwrap_or_else(|| "sensevoice-small-int8".to_string());
    let wake_word = settings
        .get("wake_word")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "密探阿泰".to_string());

    Some(RuntimeBundle {
        storage_dir,
        kws_id,
        asr_id,
        wake_word,
    })
}

fn update_voice_status(
    app: &AppHandle,
    stage: VoiceStage,
    label: &str,
    recording: bool,
    kws_active: bool,
    transcript: Option<String>,
    error: Option<String>,
) {
    let state = app.state::<AppState>();
    if let Ok(mut st) = state.status.lock() {
        st.stage = stage.clone();
        st.stage_label = label.to_string();
    }
    let _ = app.emit(
        "voice-status",
        VoiceStatusEvent {
            stage,
            stage_label: label.to_string(),
            recording,
            kws_active,
            transcript,
            error,
        },
    );
}

fn idle_stage_for_kws(kws_active: bool) -> (VoiceStage, &'static str) {
    if kws_active {
        (VoiceStage::IdleListening, "语音待机监听中（说唤醒词即可对话）")
    } else {
        (VoiceStage::ShortcutOnly, "待机低占用（快捷键唤醒）")
    }
}

fn voice_supervisor_loop(app: AppHandle, tts: TtsPlayer, rx: Receiver<VoiceCmd>) {
    let mut mic_handle: Option<(ActiveMicStream, Receiver<Vec<f32>>)> = None;
    let mut sherpa_rt: Option<(PathBuf, SherpaRuntime)> = None;

    let mut recording = false;
    let mut manual_stop_requested = false;
    let mut manual_session_id: Option<String> = None;
    let mut kws_wanted = false;
    let mut pre_roll: VecDeque<f32> = VecDeque::with_capacity(6400);
    let mut recorded_samples: Vec<f32> = Vec::new();
    let mut speech_seen = false;
    let mut rec_started_at = Instant::now();
    let mut last_speech_at = Instant::now();
    let mut vad_holder: Option<SherpaVad<'static>> = None;
    let mut kws_holder: Option<SherpaKws<'static>> = None;

    // 初始加载一次配置
    let mut config_dirty = true;

    loop {
        if config_dirty {
            config_dirty = false;
            kws_holder.take();
            vad_holder.take();

            if let Some(bundle) = read_runtime_bundle(&app) {
                kws_wanted = bundle.kws_id != "shortcut-only";
                if kws_wanted {
                    if let (Some(dll), Some(kws_paths)) = (
                        find_sherpa_c_api_dll(&bundle.storage_dir),
                        find_kws_paths(&bundle.storage_dir, &bundle.wake_word),
                    ) {
                        if sherpa_rt.as_ref().is_none_or(|(p, _)| p != &dll) {
                            sherpa_rt = SherpaRuntime::load(&dll).ok().map(|rt| (dll.clone(), rt));
                        }
                        if let Some((_, ref rt)) = sherpa_rt {
                            // 安全说明：SherpaRuntime 存活期间 kws_holder/vad_holder 会在重新配置或循环退出前先行 drop
                            let rt_static: &'static SherpaRuntime =
                                unsafe { &*(rt as *const SherpaRuntime) };
                            match SherpaKws::new(
                                rt_static,
                                &kws_paths.encoder,
                                &kws_paths.decoder,
                                &kws_paths.joiner,
                                &kws_paths.tokens,
                                &kws_paths.keywords_file,
                            ) {
                                Ok(kws) => {
                                    kws_holder = Some(kws);
                                    let vad_path = vad_model_path(&bundle.storage_dir);
                                    if vad_path.exists() {
                                        vad_holder = SherpaVad::new(rt_static, &vad_path).ok();
                                    }
                                    if mic_handle.is_none() {
                                        match start_mic_capture() {
                                            Ok(m) => mic_handle = Some(m),
                                            Err(err) => {
                                                kws_holder.take();
                                                update_voice_status(
                                                    &app,
                                                    VoiceStage::ShortcutOnly,
                                                    "待机低占用（麦克风打开失败）",
                                                    false,
                                                    false,
                                                    None,
                                                    Some(err),
                                                );
                                                continue;
                                            }
                                        }
                                    }
                                    update_voice_status(
                                        &app,
                                        VoiceStage::IdleListening,
                                        "语音待机监听中",
                                        false,
                                        true,
                                        None,
                                        None,
                                    );
                                }
                                Err(err) => {
                                    if !recording {
                                        mic_handle = None;
                                    }
                                    update_voice_status(
                                        &app,
                                        VoiceStage::ShortcutOnly,
                                        "待机低占用（唤醒初始化失败）",
                                        false,
                                        false,
                                        None,
                                        Some(err),
                                    );
                                }
                            }
                        }
                    } else {
                        if !recording {
                            mic_handle = None;
                        }
                        update_voice_status(
                            &app,
                            VoiceStage::ShortcutOnly,
                            "待机低占用（待下载 KWS 语音模型）",
                            false,
                            false,
                            None,
                            None,
                        );
                    }
                } else {
                    if !recording {
                        mic_handle = None;
                    }
                    update_voice_status(
                        &app,
                        VoiceStage::ShortcutOnly,
                        "待机低占用（快捷键唤醒）",
                        false,
                        false,
                        None,
                        None,
                    );
                }
            }
        }

        // 处理控制指令
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                VoiceCmd::Shutdown => {
                    kws_holder.take();
                    vad_holder.take();
                    drop(mic_handle);
                    tts.shutdown();
                    return;
                }
                VoiceCmd::ReloadConfig => {
                    config_dirty = true;
                }
                VoiceCmd::StopSpeaking => {
                    tts.stop_speaking();
                    let state = app.state::<AppState>();
                    state.chat_cancel.store(true, Ordering::Relaxed);
                    let (st, lb) = idle_stage_for_kws(kws_holder.is_some());
                    update_voice_status(&app, st, lb, false, kws_holder.is_some(), None, None);
                }
                VoiceCmd::StopManual => {
                    if recording {
                        manual_stop_requested = true;
                    }
                }
                VoiceCmd::StartManual { session_id } => {
                    tts.stop_speaking();
                    app.state::<AppState>()
                        .chat_cancel
                        .store(true, Ordering::Relaxed);
                    manual_session_id = session_id;

                    let Some(bundle) = read_runtime_bundle(&app) else {
                        continue;
                    };
                    let Some(dll) = find_sherpa_c_api_dll(&bundle.storage_dir) else {
                        update_voice_status(
                            &app,
                            VoiceStage::ShortcutOnly,
                            "待机低占用（未安装 Sherpa 语音运行库）",
                            false,
                            kws_holder.is_some(),
                            None,
                            Some("请先在「模型管理 -> 语音识别」下载 SenseVoice 或一键安装语音套件".to_string()),
                        );
                        continue;
                    };
                    if find_asr_paths(&bundle.storage_dir, &bundle.asr_id).is_none() {
                        update_voice_status(
                            &app,
                            VoiceStage::ShortcutOnly,
                            "待机低占用（未下载离线识别模型）",
                            false,
                            kws_holder.is_some(),
                            None,
                            Some("请先在「模型管理 -> 语音识别」下载离线 ASR 模型（如 SenseVoice-Small）".to_string()),
                        );
                        continue;
                    }

                    if sherpa_rt.as_ref().is_none_or(|(p, _)| p != &dll) {
                        sherpa_rt = SherpaRuntime::load(&dll).ok().map(|rt| (dll.clone(), rt));
                    }
                    if vad_holder.is_none() {
                        if let Some((_, ref rt)) = sherpa_rt {
                            let vad_path = vad_model_path(&bundle.storage_dir);
                            if vad_path.exists() {
                                let rt_static: &'static SherpaRuntime =
                                    unsafe { &*(rt as *const SherpaRuntime) };
                                vad_holder = SherpaVad::new(rt_static, &vad_path).ok();
                            }
                        }
                    }

                    if mic_handle.is_none() {
                        match start_mic_capture() {
                            Ok(m) => mic_handle = Some(m),
                            Err(err) => {
                                update_voice_status(
                                    &app,
                                    VoiceStage::ShortcutOnly,
                                    "麦克风不可用",
                                    false,
                                    kws_holder.is_some(),
                                    None,
                                    Some(err),
                                );
                                continue;
                            }
                        }
                    }

                    tts.play_wake_chime();
                    if let Some(ref v) = vad_holder {
                        v.clear();
                    }
                    recording = true;
                    manual_stop_requested = false;
                    speech_seen = false;
                    rec_started_at = Instant::now();
                    last_speech_at = Instant::now();
                    recorded_samples.clear();
                    recorded_samples.extend(pre_roll.iter().copied());
                    update_voice_status(
                        &app,
                        VoiceStage::Recording,
                        "正在聆听，请说话...",
                        true,
                        kws_holder.is_some(),
                        None,
                        None,
                    );
                }
            }
        }

        // 若当前没有在听唤醒词也没在录音，短暂休眠即可（0 CPU）
        let Some((_, ref audio_rx)) = mic_handle else {
            thread::sleep(Duration::from_millis(60));
            continue;
        };

        let mut drained = false;
        while let Ok(chunk) = audio_rx.try_recv() {
            drained = true;
            if chunk.is_empty() {
                continue;
            }

            // 更新 0.35s 前置环形缓冲
            for &s in &chunk {
                if pre_roll.len() >= 5600 {
                    pre_roll.pop_front();
                }
                pre_roll.push_back(s);
            }

            if !recording {
                if let Some(ref kws) = kws_holder {
                    if let Some(hit_kw) = kws.feed_and_detect(&chunk) {
                        tts.stop_speaking();
                        app.state::<AppState>()
                            .chat_cancel
                            .store(true, Ordering::Relaxed);
                        tts.play_wake_chime();
                        if let Some(ref v) = vad_holder {
                            v.clear();
                        }
                        manual_session_id = None;
                        recording = true;
                        manual_stop_requested = false;
                        speech_seen = false;
                        rec_started_at = Instant::now();
                        last_speech_at = Instant::now();
                        recorded_samples.clear();
                        recorded_samples.extend(pre_roll.iter().copied());
                        update_voice_status(
                            &app,
                            VoiceStage::Recording,
                            &format!("已唤醒（{hit_kw}），请说话..."),
                            true,
                            true,
                            None,
                            None,
                        );
                    }
                }
            } else {
                recorded_samples.extend_from_slice(&chunk);

                let has_speech = if let Some(ref vad) = vad_holder {
                    vad.accept_waveform(&chunk);
                    while vad.pop_next_segment().is_some() {}
                    vad.is_speech_detected()
                } else {
                    chunk_rms(&chunk) > 0.015
                };

                if has_speech {
                    speech_seen = true;
                    last_speech_at = Instant::now();
                }

                let total_elapsed = rec_started_at.elapsed();
                let silence_elapsed = last_speech_at.elapsed();

                let should_finish = manual_stop_requested
                    || (speech_seen && silence_elapsed >= Duration::from_millis(950) && total_elapsed >= Duration::from_millis(450))
                    || (!speech_seen && total_elapsed >= Duration::from_secs(5))
                    || total_elapsed >= Duration::from_secs(16);

                if should_finish {
                    recording = false;
                    manual_stop_requested = false;
                    tts.play_stop_chime();

                    let samples = std::mem::take(&mut recorded_samples);
                    if !kws_wanted {
                        mic_handle = None;
                    }

                    if samples.len() < 3200 || !speech_seen {
                        let (st, lb) = idle_stage_for_kws(kws_holder.is_some());
                        update_voice_status(&app, st, lb, false, kws_holder.is_some(), None, None);
                        break;
                    }

                    update_voice_status(
                        &app,
                        VoiceStage::Thinking,
                        "正在离线识别语音...",
                        false,
                        kws_holder.is_some(),
                        None,
                        None,
                    );

                    if let (Some(bundle), Some((_, rt))) =
                        (read_runtime_bundle(&app), sherpa_rt.as_ref())
                    {
                        if let Some(asr_paths) = find_asr_paths(&bundle.storage_dir, &bundle.asr_id)
                        {
                            let kind = if asr_paths.model_id == "paraformer-zh-streaming" {
                                AsrKind::Paraformer
                            } else {
                                AsrKind::SenseVoice
                            };
                            match SherpaOfflineAsr::new(
                                rt,
                                kind,
                                &asr_paths.model_onnx,
                                &asr_paths.tokens,
                            ) {
                                Ok(asr) => match asr.transcribe(&samples) {
                                    Ok(raw_text) => {
                                        let text = clean_asr_output(&raw_text);
                                        if text.is_empty() {
                                            let (st, lb) = idle_stage_for_kws(kws_holder.is_some());
                                            update_voice_status(
                                                &app,
                                                st,
                                                lb,
                                                false,
                                                kws_holder.is_some(),
                                                None,
                                                None,
                                            );
                                        } else {
                                            dispatch_voice_prompt(
                                                app.clone(),
                                                text,
                                                manual_session_id.take(),
                                                kws_holder.is_some(),
                                            );
                                        }
                                    }
                                    Err(err) => {
                                        let (st, lb) = idle_stage_for_kws(kws_holder.is_some());
                                        update_voice_status(
                                            &app,
                                            st,
                                            lb,
                                            false,
                                            kws_holder.is_some(),
                                            None,
                                            Some(err),
                                        );
                                    }
                                },
                                Err(err) => {
                                    let (st, lb) = idle_stage_for_kws(kws_holder.is_some());
                                    update_voice_status(
                                        &app,
                                        st,
                                        lb,
                                        false,
                                        kws_holder.is_some(),
                                        None,
                                        Some(err),
                                    );
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }

        if !drained {
            thread::sleep(Duration::from_millis(16));
        }
    }
}

fn dispatch_voice_prompt(
    app: AppHandle,
    transcript: String,
    preferred_session_id: Option<String>,
    kws_active: bool,
) {
    let session_id = {
        let state = app.state::<AppState>();
        let Ok(db) = state.db.lock() else {
            return;
        };
        if let Some(sid) = preferred_session_id {
            sid
        } else {
            let sessions = list_sessions(&db).unwrap_or_default();
            if let Some(first) = sessions.into_iter().next() {
                first.id
            } else if let Ok(created) = create_session(&db, "语音对话") {
                created.id
            } else {
                return;
            }
        }
    };

    update_voice_status(
        &app,
        VoiceStage::Thinking,
        &format!("思考中：「{transcript}」"),
        false,
        kws_active,
        Some(transcript.clone()),
        None,
    );

    tauri::async_runtime::spawn(async move {
        let _ = execute_chat_turn(&app, &session_id, &transcript, "pc_voice", true).await;
        let (st, lb) = idle_stage_for_kws(kws_active);
        update_voice_status(&app, st, lb, false, kws_active, None, None);
    });
}

fn clean_asr_output(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(s) = rest.find("<|") {
        out.push_str(&rest[..s]);
        if let Some(e) = rest[s..].find("|>") {
            rest = &rest[s + e + 2..];
        } else {
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

fn chunk_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

fn start_mic_capture() -> Result<(ActiveMicStream, Receiver<Vec<f32>>), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "未检测到可用的系统麦克风设备".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("读取麦克风默认配置失败: {e}"))?;

    let sample_format = supported.sample_format();
    let dev_rate = supported.sample_rate().0;
    let channels = supported.channels().max(1) as usize;
    let config: StreamConfig = supported.into();

    let (tx, rx): (SyncSender<Vec<f32>>, Receiver<Vec<f32>>) = mpsc::sync_channel(128);
    let phase = Arc::new(std::sync::Mutex::new(0.0f64));

    let err_fn = |err| {
        eprintln!("Mic capture stream error: {err}");
    };

    let stream = match sample_format {
        SampleFormat::F32 => {
            let phase_c = Arc::clone(&phase);
            device.build_input_stream(
                &config,
                move |data: &[f32], _| {
                    let mono = to_mono_f32(data, channels);
                    let resampled = resample_to_16k(&mono, dev_rate, &phase_c);
                    if !resampled.is_empty() {
                        let _ = tx.try_send(resampled);
                    }
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let phase_c = Arc::clone(&phase);
            device.build_input_stream(
                &config,
                move |data: &[i16], _| {
                    let fdata: Vec<f32> = data.iter().map(|&v| v as f32 / i16::MAX as f32).collect();
                    let mono = to_mono_f32(&fdata, channels);
                    let resampled = resample_to_16k(&mono, dev_rate, &phase_c);
                    if !resampled.is_empty() {
                        let _ = tx.try_send(resampled);
                    }
                },
                err_fn,
                None,
            )
        }
        SampleFormat::U16 => {
            let phase_c = Arc::clone(&phase);
            device.build_input_stream(
                &config,
                move |data: &[u16], _| {
                    let fdata: Vec<f32> = data
                        .iter()
                        .map(|&v| (v as f32 - 32768.0) / 32768.0)
                        .collect();
                    let mono = to_mono_f32(&fdata, channels);
                    let resampled = resample_to_16k(&mono, dev_rate, &phase_c);
                    if !resampled.is_empty() {
                        let _ = tx.try_send(resampled);
                    }
                },
                err_fn,
                None,
            )
        }
        other => {
            return Err(format!("不支持的麦克风采样格式: {other:?}"));
        }
    }
    .map_err(|e| format!("打开麦克风输入流失败: {e}"))?;

    stream
        .play()
        .map_err(|e| format!("启动麦克风监听失败: {e}"))?;

    Ok((ActiveMicStream { _stream: stream }, rx))
}

fn to_mono_f32(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn resample_to_16k(input: &[f32], in_rate: u32, phase_lock: &std::sync::Mutex<f64>) -> Vec<f32> {
    if in_rate == 16000 || input.is_empty() {
        return input.to_vec();
    }
    let step = in_rate as f64 / 16000.0;
    let mut pos = phase_lock.lock().map(|g| *g).unwrap_or(0.0);
    let mut out = Vec::with_capacity(((input.len() as f64) / step) as usize + 2);

    while (pos as usize) + 1 < input.len() {
        let idx = pos as usize;
        let frac = (pos - idx as f64) as f32;
        let s = input[idx] * (1.0 - frac) + input[idx + 1] * frac;
        out.push(s);
        pos += step;
    }

    let next_phase = pos - input.len() as f64;
    if let Ok(mut g) = phase_lock.lock() {
        *g = next_phase.max(0.0);
    }
    out
}
