# Skippy 按需 Python AI Worker

用于托管重型 GPU 语音模型（`Qwen3-ASR`、`CosyVoice` / `Index-TTS` 声音克隆）及二期视觉兜底能力。
**默认不随软件开机常驻**，仅在启用对应 GPU 模型后由 Rust 后端按需拉起，空闲超时自动杀进程释放 100% 显存。

## 环境初始化
```powershell
cd D:\skippy\worker
python -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r requirements.txt
```

需要启用本地 RTX 5070 GPU 模型时，按 [`requirements-gpu.txt`](./requirements-gpu.txt) 注释安装对应 CUDA 12.8+ 版 PyTorch 与推理依赖。
