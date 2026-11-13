"""Skippy 按需 AI Worker 微服务

设计原则：
1. 待机时本进程不启动（0 内存、0 显存）。
2. 唤醒或启用 GPU 重型模型（Qwen3-ASR / CosyVoice / Index-TTS / 二期视觉兜底）时由 Rust 主进程拉起。
3. 空闲超时后由 Rust 主进程直接终止本进程，由操作系统 100% 回收系统内存与 RTX 5070 显存。
"""

from __future__ import annotations

import gc
import os
import subprocess
import time
from typing import Optional

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

app = FastAPI(title="Skippy AI Worker", version="0.1.0")

START_TIME = time.time()
LOADED_MODELS: dict[str, object] = {}


def query_gpu_info() -> dict:
    try:
        out = subprocess.check_output(
            [
                "nvidia-smi",
                "--query-gpu=name,memory.used,memory.total",
                "--format=csv,noheader,nounits",
            ],
            text=True,
            timeout=2,
            creationflags=0x08000000 if os.name == "nt" else 0,
        ).strip()
        if out:
            name, used, total = [x.strip() for x in out.splitlines()[0].split(",")]
            return {
                "available": True,
                "name": name,
                "vram_used_mb": int(used),
                "vram_total_mb": int(total),
            }
    except Exception:
        pass
    return {"available": False, "name": None, "vram_used_mb": 0, "vram_total_mb": 0}


@app.get("/health")
def health():
    return {
        "status": "ok",
        "pid": os.getpid(),
        "uptime_secs": round(time.time() - START_TIME, 1),
        "loaded_models": list(LOADED_MODELS.keys()),
        "gpu": query_gpu_info(),
    }


class AsrRequest(BaseModel):
    audio_path: str
    model_id: str = "qwen3-asr-0.6b-gpu"
    language: str = "zh"


@app.post("/asr/transcribe")
def transcribe(req: AsrRequest):
    if not os.path.exists(req.audio_path):
        raise HTTPException(status_code=400, detail=f"Audio file not found: {req.audio_path}")
    # 延迟加载与推理挂载点：后续安装 worker/requirements-gpu.txt 后接入 Qwen3-ASR 权重推理
    raise HTTPException(
        status_code=501,
        detail="Qwen3-ASR GPU 权重尚未下载或未安装 GPU 依赖（见 worker/requirements-gpu.txt）",
    )


class TtsRequest(BaseModel):
    text: str
    model_id: str = "cosyvoice-gpu-worker"
    voice_id: Optional[str] = None
    reference_audio_path: Optional[str] = None
    output_path: Optional[str] = None


@app.post("/tts/synthesize")
def synthesize(req: TtsRequest):
    if not req.text.strip():
        raise HTTPException(status_code=400, detail="Empty text")
    # 延迟加载与推理挂载点：后续安装 worker/requirements-gpu.txt 后接入 CosyVoice / Index-TTS
    raise HTTPException(
        status_code=501,
        detail="CosyVoice / Index-TTS GPU 权重尚未下载或未安装 GPU 依赖（见 worker/requirements-gpu.txt）",
    )


@app.post("/unload")
def unload_models():
    LOADED_MODELS.clear()
    gc.collect()
    try:
        import torch  # type: ignore

        if torch.cuda.is_available():
            torch.cuda.empty_cache()
    except ImportError:
        pass
    return {"status": "unloaded", "gpu": query_gpu_info()}
