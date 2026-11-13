# Skippy

> Windows AI 语音管家与桌面控制助手（Tauri v2 + Vue 3 + Rust）

- **架构与分期规格文档**：[`docs/PROJECT_SPEC.md`](./docs/PROJECT_SPEC.md)
- **开发待办清单**：[`docs/TODO.md`](./docs/TODO.md)

## 核心设计目标
1. **收起极低占用**：关闭 GUI 时销毁 WebView 渲染层，仅保留 Rust 系统托盘核心常驻（0 显存、极低内存）。
2. **多阶段演进**：
   - **第一期**：本地语音唤醒对话 + 对话历史 GUI + 分模块模型管理与切换中心
   - **第二期**：Windows UI Automation + 视觉混合操控电脑（如自动打开 Apple Music 放歌并切随机）
   - **第三期**：内嵌 `axum` 局域网服务，手机浏览器扫码远程控制电脑

## 本地开发
```bash
pnpm install
pnpm tauri dev
```

> 数据目录（SQLite 数据库与下载模型）默认存放在系统应用本地数据目录（如 `%LocalAppData%\com.bigbiglv.skippy\`），不会污染 Git 仓库，方便多台电脑同步开发。
