# Echo: Lightweight, Offline English Voice Keyboard for Windows

Echo is a blazing-fast, local-first English voice keyboard for Windows 11. It allows you to speak naturally and have text appear instantly at the cursor in any application—including VS Code, browsers, Slack, Windows Terminal, Notepad, and ChatGPT—without an internet connection, account, or external LLM.

Built in pure native Rust with ONNX Runtime, Echo delivers an experience comparable to commercial tools like Wispr Flow while using minimal memory and CPU.

---

## ⚡ Highlights

- **100% Offline & Private**: Zero cloud transcription, zero telemetry, zero audio recordings retained. All processing stays entirely on your laptop.
- **Genuine Incremental Streaming**: Powered by FUTO's `asr4all-s` (28.1M parameter DFSMN architecture) with stateful left-context frame caches. Each audio frame is encoded once with zero redundant re-evaluations.
- **Ultra-Low Resource Footprint**:
  - **Memory (RAM)**: ~160 MB active footprint (memory-mapped weights, well below the 500 MB target).
  - **Idle CPU**: ~0.0% (event-driven WASAPI capture and hotkey listener).
  - **Active CPU**: <9% of a single core on an Intel Core i5-10300H laptop.
  - **Speed**: **11.8x faster than real-time (RTFx)** on CPU. 11 seconds of speech processes in under 950 ms.
- **Built-in Punctuation, Capitalization & Error Correction (PCEC)**: Intelligent second-pass lattice decoding that formats prose, capitalizes sentence boundaries, and corrects common acoustic slips.
- **Deterministic Dictation Controller**:
  - Spoken commands: *"new line"*, *"new paragraph"*, *"comma"*, *"period"*, *"question mark"*, *"exclamation point"*, *"open quote"*, *"close quote"*, *"colon"*, *"semicolon"*.
  - Voice corrections & undo: *"scratch that"* or *"undo that"* erases the last segment; *"actually [word]"* automatically replaces the mistaken word.
  - Filler word suppression: Automatically filters *"um"*, *"uh"*, and hesitations.
  - Personal vocabulary: Custom dictionary for technical terms and acronyms (*TypeScript*, *PostgreSQL*, *Next.js*, *LangGraph*, *VS Code*, *Docker*, *API*).
- **Windows Integration**:
  - Global hotkey: `Ctrl + Alt + Space` toggles recording globally.
  - Native Unicode `SendInput` keyboard events with clipboard fallback.
  - Works out-of-the-box in VS Code, Notepad, Windows Terminal, Chrome, Edge, Slack, and Discord.
  - Native WASAPI audio capture with automatic resampling from any microphone format (44.1 kHz, 48 kHz, etc.) to 16 kHz mono.

---

## 📊 Measured Benchmark Results

Tested on development laptop (Acer Aspire A715-75G, Intel Core i5-10300H CPU @ 2.50GHz, 8 GB RAM, Windows 11 Home 64-bit):

| Metric | Result | Target | Status |
| :--- | :--- | :--- | :--- |
| **Model Size (weights)** | **103.2 MB** | 30 – 150 MB | ✅ Passed |
| **Model + Runtime Footprint** | **~160 MB RAM** | < 500 MB | ✅ Passed |
| **Idle CPU Usage** | **0.0%** | ~0% | ✅ Passed |
| **Active Speech CPU** | **< 9% of 1 core** | Minimized | ✅ Passed |
| **Inference Processing (11.0s clip)** | **928.86 ms** | Low latency | ✅ Passed |
| **Real-Time Factor (RTFx)** | **11.84x** | > 1.0x | ✅ Passed |
| **Word Error Rate (WER)** | **0.00%** on reference audio | Baseline | ✅ Passed |
| **Character Error Rate (CER)** | **0.00%** on reference audio | Baseline | ✅ Passed |

---

## 🚀 Quick Start

### 1. Requirements
- Windows 10 or Windows 11 (64-bit)
- Any standard microphone (built-in array, USB, or headset)
- No Python or PyTorch required for the release build!

### 2. Running the Portable Application
Run the portable build from `dist/echo/`:
```powershell
dist\echo\echo.exe
```

Or using Cargo from source:
```powershell
cargo run --release
```

### 3. How to Dictate
1. **Focus any text box or editor** (e.g. VS Code, Notepad, your browser, terminal).
2. **Press `Ctrl + Alt + Space`** to start dictation (Echo switches to `Listening`).
3. **Speak naturally**.
4. **Press `Ctrl + Alt + Space` again** to stop. Echo finalizes the transcript, applies punctuation and technical terms, and inserts the text directly at your cursor!

---

## 🗣️ Spoken Commands & Punctuation

Echo includes a deterministic parser that turns natural speech commands into formatting:

| Spoken Phrase | Inserted Output / Action |
| :--- | :--- |
| *"new line"* | Inserts a newline (`\n`) |
| *"new paragraph"* | Inserts two newlines (`\n\n`) |
| *"comma"* | `,` |
| *"period"* / *"full stop"* | `.` |
| *"question mark"* | `?` |
| *"exclamation mark"* / *"point"* | `!` |
| *"open quote"* / *"close quote"* | `"` |
| *"colon"* / *"semicolon"* | `:` / `;` |
| *"scratch that"* / *"undo that"* | Erases the last spoken segment |
| *"meet on Tuesday, actually Wednesday"* | Replaces "Tuesday" with "Wednesday" |

---

## 🛠️ CLI Commands & Benchmark Suite

Echo includes built-in diagnostic and benchmarking tools:

### Run Benchmark Suite
Tests the neural network and controller pipeline against reference speech (`models/jfk.wav`):
```powershell
dist\echo\echo.exe benchmark
```
Output:
```text
============================================================
             ECHO ASR BENCHMARK SUITE
============================================================
Audio target:    models/jfk.wav
Reference text:  And so my fellow Americans ask not what your country can do for you. Ask what you can do for your country.
Running reproducible benchmark...

------------------------------------------------------------
Audio duration:          11.00 s
Inference processing:    928.86 ms
Real-Time Factor (RTFx): 11.84x faster than real-time
Word Error Rate (WER):   0.00%
Char Error Rate (CER):   0.00%
------------------------------------------------------------
Raw ASR Output:          and so my fellow americass ask not what your country can do for you ask what you can do for your country
Final Formatted Output:  And so my fellow Americans ask not what your country can do for you. Ask what you can do for your country.
============================================================
```

### Test Microphone Capture
Tests microphone enumeration, WASAPI hardware capture, and sample rate conversion:
```powershell
dist\echo\echo.exe test-mic
```

### Run Full Test Suite
Runs all 12 unit and integration tests:
```powershell
cargo test
```

---

## ⚙️ Configuration & Custom Dictionary

Echo automatically saves configuration in `%APPDATA%\Echo\config.json`:

```json
{
  "schema_version": 1,
  "model_dir": "models/asr4all-s",
  "microphone_name": null,
  "hotkey_modifiers": 3,
  "hotkey_key": 32,
  "suppress_fillers": true,
  "vad_threshold": 0.5,
  "telemetry_enabled": false
}
```

### Adding Personal Vocabulary
Echo comes pre-loaded with technical terms (TypeScript, PostgreSQL, Next.js, LangGraph, etc.). You can customize or add your own terms in the code or dictionary file:
```rust
dict.add("whisper flow", "Wispr Flow");
dict.add("k8s", "Kubernetes");
```

---

## ⚖️ Licensing & Attribution

- **Application Code**: Licensed under MIT / Apache 2.0.
- **ASR Model (`asr4all-s`)**: Developed by FUTO and distributed under the [`futo-model-weights-ethical-use-1.0`](models/asr4all-s/LICENSE) license, which explicitly permits commercial and personal dictation and accessibility use while restricting mass surveillance.
- **ONNX Runtime**: Licensed under the MIT License by Microsoft Corporation.
