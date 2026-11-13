<script setup lang="ts">
import { computed, ref } from "vue";
import { Check, Cpu, Cloud, HardDrive, Sparkles, X, AlertTriangle } from "lucide-vue-next";
import type { ModelCatalogItem, ModelCategory } from "../types/skippy";

const props = defineProps<{
  open: boolean;
  models: ModelCatalogItem[];
  ollamaConnected: boolean;
  pythonRunning: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "select", category: ModelCategory, modelId: string): void;
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

function formatDisk(mb: number): string {
  if (mb === 0) return "0 MB";
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${mb} MB`;
}
</script>

<template>
  <div
    v-if="open"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-4 backdrop-blur-xs"
    @click.self="emit('close')"
  >
    <div
      class="flex max-h-[85vh] w-full max-w-3xl flex-col overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-2xl dark:border-zinc-800 dark:bg-zinc-900"
    >
      <!-- Header -->
      <div class="flex items-center justify-between border-b border-zinc-200 px-5 py-4 dark:border-zinc-800">
        <div>
          <h2 class="text-base font-semibold text-zinc-900 dark:text-zinc-100">
            模型管理与切换中心
          </h2>
          <p class="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">
            按功能环节切换当前激活引擎；待机关闭 GUI 时显存占用为 0
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
                v-if="item.requires_ollama && !ollamaConnected"
                class="inline-flex items-center gap-1 text-amber-600 dark:text-amber-400"
              >
                <AlertTriangle class="h-3 w-3" /> 未检测到本机 Ollama（Step 2 接入检测）
              </span>
              <span
                v-if="item.requires_python && !pythonRunning"
                class="inline-flex items-center gap-1 text-amber-600 dark:text-amber-400"
              >
                <AlertTriangle class="h-3 w-3" /> 需主力机 Python Worker
              </span>
            </div>
          </div>

          <div class="shrink-0">
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
          </div>
        </div>
      </div>

      <!-- Footer VRAM Estimator -->
      <div class="border-t border-zinc-200 bg-zinc-50/80 px-5 py-3.5 dark:border-zinc-800 dark:bg-zinc-950/60">
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
</template>
