<script setup lang="ts">
import { computed, ref } from "vue";
import {
  AlertTriangle,
  Check,
  Cloud,
  Cpu,
  Download,
  FolderOpen,
  HardDrive,
  Play,
  Sparkles,
  Square,
  Trash2,
  X,
} from "lucide-vue-next";
import type {
  DownloadProgressEvent,
  ModelCatalogItem,
  ModelCategory,
  PythonEnvInfo,
} from "../types/skippy";

const props = defineProps<{
  open: boolean;
  models: ModelCatalogItem[];
  ollamaConnected: boolean;
  ollamaModels: string[];
  pythonRunning: boolean;
  pythonEnv: PythonEnvInfo;
  llamaRuntimeReady: boolean;
  llamaServerRunning: boolean;
  llamaLoadedModel: string | null;
  voiceRuntimeReady: boolean;
  voiceBusy: boolean;
  voiceError: string;
  localStorageDir: string;
  downloadMirror: string;
  downloadProgress: Record<string, DownloadProgressEvent>;
  workerBusy: boolean;
  workerError: string;
  llamaBusy: boolean;
  llamaError: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "select", category: ModelCategory, modelId: string): void;
  (e: "install-llama"): void;
  (e: "start-llama", modelId?: string): void;
  (e: "stop-llama"): void;
  (e: "install-voice-suite"): void;
  (e: "download-voice", modelId: string): void;
  (e: "download-model", modelId: string): void;
  (e: "cancel-download"): void;
  (e: "import-gguf", modelId: string): void;
  (e: "delete-model", modelId: string): void;
  (e: "start-worker"): void;
  (e: "stop-worker"): void;
}>();

const activeTab = ref<ModelCategory>("llm");

const tabs: { key: ModelCategory; label: string; desc: string }[] = [
  { key: "kws", label: "语音唤醒", desc: "待机常驻监听或快捷键唤醒" },
  { key: "asr", label: "语音识别", desc: "将你说的话转成文字" },
  { key: "llm", label: "对话大脑", desc: "负责理解意图、对话与二期控电脑" },
  { key: "tts", label: "语音合成", desc: "把文字回复读出声音" },
];

const filteredModels = computed(() =>
  props.models.filter((m) => m.category === activeTab.value),
);

const totalEstVramMb = computed(() =>
  props.models
    .filter((m) => m.is_active)
    .reduce((sum, m) => sum + m.est_vram_mb, 0),
);

const vramPercent = computed(() =>
  Math.min(100, Math.round((totalEstVramMb.value / 12288) * 100)),
);

const vramLevel = computed(() => {
  if (totalEstVramMb.value > 11264) return "danger";
  if (totalEstVramMb.value > 9500) return "warn";
  return "ok";
});

const activeLocalLlm = computed(() =>
  props.models.find(
    (m) => m.category === "llm" && m.is_active && m.engine === "llama-server",
  ),
);

const runtimeDownload = computed(() => props.downloadProgress["llama-runtime"]);

const voiceSuiteReady = computed(() => {
  const hasKws = props.models.some((m) => m.id === "sherpa-kws-zh-en" && m.is_installed);
  const hasAsr = props.models.some((m) => m.id === "sensevoice-small-int8" && m.is_installed);
  return props.voiceRuntimeReady && hasKws && hasAsr;
});

const activeVoiceDownload = computed(() => {
  for (const key of ["sherpa-runtime", "silero-vad", "sherpa-kws-zh-en", "sensevoice-small-int8"]) {
    const item = props.downloadProgress[key];
    if (item && !item.done) return item;
  }
  return null;
});

function formatDisk(mb: number): string {
  if (mb === 0) return "0 MB";
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${mb} MB`;
}

function formatBytes(bytes: number): string {
  if (!bytes) return "0 MB";
  const mb = bytes / (1024 * 1024);
  if (mb >= 1024) return `${(mb / 1024).toFixed(2)} GB`;
  return `${mb.toFixed(1)} MB`;
}
</script>

<template>
  <div
    v-if="open"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-4 backdrop-blur-xs"
    @click.self="emit('close')"
  >
    <div
      class="flex max-h-[88vh] w-full max-w-3xl flex-col overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-2xl dark:border-zinc-800 dark:bg-zinc-900"
    >
      <!-- Header -->
      <div class="flex items-center justify-between border-b border-zinc-200 px-5 py-4 dark:border-zinc-800">
        <div>
          <h2 class="text-base font-semibold text-zinc-900 dark:text-zinc-100">
            模型管理与切换中心
          </h2>
          <p class="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">
            纯本地运行无需 Ollama；待机关闭 GUI 时显存占用为 0，存放目录：
            <span class="font-mono">{{ localStorageDir }}</span>
          </p>
        </div>
        <button
          class="rounded-lg p-1.5 text-zinc-500 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
          @click="emit('close')"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <!-- Category Tabs -->
      <div class="grid grid-cols-4 gap-2 border-b border-zinc-200 bg-zinc-50/70 px-5 py-3 dark:border-zinc-800 dark:bg-zinc-950/40">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          class="rounded-lg px-3 py-2 text-left transition"
          :class="
            activeTab === tab.key
              ? 'bg-white text-blue-600 shadow-xs ring-1 ring-zinc-200 dark:bg-zinc-800 dark:text-blue-400 dark:ring-zinc-700'
              : 'text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800/60'
          "
          @click="activeTab = tab.key"
        >
          <div class="text-xs font-semibold">{{ tab.label }}</div>
          <div class="mt-0.5 truncate text-[11px] opacity-75">{{ tab.desc }}</div>
        </button>
      </div>

      <!-- Model List -->
      <div class="flex-1 space-y-2.5 overflow-y-auto p-5">
        <div
          v-for="item in filteredModels"
          :key="item.id"
          class="flex items-start justify-between gap-4 rounded-xl border p-4 transition"
          :class="
            item.is_active
              ? 'border-blue-500/60 bg-blue-50/40 dark:border-blue-500/50 dark:bg-blue-950/20'
              : 'border-zinc-200 bg-white hover:border-zinc-300 dark:border-zinc-800 dark:bg-zinc-900 dark:hover:border-zinc-700'
          "
        >
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-2">
              <span class="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
                {{ item.name }}
              </span>
              <span
                class="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] font-medium"
                :class="
                  item.compute === 'gpu'
                    ? 'bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300'
                    : item.compute === 'cloud'
                      ? 'bg-sky-100 text-sky-800 dark:bg-sky-950/60 dark:text-sky-300'
                      : 'bg-zinc-100 text-zinc-700 dark:bg-zinc-800 dark:text-zinc-300'
                "
              >
                <Sparkles v-if="item.compute === 'gpu'" class="h-3 w-3" />
                <Cloud v-else-if="item.compute === 'cloud'" class="h-3 w-3" />
                <Cpu v-else class="h-3 w-3" />
                {{ item.compute.toUpperCase() }}
              </span>

              <span
                v-if="item.engine === 'llama-server' || item.engine === 'sherpa-onnx'"
                class="rounded-md px-1.5 py-0.5 text-[11px] font-medium"
                :class="
                  item.is_installed
                    ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300'
                    : 'bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400'
                "
              >
                {{ item.is_installed ? "权重已就绪" : item.engine === "llama-server" ? "待下载/导入" : "待下载" }}
              </span>

              <span
                v-for="tag in item.tags"
                :key="tag"
                class="rounded-md bg-zinc-100 px-1.5 py-0.5 text-[11px] text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300"
              >
                {{ tag }}
              </span>
            </div>

            <p class="mt-1.5 text-xs leading-relaxed text-zinc-600 dark:text-zinc-400">
              {{ item.description }}
            </p>

            <div class="mt-2 flex flex-wrap items-center gap-3 text-[11px] text-zinc-500 dark:text-zinc-400">
              <span class="inline-flex items-center gap-1">
                <HardDrive class="h-3 w-3" /> 磁盘 {{ formatDisk(item.disk_size_mb) }}
              </span>
              <span>
                预估显存 {{ item.est_vram_mb === 0 ? "0 MB" : `${(item.est_vram_mb / 1024).toFixed(1)} GB` }}
              </span>
              <span
                v-if="item.local_path"
                class="max-w-xs truncate font-mono text-[10px] text-zinc-400"
                :title="item.local_path"
              >
                {{ item.local_path }}
              </span>
              <span
                v-if="item.requires_ollama && !ollamaConnected"
                class="inline-flex items-center gap-1 text-amber-600 dark:text-amber-400"
              >
                <AlertTriangle class="h-3 w-3" /> 未检测到本机 Ollama
              </span>
              <span
                v-else-if="item.requires_ollama && ollamaConnected"
                class="inline-flex items-center gap-1 text-emerald-600 dark:text-emerald-400"
              >
                已检测到模型：{{ ollamaModels.slice(0, 3).join(", ") || "空" }}
              </span>
              <span
                v-if="item.requires_python && !pythonEnv.venv_ready"
                class="inline-flex items-center gap-1 text-amber-600 dark:text-amber-400"
              >
                <AlertTriangle class="h-3 w-3" /> 需要配置 worker/.venv
              </span>
              <span
                v-else-if="item.requires_python && pythonEnv.venv_ready && !pythonRunning"
                class="inline-flex items-center gap-1 text-zinc-500 dark:text-zinc-400"
              >
                Python 已就绪（待机休眠中，调用时自动拉起）
              </span>
            </div>

            <!-- GGUF download progress bar -->
            <div
              v-if="downloadProgress[item.id] && !downloadProgress[item.id].done"
              class="mt-2.5 space-y-1 rounded-lg border border-blue-200 bg-blue-50/50 p-2 dark:border-blue-900/50 dark:bg-blue-950/30"
            >
              <div class="flex items-center justify-between text-[11px] text-blue-700 dark:text-blue-300">
                <span>
                  {{ downloadProgress[item.id].message }}
                  ({{ formatBytes(downloadProgress[item.id].downloaded_bytes) }}
                  <template v-if="downloadProgress[item.id].total_bytes">
                    / {{ formatBytes(downloadProgress[item.id].total_bytes) }}
                  </template>)
                </span>
                <div class="flex items-center gap-2">
                  <span>{{ downloadProgress[item.id].percent }}%</span>
                  <button
                    class="text-red-600 hover:underline dark:text-red-400"
                    @click="emit('cancel-download')"
                  >
                    取消
                  </button>
                </div>
              </div>
              <div class="h-1.5 w-full overflow-hidden rounded-full bg-blue-100 dark:bg-blue-950">
                <div
                  class="h-full rounded-full bg-blue-600 transition-all"
                  :style="{ width: `${downloadProgress[item.id].percent}%` }"
                />
              </div>
            </div>
            <p
              v-else-if="downloadProgress[item.id]?.error"
              class="mt-1.5 text-[11px] text-red-600 dark:text-red-400"
            >
              {{ downloadProgress[item.id]?.error }}
            </p>
          </div>

          <div class="flex shrink-0 flex-col items-end gap-1.5">
            <button
              class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition"
              :class="
                item.is_active
                  ? 'bg-blue-600 text-white'
                  : 'border border-zinc-200 bg-white text-zinc-700 hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200 dark:hover:bg-zinc-700'
              "
              @click="emit('select', item.category, item.id)"
            >
              <Check v-if="item.is_active" class="h-3.5 w-3.5" />
              {{ item.is_active ? "已激活" : "切换使用" }}
            </button>

            <div v-if="item.engine === 'llama-server'" class="flex items-center gap-1">
              <button
                v-if="item.downloadable && !item.is_installed"
                :disabled="Boolean(downloadProgress[item.id] && !downloadProgress[item.id].done)"
                class="inline-flex items-center gap-1 rounded-md border border-zinc-200 bg-zinc-50 px-2 py-1 text-[11px] text-zinc-700 hover:bg-zinc-100 disabled:opacity-40 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200"
                :title="`从 ${downloadMirror} 下载到 ${localStorageDir}`"
                @click="emit('download-model', item.id)"
              >
                <Download class="h-3 w-3" /> 下载
              </button>
              <button
                class="inline-flex items-center gap-1 rounded-md border border-zinc-200 bg-zinc-50 px-2 py-1 text-[11px] text-zinc-700 hover:bg-zinc-100 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200"
                title="选择本机已有的 .gguf 模型文件"
                @click="emit('import-gguf', item.id)"
              >
                <FolderOpen class="h-3 w-3" /> 导入
              </button>
              <button
                v-if="item.is_installed"
                class="rounded-md border border-zinc-200 p-1 text-zinc-400 hover:border-red-300 hover:text-red-500 dark:border-zinc-700"
                title="移除模型关联/删除已下载权重"
                @click="emit('delete-model', item.id)"
              >
                <Trash2 class="h-3 w-3" />
              </button>
            </div>

            <div v-else-if="item.engine === 'sherpa-onnx'" class="flex items-center gap-1">
              <button
                v-if="item.downloadable && !item.is_installed"
                :disabled="Boolean(downloadProgress[item.id] && !downloadProgress[item.id].done)"
                class="inline-flex items-center gap-1 rounded-md border border-zinc-200 bg-zinc-50 px-2 py-1 text-[11px] text-zinc-700 hover:bg-zinc-100 disabled:opacity-40 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200"
                :title="`下载并解压到 ${localStorageDir}\\models\\sherpa`"
                @click="emit('download-voice', item.id)"
              >
                <Download class="h-3 w-3" /> 下载
              </button>
              <button
                v-if="item.is_installed"
                class="rounded-md border border-zinc-200 p-1 text-zinc-400 hover:border-red-300 hover:text-red-500 dark:border-zinc-700"
                title="删除已下载的语音模型目录"
                @click="emit('delete-model', item.id)"
              >
                <Trash2 class="h-3 w-3" />
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Footer Runtime Bars & VRAM Estimator -->
      <div class="space-y-2.5 border-t border-zinc-200 bg-zinc-50/80 px-5 py-3.5 dark:border-zinc-800 dark:bg-zinc-950/60">
        <!-- Built-in llama-server CUDA Runtime -->
        <div class="space-y-1.5 rounded-lg border border-zinc-200 bg-white px-3 py-2 text-xs dark:border-zinc-800 dark:bg-zinc-900">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div class="flex items-center gap-2">
              <span
                class="h-2 w-2 rounded-full"
                :class="
                  llamaServerRunning
                    ? 'bg-emerald-500'
                    : llamaRuntimeReady
                      ? 'bg-zinc-400'
                      : 'bg-amber-500'
                "
              />
              <span class="font-medium text-zinc-800 dark:text-zinc-200">
                本地大脑引擎 (llama-server CUDA)：
                <template v-if="llamaServerRunning">
                  运行中 · {{ llamaLoadedModel || "已加载" }}（空闲自动释放）
                </template>
                <template v-else-if="llamaRuntimeReady">
                  已就绪（待机 0 显存，发消息自动拉起）
                </template>
                <template v-else>尚未安装运行时（免装 Ollama，约 480 MB）</template>
              </span>
            </div>

            <div class="flex items-center gap-2">
              <button
                v-if="!llamaRuntimeReady"
                :disabled="Boolean(runtimeDownload && !runtimeDownload.done)"
                class="inline-flex items-center gap-1 rounded-md bg-blue-600 px-2.5 py-1 text-[11px] font-medium text-white hover:bg-blue-700 disabled:opacity-40"
                @click="emit('install-llama')"
              >
                <Download class="h-3 w-3" />
                {{ runtimeDownload && !runtimeDownload.done ? "安装中..." : "一键安装引擎" }}
              </button>
              <button
                v-else-if="!llamaServerRunning"
                :disabled="llamaBusy || !activeLocalLlm?.is_installed"
                class="inline-flex items-center gap-1 rounded-md border border-zinc-200 bg-zinc-50 px-2.5 py-1 text-[11px] font-medium text-zinc-700 hover:bg-zinc-100 disabled:opacity-40 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200 dark:hover:bg-zinc-700"
                @click="emit('start-llama', activeLocalLlm?.id)"
              >
                <Play class="h-3 w-3" />
                {{ llamaBusy ? "加载模型中..." : "预热模型" }}
              </button>
              <button
                v-else
                :disabled="llamaBusy"
                class="inline-flex items-center gap-1 rounded-md border border-red-200 bg-red-50 px-2.5 py-1 text-[11px] font-medium text-red-700 hover:bg-red-100 disabled:opacity-40 dark:border-red-900/50 dark:bg-red-950/40 dark:text-red-300"
                @click="emit('stop-llama')"
              >
                <Square class="h-3 w-3" />
                {{ llamaBusy ? "释放中..." : "立即释放" }}
              </button>
            </div>
          </div>

          <div
            v-if="runtimeDownload && !runtimeDownload.done"
            class="space-y-1 pt-1"
          >
            <div class="flex items-center justify-between text-[11px] text-blue-600 dark:text-blue-400">
              <span>{{ runtimeDownload.message }}</span>
              <div class="flex items-center gap-2">
                <span>{{ runtimeDownload.percent }}%</span>
                <button class="text-red-500 hover:underline" @click="emit('cancel-download')">
                  取消
                </button>
              </div>
            </div>
            <div class="h-1.5 w-full overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
              <div
                class="h-full rounded-full bg-blue-600 transition-all"
                :style="{ width: `${runtimeDownload.percent}%` }"
              />
            </div>
          </div>
          <p v-if="llamaError || runtimeDownload?.error" class="text-[11px] text-red-600 dark:text-red-400">
            {{ llamaError || runtimeDownload?.error }}
          </p>
        </div>

        <!-- Local Voice Suite (Sherpa-ONNX KWS + VAD + SenseVoice) -->
        <div class="space-y-1.5 rounded-lg border border-zinc-200 bg-white px-3 py-2 text-xs dark:border-zinc-800 dark:bg-zinc-900">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div class="flex items-center gap-2">
              <span
                class="h-2 w-2 rounded-full"
                :class="
                  voiceSuiteReady
                    ? 'bg-emerald-500'
                    : voiceRuntimeReady
                      ? 'bg-blue-500'
                      : 'bg-amber-500'
                "
              />
              <span class="font-medium text-zinc-800 dark:text-zinc-200">
                本地语音引擎 (Sherpa-ONNX 唤醒 + VAD + SenseVoice)：
                <template v-if="voiceSuiteReady">已就绪（离线唤醒/识别可用，纯 CPU 0 显存）</template>
                <template v-else-if="voiceRuntimeReady">运行库已装，待下载唤醒与识别模型</template>
                <template v-else>待安装（约 210 MB，纯 CPU 0 显存）</template>
              </span>
            </div>

            <div class="flex items-center gap-2">
              <button
                v-if="!voiceSuiteReady"
                :disabled="voiceBusy || Boolean(activeVoiceDownload)"
                class="inline-flex items-center gap-1 rounded-md bg-blue-600 px-2.5 py-1 text-[11px] font-medium text-white hover:bg-blue-700 disabled:opacity-40"
                @click="emit('install-voice-suite')"
              >
                <Download class="h-3 w-3" />
                {{ voiceBusy || activeVoiceDownload ? "安装语音套件中..." : "一键安装语音套件" }}
              </button>
              <span
                v-else
                class="rounded-md bg-emerald-50 px-2 py-0.5 text-[11px] font-medium text-emerald-700 dark:bg-emerald-950/50 dark:text-emerald-300"
              >
                离线语音就绪
              </span>
            </div>
          </div>

          <div
            v-if="activeVoiceDownload"
            class="space-y-1 pt-1"
          >
            <div class="flex items-center justify-between text-[11px] text-blue-600 dark:text-blue-400">
              <span>{{ activeVoiceDownload.message }}</span>
              <div class="flex items-center gap-2">
                <span>{{ activeVoiceDownload.percent }}%</span>
                <button class="text-red-500 hover:underline" @click="emit('cancel-download')">
                  取消
                </button>
              </div>
            </div>
            <div class="h-1.5 w-full overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
              <div
                class="h-full rounded-full bg-blue-600 transition-all"
                :style="{ width: `${activeVoiceDownload.percent}%` }"
              />
            </div>
          </div>
          <p v-if="voiceError" class="text-[11px] text-red-600 dark:text-red-400">
            {{ voiceError }}
          </p>
        </div>

        <!-- On-demand Python AI Worker -->
        <div class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-white px-3 py-2 text-xs dark:border-zinc-800 dark:bg-zinc-900">
          <div class="flex items-center gap-2">
            <span
              class="h-2 w-2 rounded-full"
              :class="pythonRunning ? 'bg-emerald-500' : pythonEnv.venv_ready ? 'bg-zinc-400' : 'bg-amber-500'"
            />
            <span class="font-medium text-zinc-800 dark:text-zinc-200">
              按需 Python AI Worker（重型语音/视觉预留）：
              <template v-if="pythonRunning">运行中（空闲自动回收）</template>
              <template v-else-if="pythonEnv.venv_ready">
                待机休眠 · {{ pythonEnv.python_version || "Python" }} 已就绪
              </template>
              <template v-else>未检测到 worker/.venv</template>
            </span>
          </div>

          <div class="flex items-center gap-2">
            <button
              v-if="!pythonRunning"
              :disabled="workerBusy || !pythonEnv.venv_ready"
              class="inline-flex items-center gap-1 rounded-md border border-zinc-200 bg-zinc-50 px-2.5 py-1 text-[11px] font-medium text-zinc-700 hover:bg-zinc-100 disabled:opacity-40 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200 dark:hover:bg-zinc-700"
              @click="emit('start-worker')"
            >
              <Play class="h-3 w-3" />
              {{ workerBusy ? "启动中..." : "预热启动" }}
            </button>
            <button
              v-else
              :disabled="workerBusy"
              class="inline-flex items-center gap-1 rounded-md border border-red-200 bg-red-50 px-2.5 py-1 text-[11px] font-medium text-red-700 hover:bg-red-100 disabled:opacity-40 dark:border-red-900/50 dark:bg-red-950/40 dark:text-red-300"
              @click="emit('stop-worker')"
            >
              <Square class="h-3 w-3" />
              {{ workerBusy ? "释放中..." : "立即释放" }}
            </button>
          </div>
        </div>
        <p v-if="workerError" class="text-[11px] text-red-600 dark:text-red-400">{{ workerError }}</p>

        <!-- VRAM Bar -->
        <div>
          <div class="flex items-center justify-between text-xs">
            <span class="font-medium text-zinc-700 dark:text-zinc-300">
              唤醒后预计峰值显存（RTX 5070 · 12 GB 上限）
            </span>
            <span
              class="font-semibold"
              :class="{
                'text-emerald-600 dark:text-emerald-400': vramLevel === 'ok',
                'text-amber-600 dark:text-amber-400': vramLevel === 'warn',
                'text-red-600 dark:text-red-400': vramLevel === 'danger',
              }"
            >
              {{ (totalEstVramMb / 1024).toFixed(1) }} / 12.0 GB ({{ vramPercent }}%)
            </span>
          </div>
          <div class="mt-2 h-2 w-full overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-800">
            <div
              class="h-full rounded-full transition-all duration-300"
              :class="{
                'bg-emerald-500': vramLevel === 'ok',
                'bg-amber-500': vramLevel === 'warn',
                'bg-red-500': vramLevel === 'danger',
              }"
              :style="{ width: `${vramPercent}%` }"
            />
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
