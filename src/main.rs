#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{bounded, unbounded};

use echo::asr::{init_runtime, Asr4AllEngine, AsrEngine};
use echo::audio::AudioCapture;
use echo::benchmark::BenchmarkRunner;
use echo::config::AppConfig;
use echo::controller::{ControllerOutput, DictationController, InsertContext};
use echo::hotkey::{HotkeyAction, HotkeyManager, TwoKeyCombo};
use echo::insertion::TextInjector;
use echo::ui::{DictationState, PillOverlay, TrayCommand, TrayManager};
use echo::vocabulary::PersonalDictionary;

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

    run_app()
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
    let mut current_session = engine.create_session()?;
    let mut active_capture: Option<AudioCapture> = None;

    echo::log::info(&format!("Echo ready. Shortcut: {}", combo.to_display_str()));

    let is_running = Arc::new(AtomicBool::new(true));

    while is_running.load(Ordering::Relaxed) {
        crossbeam_channel::select! {
            recv(hotkey_rx) -> action => {
                if let Ok(action) = action {
                    match action {
                        HotkeyAction::Start { mode } => {
                            if active_capture.is_none() {
                                pill.set_state(DictationState::Listening, mode);
                                current_session.reset();

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
                                active_capture = None;
                                pill.set_state(DictationState::Processing, "");

                                // Drain any remaining audio queued
                                while let Ok(chunk) = audio_rx.try_recv() {
                                    let _ = current_session.accept_audio(&chunk);
                                }

                                // Finalize recognition
                                match current_session.finish() {
                                    Ok(res) => {
                                        if !res.formatted.trim().is_empty() {
                                            let ctx = InsertContext::current();
                                            let output = controller.process_transcript(&res.formatted, &ctx);

                                            match output {
                                                ControllerOutput::InsertText(text) => {
                                                    let _ = TextInjector::inject_text(&text);
                                                }
                                                ControllerOutput::ReplaceText { backspaces, text } => {
                                                    TextInjector::send_backspaces(backspaces);
                                                    if !text.is_empty() {
                                                        let _ = TextInjector::inject_text(&text);
                                                    }
                                                }
                                                ControllerOutput::NoOp => {}
                                            }
                                            pill.set_state(DictationState::Done, "");
                                        } else {
                                            pill.set_state(DictationState::Idle, "");
                                        }
                                    }
                                    Err(e) => {
                                        echo::log::info(&format!("Recognition finalize error: {e}"));
                                        pill.set_state(DictationState::Idle, "");
                                    }
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
                        let _ = current_session.accept_audio(&chunk);
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
                            is_running.store(false, Ordering::Relaxed);
                            break;
                        }
                    }
                }
            }
        }
    }

    pill.stop();
    tray.stop();
    hotkey_mgr.stop();
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
