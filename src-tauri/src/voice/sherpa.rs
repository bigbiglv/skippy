use libloading::Library;
use std::ffi::{c_char, c_float, c_void, CStr, CString};
use std::path::Path;
use std::ptr;

#[repr(C)]
#[derive(Default)]
struct SherpaOnnxFeatureConfig {
    sample_rate: i32,
    feature_dim: i32,
}

#[repr(C)]
struct SherpaOnnxOnlineTransducerModelConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    joiner: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOnlineParaformerModelConfig {
    encoder: *const c_char,
    decoder: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOnlineZipformer2CtcModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOnlineNemoCtcModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOnlineToneCtcModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOnlineModelConfig {
    transducer: SherpaOnnxOnlineTransducerModelConfig,
    paraformer: SherpaOnnxOnlineParaformerModelConfig,
    zipformer2_ctc: SherpaOnnxOnlineZipformer2CtcModelConfig,
    tokens: *const c_char,
    num_threads: i32,
    provider: *const c_char,
    debug: i32,
    model_type: *const c_char,
    modeling_unit: *const c_char,
    bpe_vocab: *const c_char,
    tokens_buf: *const c_char,
    tokens_buf_size: i32,
    nemo_ctc: SherpaOnnxOnlineNemoCtcModelConfig,
    t_one_ctc: SherpaOnnxOnlineToneCtcModelConfig,
}

#[repr(C)]
struct SherpaOnnxKeywordSpotterConfig {
    feat_config: SherpaOnnxFeatureConfig,
    model_config: SherpaOnnxOnlineModelConfig,
    max_active_paths: i32,
    num_trailing_blanks: i32,
    keywords_score: f32,
    keywords_threshold: f32,
    keywords_file: *const c_char,
    keywords_buf: *const c_char,
    keywords_buf_size: i32,
}

#[repr(C)]
struct SherpaOnnxKeywordResult {
    keyword: *const c_char,
    tokens: *const c_char,
    tokens_arr: *const *const c_char,
    count: i32,
    timestamps: *mut f32,
    start_time: f32,
    json: *const c_char,
}

#[repr(C)]
struct SherpaOnnxSileroVadModelConfig {
    model: *const c_char,
    threshold: f32,
    min_silence_duration: f32,
    min_speech_duration: f32,
    window_size: i32,
    max_speech_duration: f32,
}

#[repr(C)]
struct SherpaOnnxTenVadModelConfig {
    model: *const c_char,
    threshold: f32,
    min_silence_duration: f32,
    min_speech_duration: f32,
    window_size: i32,
    max_speech_duration: f32,
}

#[repr(C)]
struct SherpaOnnxVadModelConfig {
    silero_vad: SherpaOnnxSileroVadModelConfig,
    sample_rate: i32,
    num_threads: i32,
    provider: *const c_char,
    debug: i32,
    ten_vad: SherpaOnnxTenVadModelConfig,
}

#[repr(C)]
struct SherpaOnnxSpeechSegment {
    start: i32,
    samples: *mut f32,
    n: i32,
}

#[repr(C)]
struct SherpaOnnxOfflineTransducerModelConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    joiner: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineParaformerModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineNemoEncDecCtcModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineWhisperModelConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    language: *const c_char,
    task: *const c_char,
    tail_paddings: i32,
    enable_token_timestamps: i32,
    enable_segment_timestamps: i32,
}

#[repr(C)]
struct SherpaOnnxOfflineTdnnModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineSenseVoiceModelConfig {
    model: *const c_char,
    language: *const c_char,
    use_itn: i32,
}

#[repr(C)]
struct SherpaOnnxOfflineMoonshineModelConfig {
    preprocessor: *const c_char,
    encoder: *const c_char,
    uncached_decoder: *const c_char,
    cached_decoder: *const c_char,
    merged_decoder: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineFireRedAsrModelConfig {
    encoder: *const c_char,
    decoder: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineDolphinModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineZipformerCtcModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineCanaryModelConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    src_lang: *const c_char,
    tgt_lang: *const c_char,
    use_pnc: i32,
}

#[repr(C)]
struct SherpaOnnxOfflineWenetCtcModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineOmnilingualAsrCtcModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineMedAsrCtcModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineFunASRNanoModelConfig {
    encoder_adaptor: *const c_char,
    llm: *const c_char,
    embedding: *const c_char,
    tokenizer: *const c_char,
    system_prompt: *const c_char,
    user_prompt: *const c_char,
    max_new_tokens: i32,
    temperature: f32,
    top_p: f32,
    seed: i32,
    language: *const c_char,
    itn: i32,
    hotwords: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineFireRedAsrCtcModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineQwen3ASRModelConfig {
    conv_frontend: *const c_char,
    encoder: *const c_char,
    decoder: *const c_char,
    tokenizer: *const c_char,
    max_total_len: i32,
    max_new_tokens: i32,
    temperature: f32,
    top_p: f32,
    seed: i32,
    hotwords: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineCohereTranscribeModelConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    language: *const c_char,
    use_punct: i32,
    use_itn: i32,
}

#[repr(C)]
struct SherpaOnnxOfflineModelConfig {
    transducer: SherpaOnnxOfflineTransducerModelConfig,
    paraformer: SherpaOnnxOfflineParaformerModelConfig,
    nemo_ctc: SherpaOnnxOfflineNemoEncDecCtcModelConfig,
    whisper: SherpaOnnxOfflineWhisperModelConfig,
    tdnn: SherpaOnnxOfflineTdnnModelConfig,
    tokens: *const c_char,
    num_threads: i32,
    debug: i32,
    provider: *const c_char,
    model_type: *const c_char,
    modeling_unit: *const c_char,
    bpe_vocab: *const c_char,
    telespeech_ctc: *const c_char,
    sense_voice: SherpaOnnxOfflineSenseVoiceModelConfig,
    moonshine: SherpaOnnxOfflineMoonshineModelConfig,
    fire_red_asr: SherpaOnnxOfflineFireRedAsrModelConfig,
    dolphin: SherpaOnnxOfflineDolphinModelConfig,
    zipformer_ctc: SherpaOnnxOfflineZipformerCtcModelConfig,
    canary: SherpaOnnxOfflineCanaryModelConfig,
    wenet_ctc: SherpaOnnxOfflineWenetCtcModelConfig,
    omnilingual: SherpaOnnxOfflineOmnilingualAsrCtcModelConfig,
    medasr: SherpaOnnxOfflineMedAsrCtcModelConfig,
    funasr_nano: SherpaOnnxOfflineFunASRNanoModelConfig,
    fire_red_asr_ctc: SherpaOnnxOfflineFireRedAsrCtcModelConfig,
    qwen3_asr: SherpaOnnxOfflineQwen3ASRModelConfig,
    cohere_transcribe: SherpaOnnxOfflineCohereTranscribeModelConfig,
}

#[repr(C)]
struct SherpaOnnxOfflineLMConfig {
    model: *const c_char,
    scale: f32,
}

#[repr(C)]
struct SherpaOnnxHomophoneReplacerConfig {
    dict_dir: *const c_char,
    lexicon: *const c_char,
    rule_fsts: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineRecognizerConfig {
    feat_config: SherpaOnnxFeatureConfig,
    model_config: SherpaOnnxOfflineModelConfig,
    lm_config: SherpaOnnxOfflineLMConfig,
    decoding_method: *const c_char,
    max_active_paths: i32,
    hotwords_file: *const c_char,
    hotwords_score: f32,
    rule_fsts: *const c_char,
    rule_fars: *const c_char,
    blank_penalty: f32,
    hr: SherpaOnnxHomophoneReplacerConfig,
}

#[repr(C)]
struct SherpaOnnxOfflineRecognizerResult {
    text: *const c_char,
    timestamps: *mut f32,
    count: i32,
    tokens: *const c_char,
    tokens_arr: *const *const c_char,
    json: *const c_char,
    lang: *const c_char,
    emotion: *const c_char,
    event: *const c_char,
    durations: *mut f32,
    ys_log_probs: *mut f32,
    segment_timestamps: *const f32,
    segment_durations: *const f32,
    segment_texts: *const c_char,
    segment_texts_arr: *const *const c_char,
    segment_count: i32,
}

pub struct SherpaRuntime {
    _preload: Vec<Library>,
    lib: Library,
}

unsafe impl Send for SherpaRuntime {}
unsafe impl Sync for SherpaRuntime {}

impl SherpaRuntime {
    pub fn load(c_api_dll: &Path) -> Result<Self, String> {
        let mut preload = Vec::new();
        if let Some(dir) = c_api_dll.parent() {
            for dep in ["onnxruntime_providers_shared.dll", "onnxruntime.dll"] {
                let p = dir.join(dep);
                if p.exists() {
                    if let Ok(lib) = unsafe { Library::new(&p) } {
                        preload.push(lib);
                    }
                }
            }
        }

        let lib = unsafe { Library::new(c_api_dll) }
            .map_err(|e| format!("加载 {} 失败: {e}", c_api_dll.display()))?;
        Ok(Self {
            _preload: preload,
            lib,
        })
    }
}

pub struct SherpaKws<'a> {
    lib: &'a Library,
    spotter: *const c_void,
    stream: *const c_void,
}

impl<'a> SherpaKws<'a> {
    pub fn new(
        runtime: &'a SherpaRuntime,
        encoder: &Path,
        decoder: &Path,
        joiner: &Path,
        tokens: &Path,
        keywords_file: &Path,
    ) -> Result<Self, String> {
        let c_enc = path_cstring(encoder)?;
        let c_dec = path_cstring(decoder)?;
        let c_joi = path_cstring(joiner)?;
        let c_tok = path_cstring(tokens)?;
        let c_kw_file = path_cstring(keywords_file)?;
        let c_prov = CString::new("cpu").unwrap();
        let c_unit = CString::new("cjkchar").unwrap();

        let mut cfg: SherpaOnnxKeywordSpotterConfig = unsafe { std::mem::zeroed() };
        cfg.feat_config.sample_rate = 16000;
        cfg.feat_config.feature_dim = 80;
        cfg.model_config.transducer.encoder = c_enc.as_ptr();
        cfg.model_config.transducer.decoder = c_dec.as_ptr();
        cfg.model_config.transducer.joiner = c_joi.as_ptr();
        cfg.model_config.tokens = c_tok.as_ptr();
        cfg.model_config.num_threads = 2;
        cfg.model_config.provider = c_prov.as_ptr();
        cfg.model_config.modeling_unit = c_unit.as_ptr();
        cfg.max_active_paths = 4;
        cfg.num_trailing_blanks = 1;
        cfg.keywords_score = 1.5;
        cfg.keywords_threshold = 0.18;
        cfg.keywords_file = c_kw_file.as_ptr();

        unsafe {
            let create: libloading::Symbol<
                unsafe extern "C" fn(*const SherpaOnnxKeywordSpotterConfig) -> *const c_void,
            > = runtime
                .lib
                .get(b"SherpaOnnxCreateKeywordSpotter\0")
                .map_err(|e| format!("读取 SherpaOnnxCreateKeywordSpotter 失败: {e}"))?;
            let create_stream: libloading::Symbol<
                unsafe extern "C" fn(*const c_void) -> *const c_void,
            > = runtime
                .lib
                .get(b"SherpaOnnxCreateKeywordStream\0")
                .map_err(|e| format!("读取 SherpaOnnxCreateKeywordStream 失败: {e}"))?;

            let spotter = create(&cfg);
            if spotter.is_null() {
                return Err("初始化 SherpaOnnxKeywordSpotter 失败（请检查 KWS 模型文件与 keywords.txt）".to_string());
            }
            let stream = create_stream(spotter);
            if stream.is_null() {
                let destroy: libloading::Symbol<unsafe extern "C" fn(*const c_void)> =
                    runtime.lib.get(b"SherpaOnnxDestroyKeywordSpotter\0").unwrap();
                destroy(spotter);
                return Err("创建 KWS 音频流失败".to_string());
            }
            Ok(Self {
                lib: &runtime.lib,
                spotter,
                stream,
            })
        }
    }

    pub fn feed_and_detect(&self, samples: &[f32]) -> Option<String> {
        unsafe {
            let accept: libloading::Symbol<
                unsafe extern "C" fn(*const c_void, i32, *const c_float, i32),
            > = self
                .lib
                .get(b"SherpaOnnxOnlineStreamAcceptWaveform\0")
                .ok()?;
            let is_ready: libloading::Symbol<
                unsafe extern "C" fn(*const c_void, *const c_void) -> i32,
            > = self.lib.get(b"SherpaOnnxIsKeywordStreamReady\0").ok()?;
            let decode: libloading::Symbol<unsafe extern "C" fn(*const c_void, *const c_void)> =
                self.lib.get(b"SherpaOnnxDecodeKeywordStream\0").ok()?;
            let get_res: libloading::Symbol<
                unsafe extern "C" fn(*const c_void, *const c_void) -> *const SherpaOnnxKeywordResult,
            > = self.lib.get(b"SherpaOnnxGetKeywordResult\0").ok()?;
            let destroy_res: libloading::Symbol<
                unsafe extern "C" fn(*const SherpaOnnxKeywordResult),
            > = self.lib.get(b"SherpaOnnxDestroyKeywordResult\0").ok()?;
            let reset: libloading::Symbol<unsafe extern "C" fn(*const c_void, *const c_void)> =
                self.lib.get(b"SherpaOnnxResetKeywordStream\0").ok()?;

            accept(self.stream, 16000, samples.as_ptr(), samples.len() as i32);
            while is_ready(self.spotter, self.stream) != 0 {
                decode(self.spotter, self.stream);
            }

            let res_ptr = get_res(self.spotter, self.stream);
            if res_ptr.is_null() {
                return None;
            }
            let mut hit = None;
            let kw_ptr = (*res_ptr).keyword;
            if !kw_ptr.is_null() {
                let s = CStr::from_ptr(kw_ptr).to_string_lossy().trim().to_string();
                if !s.is_empty() {
                    hit = Some(s);
                }
            }
            destroy_res(res_ptr);
            if hit.is_some() {
                reset(self.spotter, self.stream);
            }
            hit
        }
    }
}

impl Drop for SherpaKws<'_> {
    fn drop(&mut self) {
        unsafe {
            if let Ok(f) = self
                .lib
                .get::<unsafe extern "C" fn(*const c_void)>(b"SherpaOnnxDestroyOnlineStream\0")
            {
                if !self.stream.is_null() {
                    f(self.stream);
                }
            }
            if let Ok(f) = self
                .lib
                .get::<unsafe extern "C" fn(*const c_void)>(b"SherpaOnnxDestroyKeywordSpotter\0")
            {
                if !self.spotter.is_null() {
                    f(self.spotter);
                }
            }
        }
    }
}

pub struct SherpaVad<'a> {
    lib: &'a Library,
    vad: *const c_void,
}

impl<'a> SherpaVad<'a> {
    pub fn new(runtime: &'a SherpaRuntime, silero_model: &Path) -> Result<Self, String> {
        let c_model = path_cstring(silero_model)?;
        let c_prov = CString::new("cpu").unwrap();

        let mut cfg: SherpaOnnxVadModelConfig = unsafe { std::mem::zeroed() };
        cfg.silero_vad.model = c_model.as_ptr();
        cfg.silero_vad.threshold = 0.45;
        cfg.silero_vad.min_silence_duration = 0.75;
        cfg.silero_vad.min_speech_duration = 0.2;
        cfg.silero_vad.window_size = 512;
        cfg.silero_vad.max_speech_duration = 16.0;
        cfg.sample_rate = 16000;
        cfg.num_threads = 1;
        cfg.provider = c_prov.as_ptr();

        unsafe {
            let create: libloading::Symbol<
                unsafe extern "C" fn(*const SherpaOnnxVadModelConfig, f32) -> *const c_void,
            > = runtime
                .lib
                .get(b"SherpaOnnxCreateVoiceActivityDetector\0")
                .map_err(|e| format!("读取 SherpaOnnxCreateVoiceActivityDetector 失败: {e}"))?;
            let vad = create(&cfg, 30.0);
            if vad.is_null() {
                return Err("初始化 Silero VAD 失败".to_string());
            }
            Ok(Self {
                lib: &runtime.lib,
                vad,
            })
        }
    }

    pub fn clear(&self) {
        unsafe {
            if let Ok(f) = self
                .lib
                .get::<unsafe extern "C" fn(*const c_void)>(b"SherpaOnnxVoiceActivityDetectorClear\0")
            {
                f(self.vad);
            }
        }
    }

    pub fn accept_waveform(&self, samples: &[f32]) {
        unsafe {
            if let Ok(f) = self.lib.get::<unsafe extern "C" fn(*const c_void, *const c_float, i32)>(
                b"SherpaOnnxVoiceActivityDetectorAcceptWaveform\0",
            ) {
                f(self.vad, samples.as_ptr(), samples.len() as i32);
            }
        }
    }

    pub fn is_speech_detected(&self) -> bool {
        unsafe {
            self.lib
                .get::<unsafe extern "C" fn(*const c_void) -> i32>(
                    b"SherpaOnnxVoiceActivityDetectorDetected\0",
                )
                .map(|f| f(self.vad) != 0)
                .unwrap_or(false)
        }
    }

    pub fn pop_next_segment(&self) -> Option<Vec<f32>> {
        unsafe {
            let empty_fn: libloading::Symbol<unsafe extern "C" fn(*const c_void) -> i32> = self
                .lib
                .get(b"SherpaOnnxVoiceActivityDetectorEmpty\0")
                .ok()?;
            if empty_fn(self.vad) != 0 {
                return None;
            }
            let front_fn: libloading::Symbol<
                unsafe extern "C" fn(*const c_void) -> *const SherpaOnnxSpeechSegment,
            > = self
                .lib
                .get(b"SherpaOnnxVoiceActivityDetectorFront\0")
                .ok()?;
            let pop_fn: libloading::Symbol<unsafe extern "C" fn(*const c_void)> = self
                .lib
                .get(b"SherpaOnnxVoiceActivityDetectorPop\0")
                .ok()?;
            let destroy_fn: libloading::Symbol<
                unsafe extern "C" fn(*const SherpaOnnxSpeechSegment),
            > = self
                .lib
                .get(b"SherpaOnnxDestroySpeechSegment\0")
                .ok()?;

            let seg_ptr = front_fn(self.vad);
            if seg_ptr.is_null() {
                return None;
            }
            let n = (*seg_ptr).n.max(0) as usize;
            let samples = if n > 0 && !(*seg_ptr).samples.is_null() {
                std::slice::from_raw_parts((*seg_ptr).samples, n).to_vec()
            } else {
                Vec::new()
            };
            destroy_fn(seg_ptr);
            pop_fn(self.vad);
            Some(samples)
        }
    }
}

impl Drop for SherpaVad<'_> {
    fn drop(&mut self) {
        unsafe {
            if let Ok(f) = self.lib.get::<unsafe extern "C" fn(*const c_void)>(
                b"SherpaOnnxDestroyVoiceActivityDetector\0",
            ) {
                if !self.vad.is_null() {
                    f(self.vad);
                }
            }
        }
    }
}

pub enum AsrKind {
    SenseVoice,
    Paraformer,
}

pub struct SherpaOfflineAsr<'a> {
    lib: &'a Library,
    recognizer: *const c_void,
}

impl<'a> SherpaOfflineAsr<'a> {
    pub fn new(
        runtime: &'a SherpaRuntime,
        kind: AsrKind,
        model_onnx: &Path,
        tokens: &Path,
    ) -> Result<Self, String> {
        let c_model = path_cstring(model_onnx)?;
        let c_tokens = path_cstring(tokens)?;
        let c_lang = CString::new("auto").unwrap();
        let c_prov = CString::new("cpu").unwrap();
        let c_dec = CString::new("greedy_search").unwrap();

        let mut cfg: SherpaOnnxOfflineRecognizerConfig = unsafe { std::mem::zeroed() };
        cfg.feat_config.sample_rate = 16000;
        cfg.feat_config.feature_dim = 80;
        cfg.model_config.tokens = c_tokens.as_ptr();
        cfg.model_config.num_threads = 4;
        cfg.model_config.provider = c_prov.as_ptr();
        cfg.decoding_method = c_dec.as_ptr();
        cfg.max_active_paths = 4;

        match kind {
            AsrKind::SenseVoice => {
                cfg.model_config.sense_voice.model = c_model.as_ptr();
                cfg.model_config.sense_voice.language = c_lang.as_ptr();
                cfg.model_config.sense_voice.use_itn = 1;
            }
            AsrKind::Paraformer => {
                cfg.model_config.paraformer.model = c_model.as_ptr();
            }
        }

        unsafe {
            let create: libloading::Symbol<
                unsafe extern "C" fn(*const SherpaOnnxOfflineRecognizerConfig) -> *const c_void,
            > = runtime
                .lib
                .get(b"SherpaOnnxCreateOfflineRecognizer\0")
                .map_err(|e| format!("读取 SherpaOnnxCreateOfflineRecognizer 失败: {e}"))?;
            let recognizer = create(&cfg);
            if recognizer.is_null() {
                return Err("初始化 SherpaOnnxOfflineRecognizer 失败（请检查 ASR 模型文件）".to_string());
            }
            Ok(Self {
                lib: &runtime.lib,
                recognizer,
            })
        }
    }

    pub fn transcribe(&self, samples: &[f32]) -> Result<String, String> {
        if samples.is_empty() {
            return Ok(String::new());
        }
        unsafe {
            let create_stream: libloading::Symbol<
                unsafe extern "C" fn(*const c_void) -> *const c_void,
            > = self
                .lib
                .get(b"SherpaOnnxCreateOfflineStream\0")
                .map_err(|e| format!("读取 SherpaOnnxCreateOfflineStream 失败: {e}"))?;
            let accept: libloading::Symbol<
                unsafe extern "C" fn(*const c_void, i32, *const c_float, i32),
            > = self
                .lib
                .get(b"SherpaOnnxAcceptWaveformOffline\0")
                .map_err(|e| format!("读取 SherpaOnnxAcceptWaveformOffline 失败: {e}"))?;
            let decode: libloading::Symbol<unsafe extern "C" fn(*const c_void, *const c_void)> =
                self.lib
                    .get(b"SherpaOnnxDecodeOfflineStream\0")
                    .map_err(|e| format!("读取 SherpaOnnxDecodeOfflineStream 失败: {e}"))?;
            let get_res: libloading::Symbol<
                unsafe extern "C" fn(*const c_void) -> *const SherpaOnnxOfflineRecognizerResult,
            > = self
                .lib
                .get(b"SherpaOnnxGetOfflineStreamResult\0")
                .map_err(|e| format!("读取 SherpaOnnxGetOfflineStreamResult 失败: {e}"))?;
            let destroy_res: libloading::Symbol<
                unsafe extern "C" fn(*const SherpaOnnxOfflineRecognizerResult),
            > = self
                .lib
                .get(b"SherpaOnnxDestroyOfflineRecognizerResult\0")
                .map_err(|e| format!("读取 SherpaOnnxDestroyOfflineRecognizerResult 失败: {e}"))?;
            let destroy_stream: libloading::Symbol<unsafe extern "C" fn(*const c_void)> = self
                .lib
                .get(b"SherpaOnnxDestroyOfflineStream\0")
                .map_err(|e| format!("读取 SherpaOnnxDestroyOfflineStream 失败: {e}"))?;

            let stream = create_stream(self.recognizer);
            if stream.is_null() {
                return Err("创建离线识别音频流失败".to_string());
            }
            accept(stream, 16000, samples.as_ptr(), samples.len() as i32);
            decode(self.recognizer, stream);
            let res_ptr = get_res(stream);
            let mut text = String::new();
            if !res_ptr.is_null() {
                if !(*res_ptr).text.is_null() {
                    text = CStr::from_ptr((*res_ptr).text)
                        .to_string_lossy()
                        .trim()
                        .to_string();
                }
                destroy_res(res_ptr);
            }
            destroy_stream(stream);
            Ok(text)
        }
    }
}

impl Drop for SherpaOfflineAsr<'_> {
    fn drop(&mut self) {
        unsafe {
            if let Ok(f) = self.lib.get::<unsafe extern "C" fn(*const c_void)>(
                b"SherpaOnnxDestroyOfflineRecognizer\0",
            ) {
                if !self.recognizer.is_null() {
                    f(self.recognizer);
                }
            }
        }
    }
}

fn path_cstring(p: &Path) -> Result<CString, String> {
    let s = p.to_string_lossy();
    CString::new(s.as_ref()).map_err(|_| format!("路径包含非法空字符: {}", p.display()))
}

// 供 Rust 编译器验证未使用的零初始化指针引用合法性
#[allow(dead_code)]
fn _assert_null_ptr() -> *const c_void {
    ptr::null()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::assets::{find_kws_paths, find_sherpa_c_api_dll};

    #[test]
    fn kws_initializes_if_local_assets_installed() {
        let storage = Path::new("D:\\SkippyData");
        let Some(dll) = find_sherpa_c_api_dll(storage) else {
            return;
        };
        let Some(kws_paths) = find_kws_paths(storage, "密探阿泰") else {
            return;
        };
        assert!(
            kws_paths
                .encoder
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|n| n.contains("chunk-8")),
            "expected chunk-8 fp32 encoder, got {:?}",
            kws_paths.encoder
        );
        let rt = SherpaRuntime::load(&dll).expect("load sherpa dll");
        let kws = SherpaKws::new(
            &rt,
            &kws_paths.encoder,
            &kws_paths.decoder,
            &kws_paths.joiner,
            &kws_paths.tokens,
            &kws_paths.keywords_file,
        )
        .expect("init kws");
        assert!(kws.feed_and_detect(&vec![0.0f32; 1600]).is_none());
    }
}
