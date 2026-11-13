export type VoiceStage =
  | "shortcut_only"
  | "idle_listening"
  | "recording"
  | "thinking"
  | "speaking"
  | "sleeping";

export interface PythonEnvInfo {
  python_available: boolean;
  python_version: string | null;
  python_path: string | null;
  venv_ready: boolean;
  worker_script_found: boolean;
}

export interface RuntimeStatus {
  stage: VoiceStage;
  stage_label: string;
  ollama_connected: boolean;
  ollama_models: string[];
  python_worker_running: boolean;
  python_env: PythonEnvInfo;
  llama_runtime_ready: boolean;
  llama_server_running: boolean;
  llama_loaded_model: string | null;
  local_storage_dir: string;
  gpu_vram_active_mb: number;
}

export interface ChatSession {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
}

export interface ChatMessage {
  id: string;
  session_id: string;
  role: "user" | "assistant" | "system";
  source: "pc_text" | "pc_voice" | "lan_web" | "system";
  content: string;
  model_used?: string | null;
  tool_calls_json?: string | null;
  created_at: string;
}

export type ModelCategory = "kws" | "asr" | "llm" | "tts";

export interface ModelCatalogItem {
  id: string;
  category: ModelCategory;
  name: string;
  engine: string;
  compute: "cpu" | "gpu" | "cloud";
  est_vram_mb: number;
  disk_size_mb: number;
  description: string;
  tags: string[];
  requires_python: boolean;
  requires_ollama: boolean;
  is_active: boolean;
  is_installed: boolean;
  local_path?: string | null;
  downloadable: boolean;
}

export interface AppSnapshot {
  status: RuntimeStatus;
  sessions: ChatSession[];
  models: ModelCatalogItem[];
  settings: Record<string, string>;
  data_dir: string;
}

export interface SendMessageResult {
  user_message: ChatMessage;
  assistant_message: ChatMessage;
  session_title?: string | null;
}

export interface DownloadProgressEvent {
  target_id: string;
  stage: string;
  downloaded_bytes: number;
  total_bytes: number;
  percent: number;
  message: string;
  done: boolean;
  error?: string | null;
}

export interface LlmStreamEvent {
  session_id: string;
  message_id: string;
  delta: string;
  full_content: string;
  stage: string;
  done: boolean;
  cancelled: boolean;
  error?: string | null;
}
