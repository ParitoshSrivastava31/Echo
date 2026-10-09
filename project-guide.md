# Project: Build a Lightweight, Offline English Dictation App for Windows

You are the lead systems engineer, speech-recognition engineer, and desktop application developer for this project.

Your task is to **build a working, installable Windows desktop dictation application** that can compete with Wispr Flow on the quality of the dictation experience while using as little memory, CPU, and battery as possible.

Do not stop after producing an architecture document, a prototype UI, or a list of recommendations. Inspect the development environment, research the available models and runtimes, implement the application, run it, test it, measure its performance, and document the remaining limitations.

## 1. Product vision

Build an open-source, local-first English voice keyboard.

The user presses a global keyboard shortcut, speaks naturally, and sees text appear at the current cursor position in applications such as VS Code, a browser, Gmail, Slack, a terminal, and ChatGPT.

The application must work without an internet connection after installation and model download.

It must not require an account, subscription, API key, cloud transcription service, or remote LLM.

The product is English-only. Do not add multilingual models or language-selection infrastructure. Prioritize English speech, technical terminology, programming vocabulary, AI conversations, natural corrections, and Indian English accents.

The goal is to find the smallest model that delivers an excellent dictation experience, not the smallest model at any cost.

Do not train a model from scratch. Do not begin with fine-tuning. First build the product around existing pretrained models and determine whether recognition quality is actually the bottleneck.

## 2. My development machine

Use this hardware as the minimum development and performance-testing target:

- Laptop: Acer Aspire A715-75G
- Operating system: Windows 11 Home Single Language, 64-bit
- CPU: Intel Core i5-10300H, 4 cores and 8 logical processors
- RAM: 8 GB
- GPU: NVIDIA GTX 1650 with 4 GB VRAM
- Development must work without the GPU. GPU acceleration is optional.

Assume that the laptop may already have other applications running.

Do not require a second computer, an Ubuntu development machine, a remote server, Docker, WSL, or a cloud service for normal development or runtime.

Keep the production application independent of Python and PyTorch. They may be used for one-time research or conversion experiments if absolutely necessary, but they must not be runtime dependencies of the installed app.

## 3. Mandatory research before selecting the ASR engine

Use your available web search and repository inspection tools before implementing the recognition engine.

Research current releases and compatible implementations available as of 9 October 2026. Check official model cards, GitHub repositories, license files, runtime documentation, model artifact sizes, and available benchmarks.

Start with these candidates:

1. FUTO `futo-org/asr4all-s`
   - Approximately 28.1 million parameters.
   - English-only.
   - Designed for efficient edge inference.
   - Supports streaming configurations.
   - Has associated punctuation, capitalization, error-correction, and voice-activity components.
   - Investigate its supported ONNX and ExecuTorch runtimes and whether a reliable native integration is feasible.

2. NVIDIA `nvidia/parakeet-tdt_ctc-110m`
   - Approximately 114 million parameters.
   - English speech recognition.
   - Includes punctuation and capitalization.
   - Investigate validated quantized artifacts and their licenses.
   - Do not assume that a third-party GGUF conversion automatically provides a reliable, genuinely streaming runtime.

3. NVIDIA Nemotron Speech Streaming, approximately 0.6B parameters.
   - Use as a reference candidate only if the runtime can be tested within the available hardware.
   - Investigate Microsoft's published work on CPU inference and quantization.
   - Do not make this the default merely because it performs well on research benchmarks.

Also search for newer English-only, compact streaming ASR models released before the research cutoff. A newer model may replace any of these candidates if it has stronger evidence of accuracy, efficiency, licensing compatibility, and deployability.

For each candidate, record:

- Model name, parameter count, and exact source URL.
- Model artifact size and expected installed footprint.
- Whether it genuinely supports incremental streaming.
- Available CPU inference backends.
- RAM requirements and measured CPU usage, if available.
- Recognition quality and benchmark methodology.
- First-partial and finalization latency.
- Support for punctuation and capitalization.
- License, attribution requirements, redistribution terms, and commercial-use restrictions.
- Whether the model can be integrated into a native Rust or C/C++ application without a permanent Python runtime.
- Whether the model and its runtime can be redistributed legally with the application.

Treat model weights, tokenizer files, vocabularies, and runtime libraries as separate licensing and packaging concerns. Read the actual license files. Do not assume that an open-weight model is unrestricted open source.

### Initial model decision

Use FUTO asr4all-small as the first model to investigate for the low-resource streaming path.

Compare it against Parakeet 110M using the actual application pipeline. Do not finalize the model choice until the selected runtime has passed a real inference test on this laptop.

Keep the ASR engine behind a replaceable interface so a better model can be added without rewriting the desktop application.

If no reliable native integration is available for a candidate, document the limitation and test the next candidate. Do not fabricate compatibility or create a fake transcription implementation.

Do not download multiple large models automatically. First estimate the download and disk requirements, then obtain only the artifacts needed for the current benchmark.

## 4. Technology requirements

Prefer native systems technology.

- Core application: Rust.
- Desktop interface: a lightweight native Rust GUI, preferably egui or another suitable native toolkit.
- Audio capture: a maintained native Rust audio library such as CPAL, or a comparably reliable Windows audio implementation.
- Global keyboard shortcuts: a maintained cross-platform Rust library or Windows-native API.
- ASR inference: the most suitable supported native backend for the selected model, such as ONNX Runtime, OpenVINO, ExecuTorch, or a model-specific optimized C/C++ runtime.
- Windows text insertion: Windows-native text-input APIs and Unicode-safe input injection, with a carefully tested clipboard fallback.
- Configuration: a versioned local configuration file.
- Logs: local structured logs with privacy-sensitive content excluded by default.

Do not use Electron. Do not use a browser-based frontend that adds a large idle memory footprint. Avoid a heavyweight webview unless measurements show that it is justified.

Do not use Python, Node.js, a remote API, or a separately installed server as a required production dependency.

Use safe Rust where practical. Isolate any necessary native FFI behind a small, well-documented module.

Pin dependency versions and document the native build requirements.

## 5. Required user experience

Implement the complete basic flow:

1. The user installs and launches the application.
2. The application checks the microphone and asks for any required permissions.
3. The user chooses a global shortcut.
4. The user focuses any text field in another application.
5. The user presses the shortcut to begin dictation.
6. The application captures microphone audio and streams it through the ASR engine.
7. Partial text becomes available while the user is speaking.
8. The application commits stable text without repeatedly duplicating or rewriting previously committed words.
9. The user stops dictation using the shortcut or a configured stop action.
10. The application finalizes the transcript and inserts the remaining text at the cursor.
11. The application returns to a low-resource idle state.

Provide a simple system-tray interface with:

- Start or stop dictation.
- Current status.
- Microphone selection.
- Shortcut configuration.
- Model status and download location.
- CPU and memory diagnostics.
- Privacy and logging controls.
- A way to exit the application completely.

Include a compact visual state indicator for idle, listening, processing, and error states.

Do not make the user open a separate editor and copy text manually for normal dictation.

## 6. Audio and streaming architecture

Implement the following logical pipeline:

Microphone capture
→ voice activity detection
→ streaming ASR
→ transcript stabilization
→ dictation controller
→ text insertion at the cursor.

Audio processing requirements:

- Use mono, 16 kHz PCM internally when required by the chosen model.
- Use a suitable resampler if the input device has a different sample rate.
- Avoid unnecessary audio copies and repeated format conversions.
- Use bounded audio buffers and explicit backpressure.
- Keep audio capture responsive while inference is running.
- Use a dedicated inference worker or equivalent isolation so model execution cannot block the UI or audio callback.
- Handle microphone removal, device changes, capture errors, and inference failures without crashing.
- Do not continuously run expensive recognition when no speech is present.
- Use the smallest suitable VAD. If the selected model already provides an appropriate VAD, evaluate it before adding another.
- Do not process the same audio repeatedly without a clear reason.

The system must distinguish between provisional and committed transcript text.

Maintain an unstable tail for words that may change as more audio arrives. Commit stable text only when the ASR engine provides sufficient evidence or its decoding policy indicates that it is safe.

Never insert every new full transcript as a new string. That would duplicate text.

If a partial transcript changes, update only the uncommitted region where possible. If the target application does not support safe incremental replacement, buffer and commit text conservatively.

Expose the following measurements:

- Time to first useful partial transcript.
- Time from speech ending to final text.
- Time spent in ASR inference.
- Amount of audio buffered.
- Number of transcript revisions.
- Number of committed words later corrected by the controller.

Do not claim a sub-300 ms end-to-end response time unless measurements support it. Report model latency and full application latency separately.

## 7. Dictation controller

This is a major part of the product. Do not rely on the ASR model alone to provide a polished experience.

Build a deterministic, testable controller that operates on recognized text and, where available, transcript timing information.

Implement these features:

### Basic text cleanup

- Capitalization and sentence boundaries.
- Punctuation normalization.
- Removal of obvious speech-recognition artifacts.
- Optional removal of filler words such as "um" and "uh".
- Preservation of meaningful words and conversational phrasing.
- Correct spacing around punctuation.
- Unicode-safe editing.

Make aggressive filler removal optional. Do not silently rewrite the user's intended meaning.

### Spoken commands

Support explicit commands such as:

- "New line"
- "New paragraph"
- "Comma"
- "Period"
- "Question mark"
- "Exclamation mark"
- "Open quote" and "close quote"
- "Undo that"
- "Scratch that"
- "Delete the last sentence"

Where appropriate, support:

- "Select the last sentence"
- "Capitalize that"
- "Make that a bullet"

Commands must be recognized conservatively. Do not convert normal prose into an action merely because it contains a phrase that resembles a command.

Use a deterministic command parser and explicit state transitions. Avoid running a large LLM for routine punctuation or command handling.

Commands that modify previously inserted text must operate on text the application actually owns. Keep a bounded edit history so corrections are reversible.

### Corrections and repetitions

Handle common self-corrections where practical:

"I'll meet you on Tuesday, actually Wednesday."

The controller should avoid preserving both alternatives when the intended correction is clear.

Support explicit correction phrases such as "scratch that" and "I mean".

Do not implement speculative rewriting that changes meaning. Add tests for ambiguous cases and prefer preserving the spoken words when confidence is low.

### Personal vocabulary

Provide a local dictionary for technical words, names, acronyms, and product-specific vocabulary.

It should be possible to teach the application that a phrase should be written as a particular term. Examples include Vercel, PostgreSQL, TypeScript, Next.js, LangGraph, and API.

Keep these corrections local. Provide a way to inspect, edit, export, and delete the dictionary.

Do not claim that dictionary replacement improves the underlying acoustic recognition. Measure whether it improves the final output.

## 8. Context and LLM policy

The core application must not need a language model beyond its ASR and small deterministic processing components.

Do not call an external LLM. Do not require an internet connection.

Do not run a general-purpose local LLM on every utterance. It would increase memory usage, latency, power consumption, and the risk of changing the user's intended meaning.

Design the controller so that an optional local text-transformation model could be added later for explicit commands such as "rewrite this" or "turn this into bullets". Do not implement this optional model unless the core dictation pipeline is already stable and fast.

App-aware profiles for VS Code, terminals, email, Slack, and ChatGPT are a later enhancement. Do not inspect screen contents, clipboard contents, documents, or other applications by default.

If app-aware behavior is added later, it must be explicit, local, and configurable.

## 9. Text insertion and Windows compatibility

Build and test a dedicated text-insertion layer.

Requirements:

- Insert Unicode text at the focused cursor.
- Work with standard Windows text fields and common desktop applications.
- Support long dictation sessions.
- Avoid duplicate insertion and lost characters.
- Handle clipboard restoration correctly if a clipboard-based fallback is used.
- Preserve existing clipboard contents.
- Do not overwrite text if focus has moved to a different application unexpectedly.
- Provide a clear failure message when insertion is not supported by a target application.
- Never silently discard a finalized transcript.

Test text insertion in at least Notepad, a browser text field, VS Code, and a terminal.

Some applications handle simulated keystrokes differently. Document these limitations rather than pretending universal compatibility.

## 10. Performance targets

The application must be useful on the specified 8 GB RAM laptop while other applications are open.

Use these as engineering goals, not guaranteed results:

- Idle CPU: approximately 0%, apart from occasional wake-ups and status updates.
- Idle memory: keep the application overhead small.
- Model plus runtime memory: target below 500 MB for the initial small-model path; lower is better.
- Download size: target approximately 30 to 150 MB for a suitable production model, but do not sacrifice recognition quality merely to hit this range.
- CPU usage while speaking: minimize sustained usage and leave capacity for other applications.
- CPU-only inference must work.
- GPU use must be optional.
- Startup must not block the desktop.
- The interface and audio capture must remain responsive during inference.
- Memory must remain bounded during long dictation sessions.
- The app must return to an idle state after dictation.
- No network traffic is required during normal use.

Measure real CPU usage, memory usage, latency, model size, and recognition accuracy. Include both application overhead and model inference in the measurements.

Do not optimize only for peak throughput. Low latency and a responsive desktop matter more than transcribing audio at hundreds of times real time.

If the model is fast enough, limit inference to a sensible number of CPU threads. Do not automatically use all eight logical processors.

Consider a low-priority inference worker and a configurable performance mode. Avoid aggressive system tuning or changing Windows settings without user consent.

## 11. Benchmark before making accuracy claims

Create a reproducible local benchmark command.

Benchmark the chosen model on recorded speech and, where possible, compare the small FUTO model with Parakeet 110M.

The test corpus should include:

- Everyday English.
- Natural conversational speech.
- Indian English accents.
- Fast speech.
- Pauses and hesitations.
- Self-corrections.
- Programming terminology.
- Framework and library names.
- AI and machine-learning vocabulary.
- Numbers, dates, URLs, and email addresses.
- Speech recorded through the laptop microphone.
- Speech recorded through a headset.
- Moderate background noise.

Do not require a large public dataset to run the basic benchmark. Include a small set of legally usable sample recordings or clear instructions for adding user-owned audio.

For each test, calculate word error rate where reference transcripts are available. Also report character error rate, latency, CPU usage, memory, and final text quality.

Create separate metrics for:

- Raw ASR output.
- Final output after dictation-controller processing.
- Command recognition accuracy.
- Technical-term accuracy.
- Text-insertion correctness.

Use identical audio inputs when comparing models.

Do not claim that this product matches or beats Wispr Flow without a fair, reproducible comparison. The initial objective is to establish a measurable baseline and improve it.

## 12. Project structure and maintainability

Use a clean, modular structure. Adapt the exact directories to the chosen runtime, but keep responsibilities separated.

Suggested modules:

- `audio`: microphone capture, resampling, buffers.
- `vad`: speech detection.
- `asr`: model interface, loading, streaming inference, model metadata.
- `streaming`: partial transcript and stable-text management.
- `controller`: commands, corrections, punctuation, formatting.
- `vocabulary`: local correction dictionary.
- `insertion`: Windows text insertion and fallback.
- `hotkey`: global shortcut handling.
- `tray`: system-tray actions and status.
- `settings`: configuration and migrations.
- `telemetry`: local performance counters without collecting user speech.
- `benchmark`: repeatable performance and accuracy tests.

The ASR interface should allow another backend to be added without changing the controller or user interface.

Define explicit interfaces for partial results, finalized results, errors, model loading, and cancellation.

Keep dependencies lean. Avoid adding a large framework for a feature that can be implemented safely with a small native module.

## 13. Privacy and offline guarantees

The app must be local-first by design.

- No cloud transcription.
- No analytics by default.
- No background uploads.
- No speech recordings retained by default.
- No transcript content in ordinary logs.
- No unnecessary telemetry.
- No remote configuration or model requests during normal use.
- No hidden collection of clipboard or screen contents.

Model downloads and updates may use the network during setup, but they must be explicit and documented.

After installation and model download, the user must be able to disconnect from the internet and dictate normally.

Provide a way to verify that the application has the required model files locally.

Document every network-capable dependency and verify that the normal dictation path does not use it.

## 14. Installation and distribution

The target is a normal Windows user, not a developer.

Provide:

- A reproducible development build.
- A release build.
- A Windows installer or portable distribution.
- A first-run setup flow.
- A model download and integrity-check step where appropriate.
- Clear model attribution and license notices.
- A README with build, install, uninstall, troubleshooting, and offline-use instructions.
- A dependency and license inventory.
- A versioned configuration format.
- Useful diagnostics that do not expose private transcripts.

The installed app must not require the user to install Python, configure a virtual environment, open a terminal, or manually start a server.

Do not bundle multiple model variants in the initial installer unless there is a compelling reason. Keep the base download small.

If model licensing prevents redistribution, provide a compliant download workflow instead of bundling the weights.

## 15. Development workflow

Work in small, verifiable milestones. Do not attempt to write the entire application in one untested pass.

### Milestone 1: Environment and model validation

Inspect the available Rust toolchain, C/C++ compiler, Windows SDK, GPU availability, and development dependencies.

Research the candidate models and choose a supported inference path.

Build a minimal command-line proof of concept that records or loads audio and produces a transcript entirely locally.

Measure model loading time, memory use, inference latency, and CPU usage.

### Milestone 2: Native audio and streaming

Implement microphone capture, VAD, the ASR worker, and partial-result handling.

Demonstrate that inference does not freeze audio capture or the UI.

Test repeated start and stop cycles.

### Milestone 3: Dictation controller

Implement punctuation, commands, corrections, and stable transcript handling.

Add unit tests for commands, ambiguous phrases, self-corrections, and Unicode.

### Milestone 4: Cursor insertion and global hotkey

Implement the global shortcut and text insertion.

Validate the workflow in Notepad, VS Code, a browser, and a terminal.

### Milestone 5: Desktop experience

Add the tray interface, settings, model status, errors, and first-run setup.

Ensure the application behaves correctly when minimized or left running for hours.

### Milestone 6: Performance and release

Run the benchmark suite on the target laptop.

Measure idle and active resource use.

Fix memory growth, audio dropouts, transcript duplication, focus issues, and latency problems before adding optional features.

Produce the release build and installation instructions.

At the end of each milestone, run the relevant tests and report actual results. Do not mark a feature complete merely because code was generated.

## 16. Required tests

At minimum, test:

- Missing model files.
- Corrupt or incomplete model downloads.
- Offline first launch after the model has been downloaded.
- Missing microphone.
- Microphone disconnection.
- Rapid repeated hotkey presses.
- Starting and stopping dictation repeatedly.
- Long speech sessions.
- Silence between utterances.
- Partial transcript revisions.
- Duplicate transcript prevention.
- Speech commands that resemble ordinary prose.
- Undo and correction behavior.
- Clipboard preservation.
- Unicode text insertion.
- Target application losing focus.
- ASR initialization failure.
- Recovery after an inference error.
- Memory usage over an extended session.

Include unit tests, integration tests where possible, and a manual Windows acceptance-test checklist.

## 17. Final acceptance criteria

The first release is complete only when:

1. The application builds and runs on Windows 11.
2. It performs English dictation with a local pretrained model.
3. It works without internet access after setup.
4. A global shortcut starts and stops dictation.
5. Dictated text appears at the focused cursor.
6. Streaming updates do not create duplicate words.
7. Basic punctuation and spoken commands work.
8. The application remains responsive while inference runs.
9. Idle and active resource usage have been measured.
10. The model and dependency licenses have been documented.
11. A repeatable benchmark exists.
12. The installation and troubleshooting instructions are complete.

Do not report success for any criterion that has not actually been tested.

## 18. Priorities and exclusions

Prioritize in this order:

1. Correct, natural dictation.
2. Low latency and reliable streaming.
3. Low RAM, CPU, and battery usage.
4. Reliable cursor insertion and global hotkeys.
5. Privacy and offline operation.
6. A simple, polished desktop experience.
7. Optional advanced features.

Do not add multilingual support, cloud inference, a mandatory local LLM, model training, complex agent frameworks, telemetry, or a remote backend.

Do not build a generic transcription editor. Build a voice keyboard that works across the desktop.

Do not optimize for benchmark scores alone. Optimize for the experience of speaking naturally and seeing the right words appear in the right place.

## 19. How to begin

Start by inspecting the repository and environment. If the repository is empty, initialize a clean project.

Then research the candidate ASR models using your search tools and inspect their actual repositories and licenses.

Create a short decision record explaining the chosen model and runtime, with evidence for model size, supported streaming behavior, licensing, and expected resource use.

Implement Milestone 1 immediately after that decision. Do not stop at planning.

If you encounter a blocker, identify it precisely, try the next viable implementation path, and keep the rest of the project moving.

At the end of each work session, report:

- What is implemented.
- What actually builds and runs.
- Tests executed and their results.
- Measured memory, CPU, and latency.
- Known bugs and limitations.
- The next concrete milestone.

The end goal is a lightweight, open-source English dictation app that a person can install and use every day, even on a modest laptop, without sending their speech to the cloud.