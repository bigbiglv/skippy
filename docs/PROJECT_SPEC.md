# Skippy 项目规格与分期技术方案

> 本文档用于跨会话、跨电脑同步项目上下文。新会话接手开发前，请优先阅读本文档与 [`docs/TODO.md`](./TODO.md)。

---

## 1. 项目定位与核心目标

- **项目名称**：`skippy`（灵感源自《赛博朋克 2077》智能手枪四季宝）
- **目标平台**：Windows 10 / 11
- **GitHub 仓库**：`https://github.com/bigbiglv/skippy.git`
- **本地目录**：`D:\skippy`
- **硬件目标环境**：主力机为 **AMD 7800X3D + RTX 5070 (12GB VRAM)**；当前初始化开发机已装 `Git`、`Node.js v22`、`pnpm`、`Rust/Cargo 1.96`，暂未安装 `Python` 与 `Ollama`（后续在主力机补齐）。

### 不可妥协的核心约束
1. **收起/待机极致低占用**：用户关闭 GUI 窗口后，必须销毁 WebView 渲染进程（`ExitRequested` + `prevent_exit()` 保留托盘），仅保留 Rust 核心常驻系统托盘，待机目标：内存约 **40~80 MB**、CPU **~1%**、GPU 显存 **0 MB**（不影响日常打游戏）。
2. **唤醒后满血按需调度**：语音唤醒或手机发指令后，允许按需热拉起本地大模型（Ollama）或重型 GPU 语音 Worker（Python Sidecar），空闲超时（如 2~3 分钟）自动卸载显存或终止 Worker 进程。
3. **跨设备与可维护性**：所有会话历史、状态机、模型配置与未来局域网服务全部由 Rust 后端持有，GUI 只是按需打开的展示/交互壳。

---

## 2. 技术栈总览

| 模块 | 选型 | 备注 |
| :--- | :--- | :--- |
| **桌面端与常驻内核** | **Tauri v2 (Rust)** | 单实例、托盘常驻、全局热键、关闭销毁 WebView |
| **前端 UI** | **Vue 3 + TypeScript + Vite + Tailwind CSS** | 清爽浅色 / 跟随系统（偏原生 Windows 11 风格） |
| **本地数据库** | **SQLite (`rusqlite`)** | 会话、消息历史、模型配置与系统设置 |
| **音频收发** | **`cpal` (WASAPI 事件驱动) + `rodio`** | 低功耗收音、环形缓冲、Audio Ducking、播报打断 |
| **轻语音引擎 (一期内嵌)** | **`sherpa-onnx` + `Edge-TTS` / `WinRT TTS`** | KWS 唤醒 + Silero VAD + 声纹 + CPU `SenseVoice`/`Paraformer` + 零占用/离线兜底 TTS |
| **大模型接入** | **`Ollama` API + OpenAI 兼容云端 API** | 自动同步已拉取模型、支持 `pull` 下载与 `keep_alive` 卸载 |
| **重型 GPU 语音 / 视觉 (按需 Worker)** | **Python Sidecar (FastAPI/HTTP)** | 预留用于 `Qwen3-ASR`、`CosyVoice`/`Index-TTS`、以及二期视觉兜底 |
| **Windows 操控 (二期)** | **Rust `uiautomation` / `windows-rs` + `AppsFolder` + SMTC** | 混合分层：系统 API 优先 -> UIA 控件树 -> Set-of-Mark 视觉兜底 |
| **局域网手机端 (三期)** | **Rust `axum` (HTTP + WebSocket)** | 内嵌于常驻内核，扫码配对 + Token 鉴权 + 手机网页复用 Vue 界面 |

---

## 3. 分期需求与对应技术方案

### 第一期：语音唤醒对话 + 对话历史 GUI + 模型管理中心（当前阶段）

#### 1.1 后台常驻与低占用机制
- 窗口关闭时原生销毁 WebView，在 `RunEvent::ExitRequested` 中通过 `api.prevent_exit()` 保留 Rust 托盘进程；左键点击托盘图标、托盘菜单或按下全局快捷键 `Alt + Space` 可按需重建窗口。
- 唤醒词模式支持切换：
  - **常驻唤醒词监听**：WASAPI 事件驱动收音 + `sherpa-onnx` KWS + Silero VAD + 可选声纹验证（防视频/音乐误触发）。
  - **纯快捷键模式**：完全关闭麦克风监听（不触发 Windows 任务栏麦克风隐私橙色小图标）。
- **并发预热**：检测到唤醒词瞬间，一边提示音 + Audio Ducking 压低背景音乐 + 本地环形缓冲录音，一边异步预热 LLM / Worker。
- **语音打断 (Barge-in)**：播报过程中再次唤醒或插话时，立即清空 `rodio` 播放队列并切回聆听态。
- **空闲自动卸载 (Idle TTL)**：无交互超时自动向 Ollama 发送 `keep_alive: 0` 并终止按需 Python Worker。

#### 1.2 GUI 页面规划
1. **常规 AI 对话页 (`App.vue`)**
   - 左侧可折叠历史会话栏 + 右侧消息流。
   - 消息气泡标注触发来源（`键盘` / `语音唤醒` / `手机遥控` / `系统`）与使用的模型标签，并预留二期 `tool_calls_json` 执行步骤折叠卡片。
   - 顶部状态胶囊展示后台状态（`待机低占用 / 监听中 / 思考中 / 播报中`）。
   - 底部支持文本输入与手动按住说话入口。
2. **模型管理与切换中心 (`ModelHubModal.vue`)**
   - 按 **4 个功能分类** 管理与单选切换：
     - **语音唤醒 (`kws`)**：仅快捷键模式 / `sherpa-onnx Zipformer KWS` / `openWakeWord`
     - **语音识别 (`asr`)**：`SenseVoice-Small (INT8, CPU)` / `Paraformer-zh-streaming (CPU)` / `Qwen3-ASR (0.6B/1.7B, GPU Worker)` / 云端兼容 ASR
     - **对话大脑 (`llm`)**：本地 Ollama（自动同步 `/api/tags` + 推荐清单一键 `pull` + 预留二期 `[视觉控电脑]` 如 `UI-TARS-1.5-7B`） / 云端 OpenAI 兼容 API（DeepSeek / Qwen / OpenAI / Gemini 等）
     - **语音合成 (`tts`)**：`Edge-TTS`（在线免部署）/ `Windows WinRT 原生 TTS` / `Kokoro-82M (CPU 离线)` / `CosyVoice & Index-TTS (GPU Worker 声音克隆)`
   - 底部提供 **RTX 5070 12GB 显存预算预估条** 与 **闲置自动卸载时间设置**。

#### 1.3 无 Python / 无 Ollama 环境下的降级运行策略
- **没有 Ollama 时**：使用云端 OpenAI 兼容 API 跑通完整对话、流式输出、语音唤醒与 TTS 播报闭环。
- **没有 Python 时**：使用纯 Rust 的 `sherpa-onnx`（唤醒 + `SenseVoice` CPU 识别）+ `Edge-TTS` / `WinRT TTS` / `Kokoro-82M` 跑通完整语音链路；GPU 重型语音模型标记为“需主力机 Python Worker”。

---

### 第二期：接入 PC 操作大模型（让 Skippy 干活）

**典型目标场景**：*“打开 Apple Music -> 点击资源库 -> 随便播放一首歌 -> 切换随机播放”*。

1. **第一优先级：系统原生捷径（毫秒级、零视觉）**
   - 启动软件：扫描 `shell:AppsFolder` (AUMID，通吃商店/MSIX 应用如 Apple Music)、开始菜单 `.lnk` 与注册表 `App Paths`，提供 `open_app(name)` 工具；找不到降级模拟 `Win` 键搜索。
   - 媒体与系统控制：Windows SMTC（播放/暂停/切歌）、音量/锁屏/进程管理。
2. **第二优先级：Windows UI Automation (UIA) 控件树（百毫秒级）**
   - Rust `uiautomation` / `windows-rs`（运行在 `spawn_blocking` STA 上下文），裁剪过滤不可见与非交互节点，输出精简带编号控件清单给大模型做选择。
3. **第三优先级：Set-of-Mark 视觉标注 + 视觉模型兜底**
   - 遇到无障碍标签缺失的图标/自绘软件/Electron 应用时，结合 UIA 框选画编号截图（或 `OmniParser`）喂给 `UI-TARS-1.5-7B` / `Qwen2.5-VL-7B`。
4. **安全与效率**：高危操作强制语音 + 弹窗二次确认；高频操作序列固化为本地 Skill 缓存。

---

### 第三期：局域网手机网页遥控

- **内嵌服务**：在 Rust 常驻内核引入 `axum` 启动 HTTP + WebSocket 服务（监听 `0.0.0.0:端口`）。
- **配对与安全**：PC 端生成局域网二维码 + 一次性配对码 / Token 鉴权。
- **手机端自适应 Web UI**：同步 PC 对话历史与状态；支持打字及**手机浏览器按住说话**推送近场清晰音频给 PC ASR；二期高危操作同步推至手机端审批。

---

## 4. 关键架构决策记录 (ADR)

1. **为什么不用现成开源语音助手仓库魔改？**：成品多为纯 Python（无法实现关闭 GUI 后几十 MB 常驻）或绑定 Linux (`systemd`/`D-Bus`)，改造成本高于使用 Tauri v2 搭干净底座并按需调用开源库/引擎。
2. **为什么选纯 Rust `sherpa-onnx` + Ollama + 可选按需 Python Worker？**：平时待机内存极低且无 Python 环境污染；主力机跑重型 GPU 语音/视觉时又能通过子进程“用时拉起、用完即杀”彻底回收显存。
3. **跨电脑开发约定**：模型权重、SQLite 数据库与 API Key 均存放在系统用户本地数据目录（如 `%LocalAppData%\com.bigbiglv.skippy\`），不入 Git 仓库。
