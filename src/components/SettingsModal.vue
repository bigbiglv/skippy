<script setup lang="ts">
import { FolderOpen, X } from "lucide-vue-next";

const props = defineProps<{
  open: boolean;
  settings: Record<string, string>;
  dataDir: string;
  localStorageDir: string;
  ollamaConnected: boolean;
  ollamaModels: string[];
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "update", key: string, value: string): void;
  (e: "pick-storage-dir"): void;
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

        <!-- Voice Wake & Speech Settings -->
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="block text-xs font-semibold text-zinc-700 dark:text-zinc-300">语音唤醒词</label>
            <input
              type="text"
              :value="settings.wake_word || '密探阿泰'"
              placeholder="密探阿泰"
              class="mt-2 w-full rounded-lg border border-zinc-200 bg-white px-3 py-2 text-xs text-zinc-900 outline-none focus:border-blue-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
              @change="emit('update', 'wake_word', ($event.target as HTMLInputElement).value)"
            />
          </div>
          <div>
            <label class="block text-xs font-semibold text-zinc-700 dark:text-zinc-300">语音问答播报</label>
            <div class="mt-2 grid grid-cols-2 gap-1.5">
              <button
                v-for="opt in [
                  { k: 'true', l: '朗读回复' },
                  { k: 'false', l: '静音文字' },
                ]"
                :key="opt.k"
                class="rounded-lg border px-2 py-2 text-xs font-medium transition"
                :class="
                  (settings.voice_reply_enabled ?? 'true') === opt.k
                    ? 'border-blue-500 bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-400'
                    : 'border-zinc-200 text-zinc-700 hover:bg-zinc-50 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800'
                "
                @click="emit('update', 'voice_reply_enabled', opt.k)"
              >
                {{ opt.l }}
              </button>
            </div>
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

        <!-- Local Storage & Mirrors -->
        <div class="space-y-3 rounded-xl border border-zinc-200 p-3.5 dark:border-zinc-800">
          <div class="text-xs font-semibold text-zinc-800 dark:text-zinc-200">
            本地引擎与模型存放位置（推荐 D 盘）
          </div>
          <div class="flex gap-2">
            <input
              type="text"
              :value="settings.local_storage_dir || localStorageDir"
              class="flex-1 rounded-lg border border-zinc-200 bg-white px-2.5 py-1.5 font-mono text-xs text-zinc-900 outline-none focus:border-blue-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
              @change="emit('update', 'local_storage_dir', ($event.target as HTMLInputElement).value)"
            />
            <button
              class="inline-flex items-center gap-1 rounded-lg border border-zinc-200 bg-zinc-50 px-2.5 py-1.5 text-xs font-medium text-zinc-700 hover:bg-zinc-100 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200 dark:hover:bg-zinc-700"
              @click="emit('pick-storage-dir')"
            >
              <FolderOpen class="h-3.5 w-3.5" />
              选择目录
            </button>
          </div>

          <div class="grid grid-cols-2 gap-2">
            <div>
              <label class="block text-[11px] text-zinc-500 dark:text-zinc-400">GGUF 权重下载镜像</label>
              <select
                :value="settings.download_mirror || 'modelscope'"
                class="mt-1 w-full rounded-lg border border-zinc-200 bg-white px-2.5 py-1.5 text-xs text-zinc-900 outline-none focus:border-blue-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
                @change="emit('update', 'download_mirror', ($event.target as HTMLSelectElement).value)"
              >
                <option value="modelscope">魔搭 ModelScope（国内高速）</option>
                <option value="hf-mirror">HF-Mirror 镜像</option>
                <option value="huggingface">HuggingFace 官方</option>
              </select>
            </div>
            <div>
              <label class="block text-[11px] text-zinc-500 dark:text-zinc-400">GitHub 加速前缀（可选）</label>
              <input
                type="text"
                :value="settings.github_mirror || ''"
                placeholder="直连留空，例如 https://gh-proxy.com"
                class="mt-1 w-full rounded-lg border border-zinc-200 bg-white px-2.5 py-1.5 text-xs text-zinc-900 outline-none focus:border-blue-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
                @change="emit('update', 'github_mirror', ($event.target as HTMLInputElement).value)"
              />
            </div>
          </div>
        </div>

        <!-- Optional Ollama Reuse -->
        <div class="space-y-3 rounded-xl border border-zinc-200 p-3.5 dark:border-zinc-800">
          <div class="flex items-center justify-between text-xs">
            <span class="font-semibold text-zinc-800 dark:text-zinc-200">
              本机已有 Ollama（可选复用）
            </span>
            <span
              class="text-[11px]"
              :class="ollamaConnected ? 'text-emerald-600 dark:text-emerald-400' : 'text-zinc-400'"
            >
              {{ ollamaConnected ? `已检测到 (${ollamaModels.length} 个模型)` : "未检测到运行中实例" }}
            </span>
          </div>
          <div class="grid grid-cols-2 gap-2">
            <div>
              <label class="block text-[11px] text-zinc-500 dark:text-zinc-400">Ollama 地址</label>
              <input
                type="text"
                :value="settings.ollama_base_url || 'http://127.0.0.1:11434'"
                class="mt-1 w-full rounded-lg border border-zinc-200 bg-white px-2.5 py-1.5 text-xs text-zinc-900 outline-none focus:border-blue-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
                @change="emit('update', 'ollama_base_url', ($event.target as HTMLInputElement).value)"
              />
            </div>
            <div>
              <label class="block text-[11px] text-zinc-500 dark:text-zinc-400">Ollama 模型名（留空自动选）</label>
              <input
                type="text"
                :value="settings.ollama_model || ''"
                :placeholder="ollamaModels[0] || '例如 qwen3:8b'"
                class="mt-1 w-full rounded-lg border border-zinc-200 bg-white px-2.5 py-1.5 text-xs text-zinc-900 outline-none focus:border-blue-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
                @change="emit('update', 'ollama_model', ($event.target as HTMLInputElement).value)"
              />
            </div>
          </div>
        </div>

        <!-- Optional Cloud API -->
        <details class="rounded-xl border border-zinc-200 p-3.5 dark:border-zinc-800">
          <summary class="cursor-pointer text-xs font-semibold text-zinc-700 dark:text-zinc-300">
            云端 OpenAI 兼容接口（可选 · 纯本地模式下无需填写）
          </summary>
          <div class="mt-3 space-y-3">
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
        </details>

        <!-- Local Data Path -->
        <div class="rounded-lg bg-zinc-50 px-3 py-2 text-[11px] text-zinc-500 dark:bg-zinc-950/60 dark:text-zinc-400">
          数据库目录（不入 Git）：<span class="font-mono">{{ dataDir }}</span>
        </div>
      </div>
    </div>
  </div>
</template>
