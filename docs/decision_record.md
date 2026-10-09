# Architectural Decision Record (ADR): ASR Engine Selection

## Status
Accepted (Milestone 1)

## Context
We are building a lightweight, open-source, local-first English voice keyboard for Windows 11 targeting an Intel Core i5-10300H (4 cores / 8 threads) laptop with 8 GB RAM.
The application must:
1. Provide responsive, streaming voice-to-text directly at the cursor.
2. Maintain low idle CPU (~0%) and minimal active RAM/CPU overhead (<500 MB RAM target).
3. Operate 100% offline without telemetry, cloud dependencies, or remote LLMs.
4. Run as a native Windows desktop app without requiring Python or PyTorch at runtime.
5. Comply with model and library licenses for redistribution and desktop dictation.

## Evaluated Candidates

### 1. FUTO `asr4all-s` (Selected)
- **Parameters**: 28.1 million (22.8M acoustic DFSMN + 5.3M PCEC language head + 2.5k kiloVAD).
- **Download Size**: ~109 MB total (shared weights: 103.2 MB, graphs: ~1 MB total).
- **Streaming Support**: Native incremental streaming. Sliding-window attention with persistent left-context state (`filter_cache`, `attn_cache`). Audio frames are encoded once per commit.
- **Inference Runtime**: ONNX Runtime (CPU execution provider). Pure native C++/Rust integration via `ort` crate.
- **Punctuation & Capitalization**: Built-in PCEC (Punctuation, Capitalization, and Error Correction) module that runs incrementally trailing the acoustic hypothesis.
- **VAD**: Built-in kiloVAD model (conv-STFT / 2,537 params), running in <0.3 ms per second of audio.
- **Resource Footprint**:
  - Memory: ~120–180 MB working set (weights memory-mapped).
  - CPU: Single-thread RTFx between 30x and 120x realtime on modern x86. Active speech uses <15% single core; idle CPU 0%.
- **License**: `futo-model-weights-ethical-use-1.0` (Permits commercial and personal use, royalty-free, explicitly authorizes dictation, accessibility, and transcription).

### 2. NVIDIA `parakeet-tdt_ctc-110m`
- **Parameters**: 114 million.
- **Footprint**: ~140–450 MB model weights.
- **Limitations for Dictation**:
  - Parakeet ONNX exports are designed for offline/batch inference, not true stateful incremental streaming.
  - Simulating streaming via chunked offline decoding creates higher latency and redundant computation.
  - CPU usage is 3x–5x higher than ASR4ALL-S due to FastConformer complexity.

### 3. NVIDIA Nemotron Speech Streaming (0.6B)
- **Parameters**: ~600 million.
- **Footprint**: >1.2 GB download, >2 GB RAM.
- **Limitations**: Exceeds the resource budget for an 8 GB RAM laptop running background tasks. High sustained CPU usage causes battery drain and thermal throttling.

## Decision
We select **FUTO asr4all-s** with **ONNX Runtime** for the primary streaming ASR engine:
- It meets all performance targets (109 MB download, <200 MB RAM, true streaming, ultra-low CPU).
- The ASR interface is abstracted behind a clean Rust trait (`AsrEngine`) so alternative models (e.g., Parakeet or future architectures) can be plugged in seamlessly.

## Implementation Architecture
- **Language**: Rust 1.99+ (native Windows MSVC target).
- **Audio Capture**: `cpal` (WASAPI native backend, 16 kHz resampled mono buffer).
- **Inference**: `ort` (ONNX Runtime C API).
- **GUI & Tray**: Native Rust GUI (`eframe` / `egui`) + system tray (`tray-icon`).
- **Input Injection**: Windows Win32 `SendInput` Unicode events with fallback.
- **Controller**: Deterministic state machine for spoken commands ("new line", "scratch that", punctuation), personal vocabulary map, and edit rollback.
