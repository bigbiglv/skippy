# Skippy 开发待办清单 (TODO)

> **当前阶段**：Phase 1 - Step 2 纯本地大模型接入与流式对话已完成（内置托管 `llama-server` CUDA + D 盘 GGUF 下载/导入 + 流式对话闭环），下一步进入 Step 3（语音唤醒、识别与播报）
> **最后更新**：2026-11-13

---

## Phase 1：语音唤醒对话 + 对话历史 GUI + 模型管理中心

### Step 1：工程骨架与低占用底座（已完成）
- [x] 初始化 Tauri v2 + Vue 3 + TypeScript + Tailwind CSS 工程 (`D:\skippy`)
- [x] 配置 `.gitignore`（排除 `target/`、`node_modules/`、`*.db`、模型文件、本地密钥）与项目目录结构
- [x] 写入项目架构文档 [`docs/PROJECT_SPEC.md`](./PROJECT_SPEC.md) 与待办清单 [`docs/TODO.md`](./TODO.md)
- [x] 配置 Tauri v2 单实例 (`tauri-plugin-single-instance`)、系统托盘（打开 Skippy / 退出 Skippy）
- [x] 实现**关闭窗口销毁 WebView** (`ExitRequested` -> `prevent_exit`)，确保收起时低内存占用
- [x] 实现全局快捷键 (`Alt + Space`) 唤醒/收起窗口（`tauri-plugin-global-shortcut`）
- [x] 搭建 Vue 3 主框架（跟随系统浅色/深色主题、左侧可折叠会话栏、主对话区、右上角模型管理弹窗入口、设置弹窗入口）
- [x] 实现 SQLite 建表与基础 CRUD（`sessions`、`messages`、`model_configs`、`app_settings`）

### Step 2：纯本地大模型与流式对话闭环（已完成，兼顾可选 Ollama / 云端）
- [x] 实现统一 OpenAI 兼容流式客户端 (`src-tauri/src/llm/client.rs`) 与 `llama-server` / `Ollama` / `Cloud` 统一流式分发
- [x] 实现内置托管 **`llama-server` (`llama.cpp` CUDA) Sidecar 管理器** (`src-tauri/src/llm/llama_supervisor.rs`)：
  - [x] 支持在「模型管理」一键下载安装 Windows CUDA 12.4 版 `llama-server` 运行时至自定义目录（默认 `D:\SkippyData\runtime\llama-server\`，免装 Ollama）
  - [x] 实现按需拉起、`/health` 等待与崩溃日志捕获、15 秒轮询 Idle TTL 超时强杀回收 100% 显存
- [x] 兼容检测本机已有 **Ollama**（自动探测 `127.0.0.1:11434/api/tags` 并支持对话调用）
- [x] 完善 `App.vue`：流式打字机输出、Qwen3 `<think>` 思考过程折叠卡片、基础 Markdown 代码块渲染、停止生成、会话自动命名
- [x] 完善 `ModelHubModal.vue` 与 `SettingsModal.vue`：D 盘目录选择、GGUF 权重下载器（魔搭 ModelScope / HF-Mirror 镜像切换 + 实时进度条 + 取消）、本地 `.gguf` 文件导入与删除

### Step 3：语音唤醒、识别与播报闭环
- [ ] 音频采集模块（`cpal` WASAPI 事件驱动 + 无锁 SPSC 环形缓冲）
- [ ] 接入 `sherpa-onnx`：
  - [ ] 本地唤醒词检测（KWS，默认 `"Skippy"`，支持切换为“仅快捷键模式”）
  - [ ] Silero VAD 语音端点检测（预留语义轮次完结判断接口）
  - [ ] 可选声纹过滤（防止电脑外放视频/音乐误触发）
  - [ ] 本地 CPU 离线识别（`SenseVoice-Small INT8` / `Paraformer-zh-streaming`）
- [ ] 语音播报与播放控制：
  - [ ] `rodio` 音频队列播放 + **Barge-in 语音插话打断**
  - [ ] Windows 媒体音量压低与恢复（Audio Ducking）
  - [ ] 分句流式 TTS 管道 + 唤醒 0ms 提示音预反馈
  - [ ] 接入 `Edge-TTS`（在线免部署）、`Windows WinRT TTS`（离线兜底）、`Kokoro-82M`（CPU ONNX 离线）
- [ ] 对话历史记录自动写入语音来源 (`pc_voice`) 并实时同步到已打开的 GUI

### Step 4：按需预热与 GPU Worker 深度推理集成
- [x] 通过 `mise` 安装 Python 3.12 (`worker/.venv`) 并搭建按需 FastAPI AI Worker 服务骨架 (`worker/server.py`)
- [x] 实现 Rust `WorkerSupervisor` 按需拉起（无黑框）、健康检查与 Idle TTL 自动杀进程回收显存
- [ ] 在 `worker/requirements-gpu.txt` 安装 PyTorch CUDA 12.8+ 并下载 `Qwen3-ASR` 与 `CosyVoice` / `Index-TTS` 权重实现真实推理
- [ ] 实现内置 `llama-server` CUDA 推理与 5070 12GB 显存按需预热 + 空闲强杀回收
- [ ] 实测收起 GUI 后的常驻内存（目标 < 80MB）与待机显存占用（目标 0MB）

---

## Phase 2：接入 PC 操控大模型（让 Skippy 干活）

- [ ] 系统原生捷径工具集：
  - [ ] `AppsFolder` (AUMID) + 开始菜单 `.lnk` + 注册表 `App Paths` 应用索引器
  - [ ] `open_app(name)` 工具（支持 Apple Music 等 UWP/MSIX 无快捷方式应用，找不到降级 `Win` 键搜索）
  - [ ] Windows SMTC 媒体控制（播放/暂停/上/下一首/音量）
- [ ] Windows UI Automation 操控层：
  - [ ] Rust `uiautomation` / `windows-rs`（`spawn_blocking` STA 线程隔离）
  - [ ] 控件树裁剪过滤算法（跳过不可见/禁用节点，输出带编号交互控件树）
  - [ ] 实测 Apple Music 等常用软件 UIA 控件识别度
- [ ] Set-of-Mark 视觉兜底：
  - [ ] 屏幕截图 + UIA 框选标号绘制（或 `OmniParser`）
  - [ ] 对接 `UI-TARS-1.5-7B` / `Qwen2.5-VL-7B`（注意限制历史截图数防 12GB 爆显存）
- [ ] 安全与体验：
  - [ ] 危险操作分级与语音/GUI 二次确认弹窗
  - [ ] 消息气泡渲染 `tool_calls_json` 执行步骤折叠卡片
  - [ ] 高频 Skill 缓存（如一键开启 Apple Music 随机播放）

---

## Phase 3：局域网手机网页遥控

- [ ] Rust 内嵌 `axum` HTTP + WebSocket 服务（监听 `0.0.0.0:端口`，复用后端对话与任务总线）
- [ ] 局域网配对与鉴权（PC 生成扫码二维码 + PIN/Token 验证）
- [ ] 手机端自适应网页视图（复用 Vue 3 工程）
  - [ ] 实时同步对话历史与执行进度
  - [ ] 手机浏览器按住说话（通过 WebSocket 推音频流到 PC ASR）
  - [ ] 高危操作手机端审批确认弹窗 (`允许 / 拒绝`)
