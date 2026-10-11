# Echo: Lightweight, Offline English Voice Keyboard for Windows

[![Release](https://img.shields.io/badge/Release-v0.2.2-00C7B1?style=flat&logo=windows)](https://github.com/ParitoshSrivastava31/Echo/releases/tag/v0.2.2)
[![Direct Download](https://img.shields.io/badge/Download-Installer%20(.exe)-blue?logo=windows&color=0078D4)](https://github.com/ParitoshSrivastava31/Echo/releases/download/v0.2.2/Echo-Setup-v0.2.2.exe)
[![Windows Release](https://github.com/ParitoshSrivastava31/Echo/actions/workflows/release.yml/badge.svg)](https://github.com/ParitoshSrivastava31/Echo/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform: Windows 11 / 10](https://img.shields.io/badge/Platform-Windows%2011%20%7C%2010-0078D4.svg)](https://github.com/ParitoshSrivastava31/Echo)

**Echo** is an offline, local-first English voice keyboard for Windows. It lets you speak naturally and have accurately punctuated, formatted text appear instantly at the cursor in any application—including VS Code, browsers, Slack, Windows Terminal, Notepad, and ChatGPT—without an internet connection, account, subscription, or external LLM.

Built in pure native Rust with ONNX Runtime, Echo delivers the voice dictation experience of commercial tools like Wispr Flow while maintaining a near-zero resource footprint.

---

## ⚡ Instant Download for Windows (v0.2.2)

| Package | Direct Download Link | Format | Size |
| :--- | :--- | :--- | :--- |
| **Windows Installer (Recommended)** | 🚀 **[Echo-Setup-v0.2.2.exe](https://github.com/ParitoshSrivastava31/Echo/releases/download/v0.2.2/Echo-Setup-v0.2.2.exe)** | Single-click self-extracting setup | ~99 MB |
| **Portable ZIP** | 📦 **[Echo-v0.2.2-windows-x64.zip](https://github.com/ParitoshSrivastava31/Echo/releases/download/v0.2.2/Echo-v0.2.2-windows-x64.zip)** | Standalone portable folder | ~108 MB |

*All speech models and runtime DLLs are fully bundled. Zero cloud setup or internet connection required.*

---

## ⚡ Key Highlights

- **100% Offline & Private**: Zero cloud transcription, zero telemetry, zero audio recordings retained. All processing runs entirely on your local machine.
- **Floating Pill HUD (Jony Ive & Dieter Rams White Ceramic Aesthetic)**:
  - **Ultra-Compact 120×36 px Capsule**: Significantly reduced width; floats featherlight and non-intrusively at the bottom-center of your screen.
  - **Pristine White Ceramic Glass**: Apple & Braun-inspired white frosted acrylic glass (`rgba(255, 255, 255, 0.96)`) with subtle hairline rim and soft studio ambient shadow.
  - **Subtle Luxury Aqua Status Jewel**: Jewelry-grade seafoam cyan/aqua dot (`#00C7B1`) with delicate breathing photon halo (no harsh traffic lights).
  - **7-Needle Gaussian Harmonic Waveform**: Mathematical acoustic envelope $[0.20, 0.45, 0.78, 1.00, 0.78, 0.45, 0.20]$ rendered in slate titanium needles reacting to live microphone RMS volume.
  - **Non-Interfering Overlay**: Built with Win32 layered windows (`WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT`). Never steals keyboard focus from your active editor, and mouse clicks pass right through.
  - **Clear State Machine**: Smooth transitions between **Listening** (breathing aqua jewel + live Gaussian needles), **Processing** (subtle graphite wave shimmer), and **Done** (emerald-mint checkmark with gentle auto-fade).
- **Dual Hotkey Modes**:
  - **Push-to-Talk**: Hold `Win + Alt` (or `Alt + E`) while speaking and release to insert.
  - **Tap-to-Toggle**: Quickly tap `Win + Alt` (or `Alt + E`) to start listening, and tap again when finished.
  - Customizable shortcuts from the tray menu (`Win + Alt`, `Alt + Space`, `Ctrl + Space`, `Win + Space`, `Ctrl + Alt`).
- **Full Windows Desktop Experience**:
  - **System Tray Icon**: Native tray menu with microphone selector, shortcut picker, filler filter toggle, "Start with Windows" autostart toggle, and dictionary editor.
  - **No Console Window**: Runs as a true native Windows GUI app (`windows_subsystem = "windows"`).
  - **Per-Monitor V2 DPI Aware**: Sharp, anti-aliased software rendering on 100%, 125%, 150%, and 200% scaling.
- **Genuine Incremental Streaming**: Powered by FUTO's `asr4all-s` (28.1M parameter DFSMN architecture) with left-context frame caches. Each audio frame is processed incrementally without redundant recalculations.
- **Audio-loss detection**: Recognition runs on a dedicated worker so inference does not block microphone queue draining. Known audio queue drops or microphone errors are reported and the incomplete transcript is withheld.
- **Deterministic Dictation Controller**:
  - **Spoken Commands**: *"new line"*, *"new paragraph"*, *"comma"*, *"period"*, *"question mark"*, *"exclamation point"*, *"open quote"*, *"close quote"*, *"colon"*, *"semicolon"*.
  - **Voice Corrections & Undo**: *"scratch that"* or *"undo that"* erases the last segment; inline self-corrections like *"call him Tuesday, actually Wednesday"* automatically replace the corrected words.
  - **Filler Suppression**: Automatically filters *"um"*, *"uh"*, and hesitations.
  - **Personal Vocabulary**: Built-in and user-customizable dictionary for technical terms and acronyms (*TypeScript*, *PostgreSQL*, *Next.js*, *LangGraph*, *Docker*, *VS Code*, *API*).

---

## 📊 Performance & Benchmark

Previously measured on one 11-second JFK reference clip using the development target (Acer Aspire A715-75G, Intel Core i5-10300H CPU @ 2.50GHz, 8 GB RAM, Windows 11 Home 64-bit). These figures do not measure developer prompts, varied accents, or performance of the new audio-loss handling. The zero WER/CER result is a single-clip baseline, not a general accuracy claim.

| Metric | Measured Result | Project Goal | Status |
| :--- | :--- | :--- | :--- |
| **Model Size (weights)** | **103.2 MB** | 30 – 150 MB | ✅ Passed |
| **Active Memory (RAM)** | **~158 MB RAM** | < 500 MB | ✅ Passed |
| **Idle CPU Usage** | **0.0%** | ~0% | ✅ Passed |
| **Active CPU (Speaking)** | **< 9% of 1 core** | Minimized | ✅ Passed |
| **Inference Processing (11.0s clip)** | **928.86 ms** | Low latency | ✅ Passed |
| **Real-Time Factor (RTFx)** | **11.84x faster than real-time** | > 1.0x | ✅ Passed |
| **Word Error Rate (WER)** | **0.00%** on one JFK reference clip | Single-clip baseline | Measured |
| **Character Error Rate (CER)** | **0.00%** on one JFK reference clip | Single-clip baseline | Measured |
| **Test Suite** | **28 tests** (previous release baseline) | CI on release | — |

---

## 📥 Download & Installation

### Option 1: Windows Installer (Recommended)

Download the latest installer from the [Releases](https://github.com/ParitoshSrivastava31/Echo/releases) page:

1. Download **`Echo-Setup-v0.2.2.exe`** (approx. 99 MB, self-contained with model weights and ONNX runtime).
2. Double-click to install. It installs to your user profile (`%LOCALAPPDATA%\Programs\Echo`) without requiring administrator permissions.
3. Optionally enable **"Start Echo when Windows starts"** and **"Create Desktop Shortcut"**.
4. Echo launches immediately and docks silently in your Windows System Tray.

### Option 2: Portable ZIP

1. Download **`Echo-v0.2.2-windows-x64.zip`**.
2. Extract to any folder (e.g. `C:\Tools\Echo`).
3. Run `echo.exe`.

---

## 🎙️ How to Dictate

1. **Focus any text box or application** (e.g. VS Code, browser, Word, Notepad, Slack, Terminal).
2. **Push-to-Talk**: Hold down **`Win + Alt`**, speak your thoughts, and release the keys.
3. **Or Tap-to-Toggle**: Tap **`Win + Alt`** once to begin dictating (the pill displays `LIVE`), speak, and tap **`Win + Alt`** again to finish.
4. The floating pill visualizer displays your voice waveform in real time. Upon release, text is punctuated, formatted, and inserted cleanly at your cursor.

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
| *"scratch that"* / *"undo that"* | Erases the last inserted segment |
| *"meet on Tuesday, actually Wednesday"* | Automatically replaces "Tuesday" with "Wednesday" |

---

## ⚙️ Configuration & Tray Menu

Right-click the Echo icon in the System Tray to access settings:

- **Hotkey**: Switch between `Win + Alt`, `Alt + Space`, `Ctrl + Space`, `Win + Space`, or `Ctrl + Alt`.
- **Microphone**: Select your preferred input device or use the Windows default.
- **Filter filler words**: Toggle suppression of *"um"*, *"uh"*, and stuttered repetitions.
- **Start with Windows**: Toggle automatic startup on login.
- **Open Personal Dictionary**: Opens `%LOCALAPPDATA%\Echo\dictionary.json` in your default editor.
- **Open Log File**: View privacy-safe application logs at `%LOCALAPPDATA%\Echo\echo.log` (transcripts and voice data are never logged).
- **Run ASR Benchmark**: Run instant accuracy and latency diagnostics.

---

## 🛠️ Building from Source

### Prerequisites

- 64-bit Windows 10 or 11
- [Rust toolchain](https://rustup.rs/) (MSVC target)
- [Inno Setup 6](https://jrsoftware.org/isdl.php) (for building the installer)

### Build Steps

```powershell
# Clone the repository
git clone https://github.com/ParitoshSrivastava31/Echo.git
cd echo

# Run the test suite
cargo test --release

# Build optimized release binary
cargo build --release

# Generate icon asset
cargo run -- write-ico

# Compile Inno Setup installer
& "ISCC.exe" scripts\installer.iss
```

The resulting installer is generated at `dist\Echo-Setup-v0.2.2.exe`.

### Diagnostic Commands

```powershell
# Run reproducibility benchmark against JFK reference speech
target\release\echo.exe benchmark

# Test microphone capture and 16kHz resampling
target\release\echo.exe test-mic
```

---

## ⚖️ Licensing & Attribution

- **Echo Application Code**: Licensed under the [MIT License](LICENSE).
- **Speech Recognition Model (`asr4all-s`)**: Pretrained model weights developed by FUTO, distributed under the [FUTO Model Weights License (Restricted Use) 1.0](models/asr4all-s/LICENSE).
- **ONNX Runtime**: Developed and licensed under the MIT License by Microsoft Corporation.
