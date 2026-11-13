# Skippy 开发待办清单 (TODO)

> **当前阶段**：Phase 1 - Step 1 已完成骨架搭建与本地编译校验，下一步进入 Step 2（接入真实大模型流式对话与 Ollama）
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

### Step 2：文字对话流式闭环与模型管理中心（下一步）
- [ ] 实现统一 `LlmProvider` Trait
- [ ] 接入 **OpenAI 兼容云端 API**（支持自定义 Base URL / API Key / Model ID 与 SSE 流式输出，当前开发机可直接验证）
- [ ] 接入 **Ollama HTTP Client**（检测服务状态、同步 `/api/tags`、一键 `/api/pull` 进度推送、`keep_alive` 参数控制）
- [ ] 完善 `App.vue`：Markdown 渲染、流式打字机效果、停止生成、会话自动命名
- [ ] 完善 `ModelHubModal.vue`：模型下载器（带进度条与镜像源切换）、本地文件校验与删除

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

### Step 4：按需预热与主力机 GPU Worker 集成（换主力机补全环境后联调）
- [ ] 在主力机安装 `Ollama` 与 `Python 3.11+`
- [ ] 实现“唤醒瞬间并发预热”（录音同时异步唤醒 Ollama / Worker）
- [ ] 实现空闲超时回收器（Idle TTL -> `Ollama keep_alive: 0` + 终止 Python Worker）
- [ ] 完成 `worker/` 目录按需 Python Worker（支持 `Qwen3-ASR`、`CosyVoice` / `Index-TTS` 声音克隆）
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
