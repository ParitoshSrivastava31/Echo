# Technical dictation benchmark seed

`data/technical-dictation-seed.jsonl` is an original, project-authored set of 58 utterances for a developer-focused dictation benchmark. It covers coding requests, debugging, technical names, prompt instructions, spoken code-like forms, numbers, self-corrections, and ordinary developer speech.

This is reference text, not recorded speech and not training data. It cannot measure accent robustness or improve the ASR model on its own. Record the same prompts through the laptop microphone and a headset, then compare each transcript with the matching `text` field. Include spontaneous dictation as a separate set because reading prepared text does not reproduce how users give coding instructions.

## Recording protocol

- Use multiple consenting English speakers, including Indian English speakers with different first-language backgrounds where available.
- Record each prompt in a quiet room using the normal dictation setup. Keep the original audio and use an uncompressed or lossless format.
- Record microphone type, sample rate, speaker ID pseudonym, prompt ID, and whether the take was read or spoken naturally. Do not collect names or other personal information.
- Have a second person verify the reference transcript against the audio. Preserve the spoken form in a verbatim transcript and use the prompt text as a normalized reference only when it matches what was actually spoken.
- Keep a fixed held-out set of speakers and prompts. Do not tune a dictionary or model on the same clips used for the final score.
- Measure raw ASR output separately from dictionary-corrected output. Report WER, technical-term accuracy, omission rate, first-partial latency, finalization latency, CPU, and peak memory.

## Dictionary policy

Do not turn every term in this benchmark into a default global replacement. The current dictionary runs after ASR and changes exact words; it does not bias acoustic decoding. A rule such as `react` to `React` would corrupt ordinary prose like “react quickly.” Add a replacement only after the benchmark shows a recurring, unambiguous error. Keep ambiguous brands and specialized stacks in optional local packs or the user's own dictionary.

For model-level gains, the ASR engine must accept contextual vocabulary or be adapted with licensed audio and accurate transcripts. The current FUTO integration exposes neither a phrase-biasing input nor an adaptation path, so a text list alone will not teach it new acoustic patterns. Preserve the raw transcript and compare it with the post-processed result so dictionary gains do not get mistaken for recognition gains.

## Licensing and privacy

The seed text is authored for this project and is covered by the repository license. Do not commit contributor recordings or transcripts without explicit permission that covers public redistribution and model evaluation. Keep third-party datasets separate, record their exact license and citation, and follow their access terms. In particular, use external speech sets as separately downloaded evaluation inputs unless their terms explicitly permit redistribution.
