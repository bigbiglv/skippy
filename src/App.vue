<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
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
  Trash2,
} from "lucide-vue-next";
import ModelHubModal from "./components/ModelHubModal.vue";
import SettingsModal from "./components/SettingsModal.vue";
import type {
  AppSnapshot,
  ChatMessage,
  ChatSession,
  ModelCatalogItem,
  ModelCategory,
  RuntimeStatus,
  SendMessageResult,
} from "./types/skippy";

const loading = ref(true);
const sidebarOpen = ref(true);
const modelHubOpen = ref(false);
const settingsOpen = ref(false);

const status = ref<RuntimeStatus>({
  stage: "shortcut_only",
  stage_label: "初始化中...",
  ollama_connected: false,
  python_worker_running: false,
  python_env: {
    python_available: false,
    python_version: null,
    python_path: null,
    venv_ready: false,
    worker_script_found: false,
  },
  gpu_vram_active_mb: 0,
});
const workerBusy = ref(false);
const workerError = ref("");
const sessions = ref<ChatSession[]>([]);
const activeSessionId = ref<string>("");
const messages = ref<ChatMessage[]>([]);
const models = ref<ModelCatalogItem[]>([]);
const settings = ref<Record<string, string>>({});
const dataDir = ref("");
const inputText = ref("");
const sending = ref(false);
const loadError = ref("");
const isTauriRuntime = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const activeLlm = computed(
  () => models.value.find((m) => m.category === "llm" && m.is_active)?.name ?? "未选择大脑",
);

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
  inputText.value = "";
  try {
    const res = await invoke<SendMessageResult>("send_text_message", {
      sessionId: activeSessionId.value,
      content: text,
    });
    messages.value.push(res.user_message, res.assistant_message);
  } finally {
    sending.value = false;
  }
}

async function handleSelectModel(category: ModelCategory, modelId: string) {
  models.value = await invoke<ModelCatalogItem[]>("select_active_model", {
    category,
    modelId,
  });
  const snap = await invoke<AppSnapshot>("get_app_snapshot");
  status.value = snap.status;
}

async function handleUpdateSetting(key: string, value: string) {
  settings.value = await invoke<Record<string, string>>("update_app_setting", {
    key,
    value,
  });
}

async function handleStartWorker() {
  workerBusy.value = true;
  workerError.value = "";
  try {
    await invoke("start_python_worker");
    const snap = await invoke<AppSnapshot>("get_app_snapshot");
    status.value = snap.status;
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

onMounted(() => {
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    applyTheme(settings.value.theme);
  });
  loadSnapshot();
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
            <span class="h-2 w-2 rounded-full bg-emerald-500" />
            <span>{{ status.stage_label }}</span>
          </div>
        </div>

        <div class="flex items-center gap-2">
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

      <!-- Messages -->
      <main class="flex-1 space-y-4 overflow-y-auto px-6 py-5">
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
            <div class="whitespace-pre-wrap">{{ msg.content }}</div>
          </div>
        </div>
      </main>

      <!-- Composer -->
      <footer class="border-t border-zinc-200 px-6 py-4 dark:border-zinc-800">
        <div
          class="flex items-end gap-2 rounded-xl border border-zinc-200 bg-zinc-50/70 p-2 focus-within:border-blue-500 focus-within:bg-white dark:border-zinc-800 dark:bg-zinc-900 dark:focus-within:bg-zinc-900"
        >
          <button
            class="rounded-lg p-2 text-zinc-400 hover:bg-zinc-200/60 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-200"
            title="语音收音将在 Step 3 接入"
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
            :disabled="!inputText.trim() || sending"
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
      :python-running="status.python_worker_running"
      :python-env="status.python_env"
      :worker-busy="workerBusy"
      :worker-error="workerError"
      @close="modelHubOpen = false"
      @select="handleSelectModel"
      @start-worker="handleStartWorker"
      @stop-worker="handleStopWorker"
    />

    <SettingsModal
      :open="settingsOpen"
      :settings="settings"
      :data-dir="dataDir"
      @close="settingsOpen = false"
      @update="handleUpdateSetting"
    />
  </div>
</template>
