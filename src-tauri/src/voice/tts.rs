use rodio::{Decoder, OutputStream, Sink};
use std::fs;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

enum TtsCmd {
    WakeChime,
    StopChime,
    Speak { epoch: u64, text: String },
    StopAll,
    Shutdown,
}

#[derive(Clone)]
pub struct TtsPlayer {
    tx: Sender<TtsCmd>,
    epoch: Arc<AtomicU64>,
    speaking_sink: Arc<Mutex<Option<Sink>>>,
}

impl TtsPlayer {
    pub fn new(temp_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel::<TtsCmd>();
        let epoch = Arc::new(AtomicU64::new(1));
        let sink_slot: Arc<Mutex<Option<Sink>>> = Arc::new(Mutex::new(None));

        let epoch_worker = Arc::clone(&epoch);
        let sink_worker = Arc::clone(&sink_slot);

        thread::spawn(move || {
            let _ = fs::create_dir_all(&temp_dir);
            let output_pair = OutputStream::try_default().ok();
            let stream_handle = output_pair.as_ref().map(|(_, handle)| handle);
            let mut synth = SapiWorker::spawn().ok();
            let mut wav_seq: u64 = 0;

            while let Ok(cmd) = rx.recv() {
                match cmd {
                    TtsCmd::Shutdown => {
                        if let Some(mut s) = synth.take() {
                            s.kill();
                        }
                        break;
                    }
                    TtsCmd::StopAll => {
                        if let Ok(mut guard) = sink_worker.lock() {
                            if let Some(sink) = guard.take() {
                                sink.stop();
                            }
                        }
                    }
                    TtsCmd::WakeChime => {
                        if let Some(handle) = stream_handle {
                            let bytes = build_chime_wav(&[(587.0, 65), (880.0, 95)]);
                            play_wav_bytes_blocking(handle, &bytes, false, &sink_worker);
                        }
                    }
                    TtsCmd::StopChime => {
                        if let Some(handle) = stream_handle {
                            let bytes = build_chime_wav(&[(660.0, 55), (440.0, 70)]);
                            play_wav_bytes_blocking(handle, &bytes, false, &sink_worker);
                        }
                    }
                    TtsCmd::Speak {
                        epoch: item_epoch,
                        text,
                    } => {
                        if item_epoch != epoch_worker.load(Ordering::Relaxed) {
                            continue;
                        }
                        let clean = sanitize_for_tts(&text);
                        if clean.is_empty() {
                            continue;
                        }

                        if synth.is_none() {
                            synth = SapiWorker::spawn().ok();
                        }
                        let Some(worker) = synth.as_mut() else {
                            continue;
                        };

                        wav_seq += 1;
                        let wav_path = temp_dir.join(format!("tts_{wav_seq}.wav"));
                        let synthesized = worker.synthesize_to_file(&clean, &wav_path);
                        if synthesized.is_err() {
                            if let Some(mut old_w) = synth.take() {
                                old_w.kill();
                            }
                            synth = SapiWorker::spawn().ok();
                            let Some(retry_w) = synth.as_mut() else {
                                continue;
                            };
                            if retry_w.synthesize_to_file(&clean, &wav_path).is_err() {
                                continue;
                            }
                        }

                        if item_epoch == epoch_worker.load(Ordering::Relaxed) {
                            if let (Ok(wav_bytes), Some(handle)) =
                                (fs::read(&wav_path), stream_handle)
                            {
                                play_wav_bytes_blocking(handle, &wav_bytes, true, &sink_worker);
                            }
                        }
                        let _ = fs::remove_file(&wav_path);
                    }
                }
            }
        });

        Self {
            tx,
            epoch,
            speaking_sink: sink_slot,
        }
    }

    pub fn current_generation(&self) -> u64 {
        self.epoch.load(Ordering::Relaxed)
    }

    pub fn play_wake_chime(&self) {
        let _ = self.tx.send(TtsCmd::WakeChime);
    }

    pub fn play_stop_chime(&self) {
        let _ = self.tx.send(TtsCmd::StopChime);
    }

    pub fn enqueue_sentence(&self, expected_epoch: u64, text: String) {
        if expected_epoch == self.epoch.load(Ordering::Relaxed) && !text.trim().is_empty() {
            let _ = self.tx.send(TtsCmd::Speak {
                epoch: expected_epoch,
                text,
            });
        }
    }

    pub fn stop_speaking(&self) -> u64 {
        let next = self.epoch.fetch_add(1, Ordering::Relaxed) + 1;
        if let Ok(mut guard) = self.speaking_sink.lock() {
            if let Some(sink) = guard.take() {
                sink.stop();
            }
        }
        let _ = self.tx.send(TtsCmd::StopAll);
        next
    }

    pub fn shutdown(&self) {
        self.stop_speaking();
        let _ = self.tx.send(TtsCmd::Shutdown);
    }
}

fn play_wav_bytes_blocking(
    handle: &rodio::OutputStreamHandle,
    wav_bytes: &[u8],
    interruptible: bool,
    sink_slot: &Arc<Mutex<Option<Sink>>>,
) {
    let Ok(sink) = Sink::try_new(handle) else {
        return;
    };
    let cursor = Cursor::new(wav_bytes.to_vec());
    let Ok(source) = Decoder::new(cursor) else {
        return;
    };
    sink.append(source);
    if interruptible {
        if let Ok(mut guard) = sink_slot.lock() {
            *guard = Some(sink);
        }
        loop {
            let done = match sink_slot.lock() {
                Ok(guard) => match guard.as_ref() {
                    Some(s) => s.empty(),
                    None => true,
                },
                Err(_) => true,
            };
            if done {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(35));
        }
        if let Ok(mut guard) = sink_slot.lock() {
            *guard = None;
        }
    } else {
        sink.sleep_until_end();
    }
}

struct SapiWorker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl SapiWorker {
    fn spawn() -> Result<Self, String> {
        let ps_script = r#"
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Speech
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$zhVoice = $synth.GetInstalledVoices() | Where-Object { $_.VoiceInfo.Culture.Name -like 'zh*' } | Select-Object -First 1
if ($zhVoice) { $synth.SelectVoice($zhVoice.VoiceInfo.Name) }
$synth.Rate = 1
while (($line = [Console]::In.ReadLine()) -ne $null) {
    if ($line -eq '__EXIT__') { break }
    $sep = $line.IndexOf("`t")
    if ($sep -lt 1) { [Console]::Out.WriteLine('ERR'); [Console]::Out.Flush(); continue }
    $wav = $line.Substring(0, $sep)
    $txt = $line.Substring($sep + 1)
    try {
        $synth.SetOutputToWaveFile($wav)
        $synth.Speak($txt)
        $synth.SetOutputToNull()
        [Console]::Out.WriteLine('OK')
    } catch {
        [Console]::Out.WriteLine('ERR')
    }
    [Console]::Out.Flush()
}
"#;
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", ps_script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let mut child = cmd.spawn().map_err(|e| format!("启动离线 TTS 失败: {e}"))?;
        let stdin = child.stdin.take().ok_or_else(|| "无法打开 TTS stdin".to_string())?;
        let stdout = BufReader::new(child.stdout.take().ok_or_else(|| "无法打开 TTS stdout".to_string())?);
        Ok(Self {
            child,
            stdin,
            stdout,
        })
    }

    fn synthesize_to_file(&mut self, text: &str, wav_path: &std::path::Path) -> Result<(), String> {
        let single_line = text.replace(['\r', '\n', '\t'], " ");
        let msg = format!("{}\t{}\n", wav_path.display(), single_line);
        self.stdin
            .write_all(msg.as_bytes())
            .and_then(|_| self.stdin.flush())
            .map_err(|e| format!("写入 TTS 指令失败: {e}"))?;

        let mut ack = String::new();
        self.stdout
            .read_line(&mut ack)
            .map_err(|e| format!("读取 TTS 回执失败: {e}"))?;
        if ack.trim() == "OK" && wav_path.exists() {
            Ok(())
        } else {
            Err("离线 TTS 合成失败".to_string())
        }
    }

    fn kill(&mut self) {
        let _ = self.stdin.write_all(b"__EXIT__\n");
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn sanitize_for_tts(raw: &str) -> String {
    let mut text = raw.to_string();
    while let Some(s) = text.find("<think>") {
        if let Some(e) = text[s..].find("</think>") {
            text.replace_range(s..s + e + 8, "");
        } else {
            text.truncate(s);
            break;
        }
    }
    while let Some(s) = text.find("```") {
        if let Some(e) = text[s + 3..].find("```") {
            text.replace_range(s..s + 3 + e + 3, "，代码块已略，");
        } else {
            break;
        }
    }
    text.replace(['*', '#', '`', '>', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_chime_wav(tones: &[(f32, u32)]) -> Vec<u8> {
    let sample_rate: u32 = 16000;
    let mut pcm: Vec<i16> = Vec::new();
    for &(freq, ms) in tones {
        let count = (sample_rate * ms) / 1000;
        for i in 0..count {
            let t = i as f32 / sample_rate as f32;
            let env = if i < 80 {
                i as f32 / 80.0
            } else if i + 120 > count {
                (count - i) as f32 / 120.0
            } else {
                1.0
            };
            let s = (2.0 * std::f32::consts::PI * freq * t).sin() * env * 0.22;
            pcm.push((s * i16::MAX as f32) as i16);
        }
    }

    let data_bytes = (pcm.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // Mono
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for s in pcm {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}
