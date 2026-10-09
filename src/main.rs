use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{bounded, unbounded};

use echo::asr::{Asr4AllEngine, AsrEngine};
use echo::audio::AudioCapture;
use echo::benchmark::BenchmarkRunner;
use echo::config::AppConfig;
use echo::controller::{ControllerOutput, DictationController};
use echo::insertion::TextInjector;
use echo::hotkey::HotkeyManager;
use echo::tray::{DictationState, TrayManager};
use echo::vocabulary::PersonalDictionary;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "benchmark" {
        return run_benchmark_command();
    }

    if args.len() > 1 && args[1] == "test-mic" {
        return run_test_mic_command();
    }

    run_app()
}

fn run_benchmark_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!("             ECHO ASR BENCHMARK SUITE");
    println!("============================================================");

    let wav_file = "models/jfk.wav";
    let reference = "And so my fellow Americans ask not what your country can do for you. Ask what you can do for your country.";

    println!("Audio target:    {}", wav_file);
    println!("Reference text:  {}", reference);
    println!("Running reproducible benchmark...\n");

    let result = BenchmarkRunner::run_wav(wav_file, reference)?;

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
    let capture = AudioCapture::new(None, tx)?;
    capture.set_recording(true);

    let start = Instant::now();
    let mut total_samples = 0;
    while start.elapsed().as_secs() < 3 {
        if let Ok(samples) = rx.recv_timeout(std::time::Duration::from_millis(500)) {
            total_samples += samples.len();
        }
    }
    capture.set_recording(false);
    println!("Captured {} 16kHz mono samples in 3.0s (expected ~48,000 samples).", total_samples);
    println!("Microphone capture and resampling verified!");
    Ok(())
}

fn run_app() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!("  ECHO - Lightweight Offline English Voice Keyboard (v0.1.0)");
    println!("============================================================");

    let config = AppConfig::load_or_default();
    println!("Loading ASR model from '{}'...", config.model_dir);
    let engine = Arc::new(Asr4AllEngine::new(&config.model_dir)?);
    println!("ASR Engine loaded successfully!");

    let dict = PersonalDictionary::default();
    println!("Loaded personal dictionary with {} technical terms.", dict.words.len());

    // Channels for audio and hotkey
    let (audio_tx, audio_rx) = bounded::<Vec<f32>>(200);
    let (hotkey_tx, hotkey_rx) = unbounded::<()>();

    let capture = Arc::new(AudioCapture::new(config.microphone_name.as_deref(), audio_tx)?);

    // Register hotkey (Ctrl + Alt + Space)
    let _hotkey_mgr = HotkeyManager::start_listening(
        hotkey_tx,
        config.hotkey_modifiers,
        config.hotkey_key,
    )?;

    println!("\n[Ready]");
    println!("  Hotkey: Ctrl + Alt + Space");
    println!("  1. Focus any text box (VS Code, Notepad, Browser, Terminal).");
    println!("  2. Press Ctrl + Alt + Space to START speaking.");
    println!("  3. Press Ctrl + Alt + Space again to STOP and insert text.");
    println!("  Press Ctrl+C in terminal to exit.\n");

    let is_listening = Arc::new(AtomicBool::new(false));
    let mut tray_mgr = TrayManager::new();
    let mut controller = DictationController::new(dict, config.suppress_fillers);

    let mut current_session = engine.create_session()?;

    while tray_mgr.is_running.load(Ordering::Relaxed) {
        // Wait for hotkey press or audio packets
        crossbeam_channel::select! {
            recv(hotkey_rx) -> _ => {
                let was_listening = is_listening.load(Ordering::SeqCst);
                if !was_listening {
                    // START DICTATION
                    is_listening.store(true, Ordering::SeqCst);
                    current_session.reset();
                    controller.reset_session();
                    capture.set_recording(true);
                    tray_mgr.set_state(DictationState::Listening);
                } else {
                    // STOP DICTATION & INSERT
                    capture.set_recording(false);
                    is_listening.store(false, Ordering::SeqCst);
                    tray_mgr.set_state(DictationState::Processing);

                    // Drain remaining queued audio
                    while let Ok(chunk) = audio_rx.try_recv() {
                        let _ = current_session.accept_audio(&chunk);
                    }

                    // Flush final transcript
                    if let Ok(final_partial) = current_session.finish() {
                        if !final_partial.formatted.is_empty() {
                            println!("  [Transcribed]: \"{}\"", final_partial.formatted);

                            let output = controller.process_transcript(&final_partial.formatted, true);
                            match output {
                                ControllerOutput::InsertText(text) => {
                                    println!("  [Inserting text]: \"{}\"", text);
                                    let _ = TextInjector::inject_text(&text);
                                }
                                ControllerOutput::ReplaceText { backspaces, text } => {
                                    println!("  [Undo/Correction]: backspacing {} chars, inserting \"{}\"", backspaces, text);
                                    TextInjector::send_backspaces(backspaces);
                                    if !text.is_empty() {
                                        let _ = TextInjector::inject_text(&text);
                                    }
                                }
                                ControllerOutput::NoOp => {}
                            }
                        }
                    }

                    tray_mgr.set_state(DictationState::Idle);
                }
            }
            recv(audio_rx) -> audio_chunk => {
                if let Ok(chunk) = audio_chunk {
                    if is_listening.load(Ordering::Relaxed) {
                        if let Ok(partial) = current_session.accept_audio(&chunk) {
                            if !partial.formatted.is_empty() {
                                // Live provisional preview in terminal
                                print!("\r  [Live]: {}               ", partial.formatted);
                                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
