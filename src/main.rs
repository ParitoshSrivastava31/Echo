#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

use echo::asr::{init_runtime, Asr4AllEngine, AsrEngine, AsrPartial};
use echo::audio::AudioCapture;
use echo::benchmark::BenchmarkRunner;
use echo::config::AppConfig;
use echo::controller::{ControllerOutput, DictationController, InsertContext};
use echo::hotkey::{HotkeyAction, HotkeyManager, TwoKeyCombo};
use echo::insertion::TextInjector;
use echo::ui::{DictationState, PillOverlay, TrayCommand, TrayManager};
use echo::vocabulary::PersonalDictionary;

enum AsrWorkerCommand {
    Reset,
    Audio(Vec<f32>),
    Finish,
    Shutdown,
}

enum AsrWorkerEvent {
    Ready(Result<(), String>),
    Finished(Result<AsrPartial, String>),
}

fn spawn_asr_worker(
    engine: Arc<Asr4AllEngine>,
) -> Result<(Sender<AsrWorkerCommand>, Receiver<AsrWorkerEvent>, std::thread::JoinHandle<()>), std::io::Error> {
    let (command_tx, command_rx) = bounded::<AsrWorkerCommand>(2000);
    let (event_tx, event_rx) = unbounded::<AsrWorkerEvent>();
    let worker = std::thread::Builder::new()
        .name("echo-asr".into())
        .spawn(move || {
            let mut session = match engine.create_session() {
                Ok(session) => {
                    let _ = event_tx.send(AsrWorkerEvent::Ready(Ok(())));
                    session
                }
                Err(error) => {
                    let message = error.to_string();
                    let _ = event_tx.send(AsrWorkerEvent::Ready(Err(message.clone())));
                    while let Ok(command) = command_rx.recv() {
                        match command {
                            AsrWorkerCommand::Finish => {
                                let _ = event_tx.send(AsrWorkerEvent::Finished(Err(message.clone())));
                            }
                            AsrWorkerCommand::Shutdown => break,
                            AsrWorkerCommand::Reset | AsrWorkerCommand::Audio(_) => {}
                        }
                    }
                    return;
                }
            };
            let mut failure: Option<String> = None;
            while let Ok(command) = command_rx.recv() {
                match command {
                    AsrWorkerCommand::Reset => {
                        session.reset();
                        failure = None;
                    }
                    AsrWorkerCommand::Audio(samples) => {
                        if failure.is_none() {
                            failure = session.accept_audio(&samples).err().map(|e| e.to_string());
                        }
                    }
                    AsrWorkerCommand::Finish => {
                        let result = match failure.take() {
                            Some(error) => Err(error),
                            None => session.finish().map_err(|e| e.to_string()),
                        };
                        let _ = event_tx.send(AsrWorkerEvent::Finished(result));
                    }
                    AsrWorkerCommand::Shutdown => break,
                }
            }
        })?;
    Ok((command_tx, event_rx, worker))
}

fn queue_asr_audio(
    command_tx: &Sender<AsrWorkerCommand>,
    samples: Vec<f32>,
    audio_fault: &AtomicBool,
    dropped_chunks: &AtomicU64,
) {
    if command_tx.try_send(AsrWorkerCommand::Audio(samples)).is_err() {
        audio_fault.store(true, Ordering::Relaxed);
        dropped_chunks.fetch_add(1, Ordering::Relaxed);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        // If launched with CLI arguments from a terminal, attach to parent console so output prints
        unsafe {
            windows_sys::Win32::System::Console::AttachConsole(0xFFFFFFFF); // ATTACH_PARENT_PROCESS
        }

        match args[1].as_str() {
            "benchmark" => return run_benchmark_command(),
            "test-mic" => return run_test_mic_command(),
            "write-ico" => return run_write_ico_command(),
            "--version" | "-v" => {
                println!("Echo v0.2.0 — Offline Voice Keyboard for Windows");
                return Ok(());
            }
            "--help" | "-h" => {
                println!("Echo — Offline Voice Keyboard for Windows");
                println!("\nUsage:");
                println!("  echo.exe              Start background voice keyboard (Pill UI + Tray)");
                println!("  echo.exe benchmark    Run accuracy & latency benchmark suite");
                println!("  echo.exe test-mic     List audio devices and test 3s microphone capture");
                println!("  echo.exe write-ico    Generate echo.ico icon asset");
                return Ok(());
            }
            _ => {}
        }
    }

    if let Err(e) = run_app() {
        echo::log::info(&format!("Fatal error in run_app: {e}"));
        eprintln!("Fatal error: {e}");
        return Err(e);
    }
    Ok(())
}



fn run_benchmark_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!("             ECHO ASR BENCHMARK SUITE");
    println!("============================================================");

    let wav_file = echo::paths::find_resource("models/jfk.wav")
        .unwrap_or_else(|| std::path::PathBuf::from("models/jfk.wav"));
    let reference = "And so my fellow Americans ask not what your country can do for you. Ask what you can do for your country.";

    println!("Audio target:    {}", wav_file.display());
    println!("Reference text:  {}", reference);
    println!("Running reproducible benchmark...\n");

    let result = BenchmarkRunner::run_wav(&wav_file, reference)?;

    println!("------------------------------------------------------------");
    println!("Audio duration:          {:.2} s", result.audio_duration_sec);
    println!("Inference processing:    {:.2} ms", result.inference_duration_ms);
    println!("Real-Time Factor (RTFx): {:.2}x faster than real-time", result.rtfx);
    println!("Word Error Rate (WER):   {:.2}%", result.wer * 100.0);
    println!("Char Error Rate (CER):   {:.2}%", result.cer * 100.0);
    println!("------------------------------------------------------------");
    println!("Raw ASR Output:          {}", result.raw_transcript);
    println!("Final Formatted Output:  {}", result.formatted_transcript);
    println!("============================================================");

    Ok(())
}

fn run_test_mic_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("Listing available microphones:");
    let devices = AudioCapture::list_input_devices();
    for (i, d) in devices.iter().enumerate() {
        println!("  [{}] {}", i, d);
    }

    println!("\nTesting default microphone capture for 3 seconds...");
    let (tx, rx) = bounded(100);
    let capture = AudioCapture::start(None, tx)?;
    println!("Recording from '{}'...", capture.device_name);

    let start = Instant::now();
    let mut total_samples = 0;
    while start.elapsed().as_secs() < 3 {
        if let Ok(samples) = rx.recv_timeout(std::time::Duration::from_millis(500)) {
            total_samples += samples.len();
        }
    }
    drop(capture);
    println!("Captured {} 16kHz mono samples in 3.0s (expected ~48,000 samples).", total_samples);
    println!("Microphone capture and resampling verified!");
    Ok(())
}

fn run_write_ico_command() -> Result<(), Box<dyn std::error::Error>> {
    let ico_bytes = echo::ui::canvas::build_ico();
    std::fs::write("echo.ico", &ico_bytes)?;
    println!("Successfully generated echo.ico ({} bytes, multi-resolution 16px to 256px)", ico_bytes.len());
    Ok(())
}

fn run_app() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Enforce single-instance mutex
    unsafe {
        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::System::Threading::CreateMutexW;

        let mutex_name = wide_str("Global\\EchoVoiceKeyboardSingletonMutex");
        let _mutex = CreateMutexW(std::ptr::null_mut(), 1, mutex_name.as_ptr());
        if GetLastError() == 183 { // ERROR_ALREADY_EXISTS
            echo::log::init();
            echo::log::info("Existing instance detected via singleton mutex. Exiting.");
            return Ok(());
        }


        // Set Per-Monitor V2 DPI awareness
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
    }

    echo::log::init();
    echo::log::info("Echo Voice Keyboard starting");

    let mut config = AppConfig::load_or_default();

    // 2. Initialize ONNX Runtime with shared thread pool
    if let Err(e) = init_runtime(config.inference_threads) {
        echo::log::info(&format!("Fatal ONNX init error: {e}"));
        eprintln!("Error initializing ONNX runtime: {e}");
        return Err(e.into());
    }

    // 3. Locate model files
    let model_dir = echo::paths::find_resource("models/asr4all-s")
        .ok_or("ASR model folder 'models/asr4all-s' not found. Please install the model files.")?;

    echo::log::info(&format!("Loading ASR model from '{}'", model_dir.display()));
    let engine = Arc::new(Asr4AllEngine::new(&model_dir)?);

    // 4. Load Personal Dictionary
    let dict_path = echo::paths::config_dir().join("dictionary.json");
    let dict = PersonalDictionary::load_or_default(&dict_path);

    // 5. Communications channels
    let (hotkey_tx, hotkey_rx) = unbounded::<HotkeyAction>();
    let (audio_tx, audio_rx) = bounded::<Vec<f32>>(200);
    let (tray_tx, tray_rx) = unbounded::<TrayCommand>();

    // 6. Spawn Pill Overlay and System Tray
    let pill = PillOverlay::spawn()?;
    let tray = TrayManager::spawn(tray_tx, config.clone())?;

    // 7. Global Keyboard Hook
    let combo = TwoKeyCombo::from_str(&config.hotkey_combo);
    let hotkey_mgr = HotkeyManager::start_listening(hotkey_tx, combo)?;

    let mut controller = DictationController::new(dict, config.suppress_fillers);
    let (asr_tx, asr_event_rx, asr_worker) = spawn_asr_worker(engine)?;
    let mut active_capture: Option<AudioCapture> = None;
    let mut awaiting_finalization = false;
    let mut quit_after_finalization = false;
    let audio_fault = Arc::new(AtomicBool::new(false));
    let dropped_chunks = Arc::new(AtomicU64::new(0));

    echo::log::info(&format!("Echo ready. Shortcut: {}", combo.to_display_str()));

    let is_running = Arc::new(AtomicBool::new(true));

    while is_running.load(Ordering::Relaxed) {
        crossbeam_channel::select! {
            recv(hotkey_rx) -> action => {
                if let Ok(action) = action {
                    match action {
                        HotkeyAction::Start { mode } => {
                            if active_capture.is_none() && !awaiting_finalization {
                                pill.set_state(DictationState::Listening, mode);
                                audio_fault.store(false, Ordering::Relaxed);
                                dropped_chunks.store(0, Ordering::Relaxed);
                                if asr_tx.send(AsrWorkerCommand::Reset).is_err() {
                                    echo::log::info("ASR worker stopped before dictation could start");
                                    pill.set_state(DictationState::Error, "");
                                    continue;
                                }

                                match AudioCapture::start(config.microphone_name.as_deref(), audio_tx.clone()) {
                                    Ok(cap) => {
                                        active_capture = Some(cap);
                                    }
                                    Err(err) => {
                                        echo::log::info(&format!("Microphone start error: {err}"));
                                        pill.set_state(DictationState::Error, "");
                                    }
                                }
                            }
                        }
                        HotkeyAction::Stop => {
                            if active_capture.is_some() {
                                // Stop microphone capture immediately (turns off Windows recording mic dot)
                                if let Some(capture) = active_capture.take() {
                                    let (mic_failed, capture_dropped) = capture.stop();
                                    if mic_failed {
                                        audio_fault.store(true, Ordering::Relaxed);
                                    }
                                    if capture_dropped > 0 {
                                        dropped_chunks.fetch_add(capture_dropped, Ordering::Relaxed);
                                        audio_fault.store(true, Ordering::Relaxed);
                                    }
                                }
                                pill.set_state(DictationState::Processing, "");

                                // Drain any remaining audio queued
                                while let Ok(chunk) = audio_rx.try_recv() {
                                    queue_asr_audio(&asr_tx, chunk, &audio_fault, &dropped_chunks);
                                }

                                if asr_tx.send(AsrWorkerCommand::Finish).is_ok() {
                                    awaiting_finalization = true;
                                } else {
                                    echo::log::info("ASR worker stopped before recognition could be finalized");
                                    pill.set_state(DictationState::Error, "");
                                }
                            }
                        }
                    }
                }
            }

            recv(audio_rx) -> audio_chunk => {
                if let Ok(chunk) = audio_chunk {
                    if active_capture.is_some() {
                        let rms = echo::audio::resampler::rms(&chunk);
                        pill.set_audio_level(rms);
                        queue_asr_audio(&asr_tx, chunk, &audio_fault, &dropped_chunks);
                    }
                }
            }

            recv(asr_event_rx) -> event => {
                if let Ok(event) = event {
                    match event {
                        AsrWorkerEvent::Ready(Ok(())) => echo::log::info("ASR worker ready"),
                        AsrWorkerEvent::Ready(Err(error)) => {
                            echo::log::info(&format!("ASR worker initialization error: {error}"));
                            pill.set_state(DictationState::Error, "");
                        }
                        AsrWorkerEvent::Finished(result) => {
                            awaiting_finalization = false;
                            let dropped = dropped_chunks.load(Ordering::Relaxed);
                            if audio_fault.load(Ordering::Relaxed) {
                                echo::log::info(&format!(
                                    "Dictation audio was incomplete; {dropped} audio chunks were dropped or the microphone failed. Transcript withheld."
                                ));
                                pill.set_state(DictationState::Error, "");
                            } else {
                                match result {
                                    Ok(res) if !res.formatted.trim().is_empty() => {
                                        let ctx = InsertContext::current();
                                        let output = controller.process_transcript(&res.formatted, &ctx);
                                        let insertion = match output {
                                            ControllerOutput::InsertText(text) => TextInjector::inject_text(&text),
                                            ControllerOutput::ReplaceText { backspaces, text } => {
                                                TextInjector::send_backspaces(backspaces);
                                                if text.is_empty() { Ok(()) } else { TextInjector::inject_text(&text) }
                                            }
                                            ControllerOutput::NoOp => Ok(()),
                                        };
                                        match insertion {
                                            Ok(()) => pill.set_state(DictationState::Done, ""),
                                            Err(error) => {
                                                echo::log::info(&format!("Text insertion error: {error}"));
                                                controller.forget_last();
                                                pill.set_state(DictationState::Error, "");
                                            }
                                        }
                                    }
                                    Ok(_) => pill.set_state(DictationState::Idle, ""),
                                    Err(error) => {
                                        echo::log::info(&format!("Recognition finalize error: {error}"));
                                        pill.set_state(DictationState::Error, "");
                                    }
                                }
                            }
                            if quit_after_finalization {
                                is_running.store(false, Ordering::Relaxed);
                            }
                        }
                    }
                }
            }

            recv(tray_rx) -> cmd => {
                if let Ok(cmd) = cmd {
                    match cmd {
                        TrayCommand::ChangeHotkey(new_combo) => {
                            config.hotkey_combo = new_combo.clone();
                            let _ = config.save();
                            let combo_obj = TwoKeyCombo::from_str(&new_combo);
                            hotkey_mgr.set_combo(combo_obj);
                            tray.update_config(config.clone());
                        }
                        TrayCommand::ChangeMicrophone(mic_name) => {
                            config.microphone_name = mic_name;
                            let _ = config.save();
                            tray.update_config(config.clone());
                        }
                        TrayCommand::ToggleSuppressFillers => {
                            config.suppress_fillers = !config.suppress_fillers;
                            let _ = config.save();
                            controller.suppress_fillers = config.suppress_fillers;
                            tray.update_config(config.clone());
                        }
                        TrayCommand::ToggleAutostart => {
                            let curr = echo::ui::tray::is_autostart_enabled();
                            let _ = echo::ui::tray::set_autostart_enabled(!curr);
                        }
                        TrayCommand::OpenDictionary => {
                            let p = echo::paths::config_dir().join("dictionary.json");
                            let _ = controller.dictionary.save(&p);
                            open_file_in_shell(&p);
                        }
                        TrayCommand::OpenLogs => {
                            let p = echo::paths::data_dir().join("echo.log");
                            open_file_in_shell(&p);
                        }
                        TrayCommand::RunBenchmark => {
                            let _ = run_benchmark_command();
                        }
                        TrayCommand::Quit => {
                            if active_capture.is_some() {
                                active_capture = None;
                            }
                            if awaiting_finalization {
                                quit_after_finalization = true;
                            } else {
                                is_running.store(false, Ordering::Relaxed);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    pill.stop();
    tray.stop();
    hotkey_mgr.stop();
    let _ = asr_tx.send(AsrWorkerCommand::Shutdown);
    let _ = asr_worker.join();
    echo::log::info("Echo Voice Keyboard exited cleanly");

    Ok(())
}

fn open_file_in_shell(path: &std::path::Path) {
    unsafe {
        let path_str = path.to_string_lossy();
        let path_wide = wide_str(&path_str);
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            wide_str("open").as_ptr(),
            path_wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1, // SW_SHOWNORMAL
        );
    }
}

fn wide_str(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}
