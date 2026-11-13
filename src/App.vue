<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Bot,
  Boxes,
  Keyboard,
  Mic,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Send,
  Settings,
  Smartphone,
  Square,
  Trash2,
  VolumeX,
} from "lucide-vue-next";
import ModelHubModal from "./components/ModelHubModal.vue";
import SettingsModal from "./components/SettingsModal.vue";
import type {
  AppSnapshot,
  ChatMessage,
  ChatSession,
  ChatTurnStartedEvent,
  DownloadProgressEvent,
  LlmStreamEvent,
  ModelCatalogItem,
  ModelCategory,
  RuntimeStatus,
  SendMessageResult,
  VoiceStatusEvent,
} from "./types/skippy";

const loading = ref(true);
const sidebarOpen = ref(true);
const modelHubOpen = ref(false);
const settingsOpen = ref(false);
const messagePaneRef = ref<HTMLElement | null>(null);

const status = ref<RuntimeStatus>({
  stage: "shortcut_only",
  stage_label: "初始化中...",
  ollama_connected: false,
  ollama_models: [],
  python_worker_running: false,
  python_env: {
    python_available: false,
    python_version: null,
    python_path: null,
    venv_ready: false,
    worker_script_found: false,
  },
  llama_runtime_ready: false,
  llama_server_running: false,
  llama_loaded_model: null,
  voice_runtime_ready: false,
  local_storage_dir: "D:\\SkippyData",
  gpu_vram_active_mb: 0,
});
const workerBusy = ref(false);
const workerError = ref("");
const llamaBusy = ref(false);
const llamaError = ref("");
const voiceBusy = ref(false);
const voiceError = ref("");
const voiceRecording = ref(false);
const downloadProgress = ref<Record<string, DownloadProgressEvent>>({});
const sessions = ref<ChatSession[]>([]);
const activeSessionId = ref<string>("");
const messages = ref<ChatMessage[]>([]);
const models = ref<ModelCatalogItem[]>([]);
const settings = ref<Record<string, string>>({});
const dataDir = ref("");
const inputText = ref("");
const sending = ref(false);
const streamStage = ref("");
const loadError = ref("");
const isTauriRuntime = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const activeLlm = computed(
  () => models.value.find((m) => m.category === "llm" && m.is_active)?.name ?? "未选择大脑",
);

const activeLlmItem = computed(() =>
  models.value.find((m) => m.category === "llm" && m.is_active),
);

function scrollToBottom() {
  nextTick(() => {
    const el = messagePaneRef.value;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  });
}

function splitThinking(raw: string): { think: string; answer: string; thinkingOpen: boolean } {
  const start = raw.indexOf("<think>");
  if (start === -1) {
    return { think: "", answer: raw, thinkingOpen: false };
  }
  const afterStart = raw.slice(start + 7);
  const end = afterStart.indexOf("</think>");
  if (end === -1) {
    return { think: afterStart.trim(), answer: "", thinkingOpen: true };
  }
  const think = afterStart.slice(0, end).trim();
  const answer = (raw.slice(0, start) + afterStart.slice(end + 8)).trim();
  return { think, answer, thinkingOpen: false };
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function formatMarkdownBasic(text: string): string {
  const parts = text.split(/```(\w*)\n?([\s\S]*?)```/g);
  let out = "";
  for (let i = 0; i < parts.length; i += 3) {
    const prose = escapeHtml(parts[i] ?? "")
      .replace(/`([^`\n]+)`/g, '<code class="rounded bg-zinc-200/80 px-1 py-0.5 font-mono text-[12px] dark:bg-zinc-800">$1</code>')
      .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
      .replace(/\n/g, "<br/>");
    out += prose;
    if (i + 2 < parts.length) {
      const code = escapeHtml(parts[i + 2] ?? "");
      out += `<pre class="my-2 overflow-x-auto rounded-lg bg-zinc-900 p-3 font-mono text-xs text-zinc-100"><code>${code}</code></pre>`;
    }
  }
  return out;
}

function applyTheme(theme: string | undefined) {
  const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const shouldDark = theme === "dark" || (theme !== "light" && prefersDark);
  document.documentElement.classList.toggle("dark", shouldDark);
}

watch(
  () => settings.value.theme,
  (val) => applyTheme(val),
);

async function loadSnapshot() {
  if (!isTauriRuntime) {
    status.value.stage_label = "网页预览模式（无桌面后端）";
    loadError.value =
      "当前正在普通浏览器/编辑器网页中访问 localhost:1420，无法连接 Rust 桌面后端。请切换到 pnpm tauri dev 自动弹出的 Skippy 桌面独立窗口（若已隐藏到托盘，可按 Alt+Space 或点击右下角托盘图标唤醒）。";
    loading.value = false;
    return;
  }

  loadError.value = "";
  try {
    const snap = await invoke<AppSnapshot>("get_app_snapshot");
    status.value = snap.status;
    sessions.value = snap.sessions;
    models.value = snap.models;
    settings.value = snap.settings;
    dataDir.value = snap.data_dir;
    applyTheme(snap.settings.theme);

    if (snap.sessions.length > 0) {
      activeSessionId.value = snap.sessions[0].id;
      await loadMessages(snap.sessions[0].id);
    }
  } catch (err) {
    status.value.stage_label = "后端连接异常";
    loadError.value = `无法读取 Skippy 桌面后端状态：${String(err)}`;
  } finally {
    loading.value = false;
  }
}

async function loadMessages(sessionId: string) {
  activeSessionId.value = sessionId;
  messages.value = await invoke<ChatMessage[]>("get_session_messages", { sessionId });
  scrollToBottom();
}

async function refreshSnapshotStatus() {
  const snap = await invoke<AppSnapshot>("get_app_snapshot");
  status.value = snap.status;
  models.value = snap.models;
  settings.value = snap.settings;
}

async function handleCreateSession() {
  const created = await invoke<ChatSession>("create_chat_session", {
    title: `新对话 ${sessions.value.length + 1}`,
  });
  sessions.value.unshift(created);
  await loadMessages(created.id);
}

async function handleDeleteSession(sessionId: string) {
  await invoke("delete_chat_session", { sessionId });
  sessions.value = sessions.value.filter((s) => s.id !== sessionId);
  if (activeSessionId.value === sessionId) {
    if (sessions.value.length > 0) {
      await loadMessages(sessions.value[0].id);
    } else {
      await handleCreateSession();
    }
  }
}

async function handleSend() {
  const text = inputText.value.trim();
  if (!text || !activeSessionId.value || sending.value) return;
  sending.value = true;
  streamStage.value = "preparing";
  inputText.value = "";

  const sid = activeSessionId.value;
  const tempUserId = `temp-user-${Date.now()}`;
  const tempAssistantId = `temp-assistant-${Date.now()}`;
  const nowIso = new Date().toISOString();

  messages.value.push(
    {
      id: tempUserId,
      session_id: sid,
      role: "user",
      source: "pc_text",
      content: text,
      created_at: nowIso,
    },
    {
      id: tempAssistantId,
      session_id: sid,
      role: "assistant",
      source: "pc_text",
      content: "",
      model_used: activeLlm.value,
      created_at: nowIso,
    },
  );
  scrollToBottom();

  try {
    const res = await invoke<SendMessageResult>("send_text_message", {
      sessionId: sid,
      content: text,
    });

    const userIdx = messages.value.findIndex((m) => m.id === tempUserId);
    if (userIdx !== -1) messages.value[userIdx] = res.user_message;

    const assistantIdx = messages.value.findIndex((m) => m.id === tempAssistantId);
    if (assistantIdx !== -1) messages.value[assistantIdx] = res.assistant_message;

    if (res.session_title) {
      const targetSession = sessions.value.find((s) => s.id === sid);
      if (targetSession) targetSession.title = res.session_title;
    }

    await refreshSnapshotStatus();
    scrollToBottom();
  } catch (err) {
    const assistantIdx = messages.value.findIndex((m) => m.id === tempAssistantId);
    if (assistantIdx !== -1) {
      messages.value[assistantIdx] = {
        ...messages.value[assistantIdx],
        content: `发送失败：${String(err)}`,
        model_used: "system-error",
      };
    }
  } finally {
    sending.value = false;
    streamStage.value = "";
  }
}

async function handleStopGeneration() {
  await invoke("stop_generation");
}

async function handleSelectModel(category: ModelCategory, modelId: string) {
  models.value = await invoke<ModelCatalogItem[]>("select_active_model", {
    category,
    modelId,
  });
  await refreshSnapshotStatus();
}

async function handleUpdateSetting(key: string, value: string) {
  settings.value = await invoke<Record<string, string>>("update_app_setting", {
    key,
    value,
  });
  if (key === "local_storage_dir") {
    await refreshSnapshotStatus();
  }
}

async function handlePickStorageDir() {
  const picked = await invoke<string | null>("pick_storage_directory", {
    initial: settings.value.local_storage_dir || status.value.local_storage_dir,
  });
  if (picked) {
    await handleUpdateSetting("local_storage_dir", picked);
  }
}

async function handleInstallLlama() {
  llamaError.value = "";
  try {
    status.value = await invoke<RuntimeStatus>("install_llama_runtime", {
      mirror: settings.value.github_mirror || null,
    });
  } catch (err) {
    llamaError.value = String(err);
  }
}

async function handleStartLlama(modelId?: string) {
  llamaBusy.value = true;
  llamaError.value = "";
  try {
    status.value = await invoke<RuntimeStatus>("start_llama_server", {
      modelId: modelId || null,
    });
  } catch (err) {
    llamaError.value = String(err);
  } finally {
    llamaBusy.value = false;
  }
}

async function handleStopLlama() {
  llamaBusy.value = true;
  llamaError.value = "";
  try {
    status.value = await invoke<RuntimeStatus>("stop_llama_server");
  } catch (err) {
    llamaError.value = String(err);
  } finally {
    llamaBusy.value = false;
  }
}

async function handleDownloadModel(modelId: string) {
  try {
    models.value = await invoke<ModelCatalogItem[]>("download_gguf_model", {
      modelId,
      mirror: settings.value.download_mirror || "modelscope",
    });
  } catch (err) {
    llamaError.value = String(err);
  }
}

async function handleCancelDownload() {
  await invoke("cancel_download");
}

async function handleImportGguf(modelId: string) {
  llamaError.value = "";
  const filePath = await invoke<string | null>("pick_local_gguf_file");
  if (!filePath) return;
  try {
    models.value = await invoke<ModelCatalogItem[]>("import_local_gguf", {
      modelId,
      filePath,
    });
    await refreshSnapshotStatus();
  } catch (err) {
    llamaError.value = String(err);
  }
}

async function handleDeleteModel(modelId: string) {
  try {
    models.value = await invoke<ModelCatalogItem[]>("delete_local_model", {
      modelId,
    });
    await refreshSnapshotStatus();
  } catch (err) {
    llamaError.value = String(err);
  }
}

async function handleStartWorker() {
  workerBusy.value = true;
  workerError.value = "";
  try {
    await invoke("start_python_worker");
    await refreshSnapshotStatus();
  } catch (err) {
    workerError.value = String(err);
  } finally {
    workerBusy.value = false;
  }
}

async function handleStopWorker() {
  workerBusy.value = true;
  workerError.value = "";
  try {
    status.value = await invoke<RuntimeStatus>("stop_python_worker");
  } catch (err) {
    workerError.value = String(err);
  } finally {
    workerBusy.value = false;
  }
}

async function handleToggleMic() {
  if (!isTauriRuntime) return;
  if (voiceRecording.value) {
    try {
      await invoke("stop_voice_capture");
    } catch (err) {
      voiceError.value = String(err);
    }
    return;
  }
  voiceError.value = "";
  try {
    await invoke("start_voice_capture", {
      sessionId: activeSessionId.value || null,
    });
  } catch (err) {
    voiceError.value = String(err);
  }
}

async function handleStopSpeaking() {
  if (!isTauriRuntime) return;
  await invoke("stop_speaking");
}

async function handleInstallVoiceSuite() {
  voiceBusy.value = true;
  voiceError.value = "";
  try {
    const snap = await invoke<AppSnapshot>("install_voice_suite");
    status.value = snap.status;
    models.value = snap.models;
    settings.value = snap.settings;
  } catch (err) {
    voiceError.value = String(err);
  } finally {
    voiceBusy.value = false;
  }
}

async function handleDownloadVoice(modelId: string) {
  voiceError.value = "";
  try {
    models.value = await invoke<ModelCatalogItem[]>("download_voice_model", {
      modelId,
    });
    await refreshSnapshotStatus();
  } catch (err) {
    voiceError.value = String(err);
  }
}

function sourceLabel(source: ChatMessage["source"]): string {
  switch (source) {
    case "pc_voice":
      return "语音唤醒";
    case "lan_web":
      return "手机遥控";
    case "system":
      return "系统";
    default:
      return "键盘";
  }
}

let unlistenDownload: UnlistenFn | null = null;
let unlistenStream: UnlistenFn | null = null;
let unlistenVoice: UnlistenFn | null = null;
let unlistenTurnStarted: UnlistenFn | null = null;

onMounted(async () => {
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    applyTheme(settings.value.theme);
  });

  if (isTauriRuntime) {
    unlistenDownload = await listen<DownloadProgressEvent>("download-progress", (evt) => {
      downloadProgress.value = {
        ...downloadProgress.value,
        [evt.payload.target_id]: evt.payload,
      };
    });

    unlistenVoice = await listen<VoiceStatusEvent>("voice-status", (evt) => {
      const p = evt.payload;
      status.value = {
        ...status.value,
        stage: p.stage,
        stage_label: p.stage_label,
      };
      voiceRecording.value = p.recording;
      if (p.error) {
        voiceError.value = p.error;
      }
    });

    unlistenTurnStarted = await listen<ChatTurnStartedEvent>("chat-turn-started", (evt) => {
      const p = evt.payload;
      if (p.session_title) {
        const targetSession = sessions.value.find((s) => s.id === p.session_id);
        if (targetSession) targetSession.title = p.session_title;
      }
      if (p.session_id !== activeSessionId.value) return;

      const hasRealUser = messages.value.some((m) => m.id === p.user_message.id);
      const hasRealAsst = messages.value.some((m) => m.id === p.assistant_message.id);
      if (hasRealUser && hasRealAsst) return;

      const tempUserIdx = messages.value.findIndex((m) => m.id.startsWith("temp-user-"));
      if (tempUserIdx !== -1) {
        messages.value[tempUserIdx] = p.user_message;
      } else if (!hasRealUser) {
        messages.value.push(p.user_message);
      }

      const tempAsstIdx = messages.value.findIndex((m) => m.id.startsWith("temp-assistant-"));
      if (tempAsstIdx !== -1) {
        messages.value[tempAsstIdx] = p.assistant_message;
      } else if (!hasRealAsst) {
        messages.value.push(p.assistant_message);
      }
      scrollToBottom();
    });

    unlistenStream = await listen<LlmStreamEvent>("llm-stream", (evt) => {
      const p = evt.payload;
      if (p.session_id !== activeSessionId.value) return;
      streamStage.value = p.done || p.cancelled || p.error ? "" : p.stage;

      const idx = messages.value.findIndex(
        (m) => m.id === p.message_id || m.id.startsWith("temp-assistant-"),
      );
      if (idx !== -1 && p.full_content) {
        messages.value[idx] = {
          ...messages.value[idx],
          content: p.full_content,
        };
        scrollToBottom();
      }
      if (p.done || p.cancelled || p.error) {
        void refreshSnapshotStatus();
      }
    });
  }

  await loadSnapshot();
});

onUnmounted(() => {
  unlistenDownload?.();
  unlistenStream?.();
  unlistenVoice?.();
  unlistenTurnStarted?.();
});
</script>

<template>
  <div class="flex h-screen w-screen bg-zinc-50 text-zinc-900 dark:bg-zinc-950 dark:text-zinc-100">
    <!-- Left Sidebar -->
    <aside
      v-if="sidebarOpen"
      class="flex w-64 shrink-0 flex-col border-r border-zinc-200 bg-zinc-100/70 dark:border-zinc-800 dark:bg-zinc-900/70"
    >
      <div class="flex items-center justify-between px-4 py-3.5">
        <div class="flex items-center gap-2">
          <div class="flex h-7 w-7 items-center justify-center rounded-lg bg-blue-600 text-xs font-bold text-white">
            S
          </div>
          <span class="text-sm font-semibold tracking-tight">Skippy</span>
        </div>
        <button
          class="rounded-lg p-1.5 text-zinc-500 hover:bg-zinc-200/70 dark:text-zinc-400 dark:hover:bg-zinc-800"
          title="新建对话"
          @click="handleCreateSession"
        >
          <Plus class="h-4 w-4" />
        </button>
      </div>

      <div class="flex-1 space-y-1 overflow-y-auto px-2.5 py-1">
        <div
          v-for="s in sessions"
          :key="s.id"
          class="group flex cursor-pointer items-center justify-between rounded-lg px-3 py-2 text-xs transition"
          :class="
            s.id === activeSessionId
              ? 'bg-white font-medium text-zinc-900 shadow-xs ring-1 ring-zinc-200/80 dark:bg-zinc-800 dark:text-zinc-100 dark:ring-zinc-700'
              : 'text-zinc-600 hover:bg-zinc-200/50 dark:text-zinc-400 dark:hover:bg-zinc-800/50'
          "
          @click="loadMessages(s.id)"
        >
          <span class="truncate">{{ s.title }}</span>
          <button
            class="hidden rounded p-0.5 text-zinc-400 hover:text-red-500 group-hover:block"
            title="删除会话"
            @click.stop="handleDeleteSession(s.id)"
          >
            <Trash2 class="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      <div class="border-t border-zinc-200 px-3 py-2.5 text-[11px] text-zinc-500 dark:border-zinc-800 dark:text-zinc-400">
        关闭窗口将销毁渲染层，托盘常驻低占用运行
      </div>
    </aside>

    <!-- Main Chat Panel -->
    <div class="flex min-w-0 flex-1 flex-col bg-white dark:bg-zinc-950">
      <div
        v-if="loadError"
        class="border-b border-amber-200 bg-amber-50 px-4 py-2.5 text-xs text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/40 dark:text-amber-300"
      >
        {{ loadError }}
      </div>

      <div
        v-if="voiceError"
        class="flex items-center justify-between border-b border-red-200 bg-red-50 px-4 py-2 text-xs text-red-700 dark:border-red-900/60 dark:bg-red-950/40 dark:text-red-300"
      >
        <span>语音提示：{{ voiceError }}</span>
        <button class="ml-3 text-[11px] font-medium hover:underline" @click="voiceError = ''">
          关闭
        </button>
      </div>

      <!-- Top Bar -->
      <header class="flex items-center justify-between border-b border-zinc-200 px-4 py-3 dark:border-zinc-800">
        <div class="flex items-center gap-2.5">
          <button
            class="rounded-lg p-1.5 text-zinc-500 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
            @click="sidebarOpen = !sidebarOpen"
          >
            <PanelLeftClose v-if="sidebarOpen" class="h-4 w-4" />
            <PanelLeftOpen v-else class="h-4 w-4" />
          </button>

          <div
            class="inline-flex items-center gap-1.5 rounded-full border border-zinc-200 bg-zinc-50 px-2.5 py-1 text-xs text-zinc-700 dark:border-zinc-800 dark:bg-zinc-900 dark:text-zinc-300"
          >
            <span
              class="h-2 w-2 rounded-full"
              :class="{
                'bg-red-500 animate-pulse': status.stage === 'recording',
                'bg-blue-500 animate-pulse': status.stage === 'speaking',
                'bg-amber-500 animate-pulse': status.stage === 'thinking',
                'bg-emerald-500':
                  status.stage !== 'recording' &&
                  status.stage !== 'speaking' &&
                  status.stage !== 'thinking',
              }"
            />
            <span>{{ status.stage_label }}</span>
          </div>
        </div>

        <div class="flex items-center gap-2">
          <button
            v-if="status.stage === 'speaking'"
            class="inline-flex items-center gap-1.5 rounded-lg border border-blue-200 bg-blue-50 px-3 py-1.5 text-xs font-medium text-blue-700 hover:bg-blue-100 dark:border-blue-900/50 dark:bg-blue-950/40 dark:text-blue-300"
            title="立即打断并停止语音播报"
            @click="handleStopSpeaking"
          >
            <VolumeX class="h-3.5 w-3.5" />
            <span>停止播报</span>
          </button>
          <button
            class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-200 bg-white px-3 py-1.5 text-xs font-medium text-zinc-700 hover:bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-900 dark:text-zinc-200 dark:hover:bg-zinc-800"
            @click="modelHubOpen = true"
          >
            <Boxes class="h-3.5 w-3.5 text-blue-600 dark:text-blue-400" />
            <span>模型管理 · {{ activeLlm }}</span>
          </button>
          <button
            class="rounded-lg border border-zinc-200 bg-white p-1.5 text-zinc-600 hover:bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800"
            title="设置"
            @click="settingsOpen = true"
          >
            <Settings class="h-4 w-4" />
          </button>
        </div>
      </header>

      <!-- Quick Setup Hint for Pure Local Mode -->
      <div
        v-if="!loadError && activeLlmItem && activeLlmItem.engine === 'llama-server' && (!status.llama_runtime_ready || !activeLlmItem.is_installed)"
        class="flex items-center justify-between border-b border-blue-200 bg-blue-50/80 px-4 py-2 text-xs text-blue-800 dark:border-blue-900/50 dark:bg-blue-950/30 dark:text-blue-200"
      >
        <span>
          纯本地大脑待就绪：
          <template v-if="!status.llama_runtime_ready">需要先安装内置 llama-server 引擎</template>
          <template v-if="!status.llama_runtime_ready && !activeLlmItem.is_installed">，并</template>
          <template v-if="!activeLlmItem.is_installed">下载或导入 {{ activeLlmItem.name }} 权重</template>
          （将存放于 <span class="font-mono">{{ status.local_storage_dir }}</span>）
        </span>
        <button
          class="rounded-md bg-blue-600 px-2.5 py-1 text-[11px] font-medium text-white hover:bg-blue-700"
          @click="modelHubOpen = true"
        >
          打开模型管理配置
        </button>
      </div>

      <!-- Messages -->
      <main ref="messagePaneRef" class="flex-1 space-y-4 overflow-y-auto px-6 py-5">
        <div v-if="loading" class="text-center text-xs text-zinc-400">正在同步历史记录...</div>

        <div
          v-for="msg in messages"
          :key="msg.id"
          class="flex"
          :class="msg.role === 'user' ? 'justify-end' : 'justify-start'"
        >
          <div
            class="max-w-[78%] rounded-2xl px-4 py-3 text-sm leading-relaxed"
            :class="
              msg.role === 'user'
                ? 'bg-blue-600 text-white'
                : 'border border-zinc-200 bg-zinc-50 text-zinc-800 dark:border-zinc-800 dark:bg-zinc-900 dark:text-zinc-100'
            "
          >
            <div
              class="mb-1.5 flex items-center gap-1.5 text-[11px]"
              :class="msg.role === 'user' ? 'text-blue-100' : 'text-zinc-400'"
            >
              <Mic v-if="msg.source === 'pc_voice'" class="h-3 w-3" />
              <Smartphone v-else-if="msg.source === 'lan_web'" class="h-3 w-3" />
              <Bot v-else-if="msg.role === 'assistant'" class="h-3 w-3" />
              <Keyboard v-else class="h-3 w-3" />
              <span>{{ sourceLabel(msg.source) }}</span>
              <span v-if="msg.model_used">· {{ msg.model_used }}</span>
            </div>

            <div
              v-if="msg.role === 'assistant' && !msg.content && (sending || Boolean(streamStage))"
              class="text-xs text-zinc-400"
            >
              {{ streamStage === "loading_model" ? "正在加载本地模型至 RTX 5070 显存..." : "Skippy 思考中..." }}
            </div>

            <template v-else-if="msg.role === 'assistant'">
              <details
                v-if="splitThinking(msg.content).think"
                :open="splitThinking(msg.content).thinkingOpen"
                class="mb-2 rounded-lg border border-zinc-200 bg-white/70 px-3 py-2 text-xs text-zinc-500 dark:border-zinc-700 dark:bg-zinc-950/50 dark:text-zinc-400"
              >
                <summary class="cursor-pointer font-medium select-none">思考过程</summary>
                <div class="mt-1.5 whitespace-pre-wrap leading-relaxed">
                  {{ splitThinking(msg.content).think }}
                </div>
              </details>
              <!-- eslint-disable-next-line vue/no-v-html -->
              <div
                v-if="splitThinking(msg.content).answer"
                class="break-words"
                v-html="formatMarkdownBasic(splitThinking(msg.content).answer)"
              />
            </template>

            <div v-else class="whitespace-pre-wrap">{{ msg.content }}</div>
          </div>
        </div>
      </main>

      <!-- Composer -->
      <footer class="border-t border-zinc-200 px-6 py-4 dark:border-zinc-800">
        <div
          class="flex items-end gap-2 rounded-xl border border-zinc-200 bg-zinc-50/70 p-2 focus-within:border-blue-500 focus-within:bg-white dark:border-zinc-800 dark:bg-zinc-900 dark:focus-within:bg-zinc-900"
        >
          <button
            class="rounded-lg p-2 transition"
            :class="
              voiceRecording
                ? 'animate-pulse bg-red-500 text-white hover:bg-red-600'
                : 'text-zinc-500 hover:bg-zinc-200/60 hover:text-zinc-700 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-zinc-200'
            "
            :title="voiceRecording ? '正在聆听，点击立即结束收音并识别' : '点击说话（纯本地离线语音识别 + 播报）'"
            @click="handleToggleMic"
          >
            <Mic class="h-4 w-4" />
          </button>
          <textarea
            v-model="inputText"
            rows="1"
            placeholder="输入指令或问题（回车发送，Alt+Space 快速呼出/收起）..."
            class="max-h-32 flex-1 resize-none bg-transparent py-1.5 text-sm outline-none placeholder:text-zinc-400"
            @keydown.enter.exact.prevent="handleSend"
          />
          <button
            v-if="sending"
            class="inline-flex items-center gap-1 rounded-lg bg-red-600 px-3 py-2 text-xs font-medium text-white transition hover:bg-red-700"
            @click="handleStopGeneration"
          >
            <Square class="h-3.5 w-3.5" />
            <span>停止</span>
          </button>
          <button
            v-else
            :disabled="!inputText.trim()"
            class="inline-flex items-center gap-1 rounded-lg bg-blue-600 px-3 py-2 text-xs font-medium text-white transition hover:bg-blue-700 disabled:opacity-40"
            @click="handleSend"
          >
            <Send class="h-3.5 w-3.5" />
            <span>发送</span>
          </button>
        </div>
      </footer>
    </div>

    <!-- Modals -->
    <ModelHubModal
      :open="modelHubOpen"
      :models="models"
      :ollama-connected="status.ollama_connected"
      :ollama-models="status.ollama_models"
      :python-running="status.python_worker_running"
      :python-env="status.python_env"
      :llama-runtime-ready="status.llama_runtime_ready"
      :llama-server-running="status.llama_server_running"
      :llama-loaded-model="status.llama_loaded_model"
      :voice-runtime-ready="status.voice_runtime_ready"
      :voice-busy="voiceBusy"
      :voice-error="voiceError"
      :local-storage-dir="status.local_storage_dir"
      :download-mirror="settings.download_mirror || 'modelscope'"
      :download-progress="downloadProgress"
      :worker-busy="workerBusy"
      :worker-error="workerError"
      :llama-busy="llamaBusy"
      :llama-error="llamaError"
      @close="modelHubOpen = false"
      @select="handleSelectModel"
      @install-llama="handleInstallLlama"
      @start-llama="handleStartLlama"
      @stop-llama="handleStopLlama"
      @install-voice-suite="handleInstallVoiceSuite"
      @download-voice="handleDownloadVoice"
      @download-model="handleDownloadModel"
      @cancel-download="handleCancelDownload"
      @import-gguf="handleImportGguf"
      @delete-model="handleDeleteModel"
      @start-worker="handleStartWorker"
      @stop-worker="handleStopWorker"
    />

    <SettingsModal
      :open="settingsOpen"
      :settings="settings"
      :data-dir="dataDir"
      :local-storage-dir="status.local_storage_dir"
      :ollama-connected="status.ollama_connected"
      :ollama-models="status.ollama_models"
      @close="settingsOpen = false"
      @update="handleUpdateSetting"
      @pick-storage-dir="handlePickStorageDir"
    />
  </div>
</template>
