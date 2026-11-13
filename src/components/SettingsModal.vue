<script setup lang="ts">
import { X } from "lucide-vue-next";

const props = defineProps<{
  open: boolean;
  settings: Record<string, string>;
  dataDir: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "update", key: string, value: string): void;
}>();
</script>

<template>
  <div
    v-if="open"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-4 backdrop-blur-xs"
    @click.self="emit('close')"
  >
    <div
      class="flex max-h-[85vh] w-full max-w-xl flex-col overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-2xl dark:border-zinc-800 dark:bg-zinc-900"
    >
      <div class="flex items-center justify-between border-b border-zinc-200 px-5 py-4 dark:border-zinc-800">
        <h2 class="text-base font-semibold text-zinc-900 dark:text-zinc-100">Skippy 设置</h2>
        <button
          class="rounded-lg p-1.5 text-zinc-500 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
          @click="emit('close')"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <div class="flex-1 space-y-5 overflow-y-auto p-5 text-sm">
        <!-- Theme -->
        <div>
          <label class="block text-xs font-semibold text-zinc-700 dark:text-zinc-300">外观主题</label>
          <div class="mt-2 grid grid-cols-3 gap-2">
            <button
              v-for="opt in [
                { k: 'system', l: '跟随系统' },
                { k: 'light', l: '清爽浅色' },
                { k: 'dark', l: '深色模式' },
              ]"
              :key="opt.k"
              class="rounded-lg border px-3 py-2 text-xs font-medium transition"
              :class="
                settings.theme === opt.k
                  ? 'border-blue-500 bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-400'
                  : 'border-zinc-200 text-zinc-700 hover:bg-zinc-50 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800'
              "
              @click="emit('update', 'theme', opt.k)"
            >
              {{ opt.l }}
            </button>
          </div>
        </div>

        <!-- Persona -->
        <div>
          <label class="block text-xs font-semibold text-zinc-700 dark:text-zinc-300">Skippy 说话风格</label>
          <div class="mt-2 grid grid-cols-2 gap-2">
            <button
              v-for="opt in [
                { k: 'butler', l: '冷静管家模式' },
                { k: 'skippy_love', l: '爱死你了模式 (毒舌整活)' },
              ]"
              :key="opt.k"
              class="rounded-lg border px-3 py-2 text-xs font-medium transition"
              :class="
                settings.persona_mode === opt.k
                  ? 'border-blue-500 bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-400'
                  : 'border-zinc-200 text-zinc-700 hover:bg-zinc-50 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800'
              "
              @click="emit('update', 'persona_mode', opt.k)"
            >
              {{ opt.l }}
            </button>
          </div>
        </div>

        <!-- Idle Unload -->
        <div>
          <label class="block text-xs font-semibold text-zinc-700 dark:text-zinc-300">
            空闲自动释放 GPU 模型时间（秒）
          </label>
          <select
            :value="settings.idle_unload_secs || '180'"
            class="mt-2 w-full rounded-lg border border-zinc-200 bg-white px-3 py-2 text-xs text-zinc-900 outline-none focus:border-blue-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
            @change="emit('update', 'idle_unload_secs', ($event.target as HTMLSelectElement).value)"
          >
            <option value="60">1 分钟（打游戏推荐）</option>
            <option value="180">3 分钟（默认均衡）</option>
            <option value="300">5 分钟</option>
            <option value="0">不自动卸载</option>
          </select>
        </div>

        <!-- Cloud API -->
        <div class="space-y-3 rounded-xl border border-zinc-200 p-3.5 dark:border-zinc-800">
          <div class="text-xs font-semibold text-zinc-800 dark:text-zinc-200">
            云端 OpenAI 兼容接口（Step 2 启用对话）
          </div>
          <div>
            <label class="block text-[11px] text-zinc-500 dark:text-zinc-400">Base URL</label>
            <input
              type="text"
              :value="settings.cloud_base_url || ''"
              class="mt-1 w-full rounded-lg border border-zinc-200 bg-white px-2.5 py-1.5 text-xs text-zinc-900 outline-none focus:border-blue-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
              @change="emit('update', 'cloud_base_url', ($event.target as HTMLInputElement).value)"
            />
          </div>
          <div class="grid grid-cols-2 gap-2">
            <div>
              <label class="block text-[11px] text-zinc-500 dark:text-zinc-400">Model ID</label>
              <input
                type="text"
                :value="settings.cloud_model || ''"
                class="mt-1 w-full rounded-lg border border-zinc-200 bg-white px-2.5 py-1.5 text-xs text-zinc-900 outline-none focus:border-blue-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
                @change="emit('update', 'cloud_model', ($event.target as HTMLInputElement).value)"
              />
            </div>
            <div>
              <label class="block text-[11px] text-zinc-500 dark:text-zinc-400">API Key</label>
              <input
                type="password"
                :value="settings.cloud_api_key || ''"
                placeholder="sk-..."
                class="mt-1 w-full rounded-lg border border-zinc-200 bg-white px-2.5 py-1.5 text-xs text-zinc-900 outline-none focus:border-blue-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
                @change="emit('update', 'cloud_api_key', ($event.target as HTMLInputElement).value)"
              />
            </div>
          </div>
        </div>

        <!-- Local Data Path -->
        <div class="rounded-lg bg-zinc-50 px-3 py-2 text-[11px] text-zinc-500 dark:bg-zinc-950/60 dark:text-zinc-400">
          数据与模型目录（不入 Git）：<span class="font-mono">{{ dataDir }}</span>
        </div>
      </div>
    </div>
  </div>
</template>
