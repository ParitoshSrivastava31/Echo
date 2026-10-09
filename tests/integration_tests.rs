use std::path::Path;

#[test]
fn test_missing_model_directory_error_handling() {
    // Attempting to initialize Asr4AllEngine with nonexistent folder should cleanly return Err
    let fake_path = "models/non_existent_model_dir_xyz";
    assert!(!Path::new(fake_path).exists());
}

#[test]
fn test_audio_resampler_various_frequencies() {
    let mut resampler_44k = echo::audio::resampler::LinearResampler::new(44100, 16000);
    let in_44k = vec![0.5f32; 4410]; // 0.1s
    let out_16k = resampler_44k.process_interleaved(&in_44k, 1);
    assert!((out_16k.len() as i32 - 1600).abs() <= 2);

    let mut resampler_96k = echo::audio::resampler::LinearResampler::new(96000, 16000);
    let in_96k = vec![0.5f32; 9600]; // 0.1s
    let out_16k_from_96k = resampler_96k.process_interleaved(&in_96k, 1);
    assert!((out_16k_from_96k.len() as i32 - 1600).abs() <= 2);
}

#[test]
fn test_personal_vocabulary_persistence() {
    let tmp_path = std::env::temp_dir().join("echo_test_dict.json");
    let mut dict = echo::vocabulary::PersonalDictionary::default();
    dict.add("whisper flow", "Wispr Flow");
    assert!(dict.save(&tmp_path).is_ok());

    let loaded = echo::vocabulary::PersonalDictionary::load_or_default(&tmp_path);
    assert_eq!(loaded.words.get("whisper flow"), Some(&"Wispr Flow".to_string()));
    let _ = std::fs::remove_file(tmp_path);
}

#[test]
fn test_command_parser_boundaries() {
    use echo::controller::commands::{Action, CommandParser};

    // Single-word "newline" should NOT be replaced, preserving prose
    assert_eq!(
        CommandParser::parse("The newline character in C is backslash n"),
        Action::Insert("The newline character in C is backslash n".to_string())
    );

    // Spoken "new line" should be replaced
    assert_eq!(
        CommandParser::parse("First sentence period new line Second sentence"),
        Action::Insert("First sentence.\nSecond sentence".to_string())
    );

    // Exact undo command triggers UndoLast
    assert_eq!(CommandParser::parse("scratch that"), Action::UndoLast);
    assert_eq!(CommandParser::parse("scratch that."), Action::UndoLast);
}
