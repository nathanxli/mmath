use std::env;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use vosk::{CompleteResult, DecodingState, Model, Recognizer, Word};

/// Vosk wants 16 kHz mono i16 audio.
const TARGET_SAMPLE_RATE: f64 = 16000.0;

/// Words the recognizer is allowed to hear. Constraining the grammar to number
/// words is what makes recognition fast and accurate for this domain.
const GRAMMAR: &str = "zero oh one two three four five six seven eight nine ten \
    eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen \
    twenty thirty forty fifty sixty seventy eighty ninety hundred thousand \
    minus negative";

pub enum VoiceEvent {
    /// A number recognized (from a streaming partial or a final result) --
    /// apply it as the current answer immediately.
    Answer { value: i32 },
    /// Sent once the utterance finalizes: the decoder-derived instant the
    /// user finished saying `value`, for retroactive latency measurement.
    /// (Word timestamps are only available on final results when using a
    /// grammar-constrained recognizer -- enabling them on partials silences
    /// partial output entirely in libvosk 0.3.42.)
    Latency { value: i32, speech_ended_at: Instant },
}

pub struct VoiceEngine {
    // Keeps the microphone stream alive; dropping it stops capture, which in
    // turn shuts down the recognition thread.
    _stream: cpal::Stream,
    pub events: Receiver<VoiceEvent>,
}

impl VoiceEngine {
    pub fn start() -> Result<Self, String> {
        let model_path = find_model()?;

        // Keep Kaldi's logging off stderr so it can't corrupt the TUI.
        vosk::set_log_level(vosk::LogLevel::Error);

        let model = Model::new(model_path.to_string_lossy())
            .ok_or_else(|| format!("failed to load Vosk model at {}", model_path.display()))?;
        let mut recognizer =
            Recognizer::new_with_grammar(&model, TARGET_SAMPLE_RATE as f32, &[GRAMMAR, "[unk]"])
                .ok_or("failed to create Vosk recognizer")?;
        // Word-level timestamps on final results let us pin end-of-speech
        // precisely. Do NOT enable set_partial_words: with a grammar it
        // suppresses partial output altogether (libvosk 0.3.42).
        recognizer.set_words(true);

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("no microphone found (check input devices and permissions)")?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("failed to query microphone config: {}", e))?;
        let input_rate = config.sample_rate().0 as f64;
        let channels = config.channels() as usize;

        // Audio callback -> recognition thread. The callback only downmixes to
        // mono and forwards; Vosk runs on its own thread.
        let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>();
        let (event_tx, event_rx) = mpsc::channel::<VoiceEvent>();

        let err_fn = |_err: cpal::StreamError| {};
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &_| send_mono(data, channels, &audio_tx),
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &_| {
                    let floats: Vec<f32> =
                        data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                    send_mono(&floats, channels, &audio_tx);
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                &config.into(),
                move |data: &[u16], _: &_| {
                    let floats: Vec<f32> = data
                        .iter()
                        .map(|&s| (s as f32 - 32768.0) / 32768.0)
                        .collect();
                    send_mono(&floats, channels, &audio_tx);
                },
                err_fn,
                None,
            ),
            other => return Err(format!("unsupported microphone sample format: {}", other)),
        }
        .map_err(|e| format!("failed to open microphone stream: {}", e))?;
        stream
            .play()
            .map_err(|e| format!("failed to start microphone stream: {}", e))?;

        thread::spawn(move || {
            let mut resampler = Resampler::new(input_rate, TARGET_SAMPLE_RATE);
            let mut pcm: Vec<i16> = Vec::new();
            let mut samples_fed: u64 = 0;
            let mut last_partial = String::new();
            let mut last_sent: Option<i32> = None;

            while let Ok(buf) = audio_rx.recv() {
                pcm.clear();
                resampler.process(&buf, &mut pcm);
                if pcm.is_empty() {
                    continue;
                }
                samples_fed += pcm.len() as u64;

                match recognizer.accept_waveform(&pcm) {
                    Ok(DecodingState::Finalized) => {
                        // Endpoint (trailing silence): flush the final result,
                        // report the utterance's end-of-speech time, and reset
                        // per-utterance state for the next answer.
                        if let CompleteResult::Single(res) = recognizer.result() {
                            if let Some(value) = parse_answer(res.text) {
                                if last_sent != Some(value) {
                                    let _ = event_tx.send(VoiceEvent::Answer { value });
                                }
                                let _ = event_tx.send(VoiceEvent::Latency {
                                    value,
                                    speech_ended_at: speech_end_instant(&res.result, samples_fed),
                                });
                            }
                        }
                        last_sent = None;
                        last_partial.clear();
                    }
                    Ok(DecodingState::Running) => {
                        // Match on partials while the user is still speaking --
                        // waiting for the final result would add 300-500ms.
                        let partial = recognizer.partial_result();
                        if partial.partial != last_partial {
                            if let Some(value) = parse_answer(partial.partial) {
                                if last_sent != Some(value) {
                                    last_sent = Some(value);
                                    let _ = event_tx.send(VoiceEvent::Answer { value });
                                }
                            }
                            last_partial = partial.partial.to_string();
                        }
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            _stream: stream,
            events: event_rx,
        })
    }
}

/// Downmix interleaved samples to mono and forward to the recognition thread.
fn send_mono(data: &[f32], channels: usize, tx: &Sender<Vec<f32>>) {
    let mono: Vec<f32> = data
        .chunks_exact(channels.max(1))
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    let _ = tx.send(mono);
}

/// Decoder-authoritative end-of-speech instant: the end timestamp of the last
/// number word, mapped from audio-stream time to wall clock. Vosk word times
/// are stream-absolute seconds, and "now" corresponds to stream position
/// `samples_fed / 16000`, so end-of-speech was `stream_secs - word_end` ago.
fn speech_end_instant(words: &[Word], samples_fed: u64) -> Instant {
    let now = Instant::now();
    let stream_secs = samples_fed as f64 / TARGET_SAMPLE_RATE;
    let last_number_end = words
        .iter()
        .rev()
        .find(|w| is_number_word(w.word))
        .map(|w| w.end as f64);
    match last_number_end {
        // Guard against nonsense timestamps; fall back to "now" (latency 0).
        Some(end) if (0.0..=10.0).contains(&(stream_secs - end)) => {
            now - Duration::from_secs_f64(stream_secs - end)
        }
        _ => now,
    }
}

fn find_model() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("MMATH_VOSK_MODEL") {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Ok(path);
        }
        return Err(format!(
            "MMATH_VOSK_MODEL is set but {} is not a directory",
            path.display()
        ));
    }

    let mut roots = vec![PathBuf::from("models")];
    // Also look relative to the executable (target/debug/mmath -> project root).
    if let Ok(exe) = env::current_exe() {
        if let Some(root) = exe.ancestors().nth(3) {
            roots.push(root.join("models"));
        }
    }
    for root in roots {
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir()
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("vosk-model"))
                {
                    return Ok(path);
                }
            }
        }
    }
    Err("no Vosk model found; run scripts/fetch-voice-assets.sh or set MMATH_VOSK_MODEL".into())
}

/// Linear-interpolation resampler from the microphone rate to 16 kHz.
/// Carries fractional position and the last sample across buffers.
struct Resampler {
    step: f64,
    pos: f64,
    last: f32,
}

impl Resampler {
    fn new(in_rate: f64, out_rate: f64) -> Self {
        Self {
            step: in_rate / out_rate,
            pos: 0.0,
            last: 0.0,
        }
    }

    fn process(&mut self, input: &[f32], out: &mut Vec<i16>) {
        if input.is_empty() {
            return;
        }
        // Virtual buffer: [self.last, input[0], ..., input[n-1]], pos measured
        // from self.last at index 0.
        let n = input.len();
        let mut pos = self.pos;
        while pos < n as f64 {
            let i = pos as usize;
            let frac = pos - i as f64;
            let a = if i == 0 { self.last } else { input[i - 1] };
            let b = input[i];
            let sample = a + (b - a) * frac as f32;
            out.push((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
            pos += self.step;
        }
        self.pos = pos - n as f64;
        self.last = input[n - 1];
    }
}

/// Parse a recognized transcript into an integer answer.
///
/// Takes the last contiguous run of number words (ignoring `[unk]` and other
/// noise) and parses the longest suffix that forms a valid number, so stale
/// words earlier in the utterance don't block a match.
pub fn parse_answer(text: &str) -> Option<i32> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let last_run = tokens
        .split(|t| !is_number_word(t))
        .rev()
        .find(|run| !run.is_empty())?;
    for start in 0..last_run.len() {
        if let Some(value) = parse_run(&last_run[start..]) {
            return Some(value);
        }
    }
    None
}

fn is_number_word(word: &str) -> bool {
    word == "minus"
        || word == "negative"
        || word == "hundred"
        || word == "thousand"
        || digit_value(word).is_some()
        || teen_value(word).is_some()
        || tens_value(word).is_some()
}

fn digit_value(word: &str) -> Option<i64> {
    Some(match word {
        "zero" | "oh" => 0,
        "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        _ => return None,
    })
}

fn teen_value(word: &str) -> Option<i64> {
    Some(match word {
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        "thirteen" => 13,
        "fourteen" => 14,
        "fifteen" => 15,
        "sixteen" => 16,
        "seventeen" => 17,
        "eighteen" => 18,
        "nineteen" => 19,
        _ => return None,
    })
}

fn tens_value(word: &str) -> Option<i64> {
    Some(match word {
        "twenty" => 20,
        "thirty" => 30,
        "forty" => 40,
        "fifty" => 50,
        "sixty" => 60,
        "seventy" => 70,
        "eighty" => 80,
        "ninety" => 90,
        _ => return None,
    })
}

fn parse_run(words: &[&str]) -> Option<i32> {
    let (negative, words) = match words.split_first()? {
        (&"minus", rest) | (&"negative", rest) => (true, rest),
        _ => (false, words),
    };
    if words.is_empty() {
        return None;
    }

    // Digit-by-digit style: "one two three" -> 123, "four oh five" -> 405.
    // (A lone "zero" also lands here via the standard path below.)
    if words.len() >= 2 && words.iter().all(|w| digit_value(w).is_some()) {
        if words.len() > 7 {
            return None;
        }
        let mut value: i64 = 0;
        for w in words {
            value = value * 10 + digit_value(w).unwrap();
        }
        return finish(value, negative);
    }

    parse_standard(words).and_then(|v| finish(v, negative))
}

fn finish(value: i64, negative: bool) -> Option<i32> {
    let value = if negative { -value } else { value };
    i32::try_from(value).ok()
}

#[derive(PartialEq, Clone, Copy)]
enum LastWord {
    None,
    Unit,
    Teen,
    Tens,
    Hundred,
    Thousand,
}

/// Standard English number grammar: "one hundred twenty three", "forty two",
/// "twelve thousand fifty". Rejects sequences like "twelve fifteen" so the
/// caller can fall back to a shorter suffix.
fn parse_standard(words: &[&str]) -> Option<i64> {
    // A lone "zero" is a real answer; a lone "oh" is more likely noise.
    if words == ["zero"] {
        return Some(0);
    }

    let mut total: i64 = 0;
    let mut current: i64 = 0;
    let mut last = LastWord::None;

    for &word in words {
        if word == "zero" || word == "oh" {
            return None; // only valid alone or in digit-by-digit mode
        } else if let Some(v) = digit_value(word) {
            if !matches!(last, LastWord::None | LastWord::Tens | LastWord::Hundred | LastWord::Thousand) {
                return None;
            }
            current += v;
            last = LastWord::Unit;
        } else if let Some(v) = teen_value(word) {
            if !matches!(last, LastWord::None | LastWord::Hundred | LastWord::Thousand) {
                return None;
            }
            current += v;
            last = LastWord::Teen;
        } else if let Some(v) = tens_value(word) {
            if !matches!(last, LastWord::None | LastWord::Hundred | LastWord::Thousand) {
                return None;
            }
            current += v;
            last = LastWord::Tens;
        } else if word == "hundred" {
            if !matches!(last, LastWord::Unit | LastWord::Teen) || !(1..=99).contains(&current) {
                return None;
            }
            current *= 100;
            last = LastWord::Hundred;
        } else if word == "thousand" {
            if last == LastWord::None || last == LastWord::Thousand || current == 0 {
                return None;
            }
            total += current * 1000;
            current = 0;
            last = LastWord::Thousand;
        } else {
            return None;
        }
    }

    if last == LastWord::None {
        return None;
    }
    Some(total + current)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RMS level (on i16 samples) above which a chunk counts as speech.
    /// Only used by the latency probe as a sanity reference against the
    /// decoder's word-end timestamps.
    const SPEECH_RMS_THRESHOLD: f64 = 300.0;

    fn rms(samples: &[i16]) -> f64 {
        let sum: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
        (sum / samples.len() as f64).sqrt()
    }

    #[test]
    fn parses_standard_numbers() {
        assert_eq!(parse_answer("zero"), Some(0));
        assert_eq!(parse_answer("seven"), Some(7));
        assert_eq!(parse_answer("twelve"), Some(12));
        assert_eq!(parse_answer("forty"), Some(40));
        assert_eq!(parse_answer("forty two"), Some(42));
        assert_eq!(parse_answer("one hundred"), Some(100));
        assert_eq!(parse_answer("one hundred five"), Some(105));
        assert_eq!(parse_answer("one hundred twenty three"), Some(123));
        assert_eq!(parse_answer("nineteen hundred"), Some(1900));
        assert_eq!(parse_answer("two thousand"), Some(2000));
        assert_eq!(parse_answer("twelve thousand fifty"), Some(12050));
        assert_eq!(parse_answer("one thousand two hundred thirty four"), Some(1234));
    }

    #[test]
    fn parses_digit_by_digit() {
        assert_eq!(parse_answer("one two three"), Some(123));
        assert_eq!(parse_answer("four oh five"), Some(405));
        assert_eq!(parse_answer("nine nine"), Some(99));
    }

    #[test]
    fn parses_negative() {
        assert_eq!(parse_answer("minus three"), Some(-3));
        assert_eq!(parse_answer("negative forty two"), Some(-42));
    }

    #[test]
    fn takes_last_number_in_utterance() {
        // Stale words from the same utterance must not block a match.
        assert_eq!(parse_answer("twelve fifteen"), Some(15));
        assert_eq!(parse_answer("[unk] forty two"), Some(42));
        assert_eq!(parse_answer("seven [unk] thirty"), Some(30));
    }

    #[test]
    fn rejects_junk() {
        assert_eq!(parse_answer(""), None);
        assert_eq!(parse_answer("[unk]"), None);
        assert_eq!(parse_answer("hundred"), None);
        assert_eq!(parse_answer("minus"), None);
        assert_eq!(parse_answer("oh"), None);
    }

    #[test]
    fn resampler_downsamples_3x() {
        let mut r = Resampler::new(48000.0, 16000.0);
        let input: Vec<f32> = (0..480).map(|i| (i as f32 / 480.0) * 0.5).collect();
        let mut out = Vec::new();
        r.process(&input, &mut out);
        assert_eq!(out.len(), 160);
        // Monotone ramp in -> monotone ramp out.
        assert!(out.windows(2).all(|w| w[0] <= w[1]));
    }

    /// End-to-end pipeline check: synthesize speech with macOS `say`, run it
    /// through the grammar-constrained recognizer, and parse the result.
    /// Skips (passes) when the model or `say` is unavailable.
    #[test]
    fn recognizes_synthesized_speech() {
        let Ok(model_path) = find_model() else {
            eprintln!("skipping: no Vosk model available");
            return;
        };
        let wav_path = std::env::temp_dir().join("mmath_voice_test.wav");
        let status = std::process::Command::new("say")
            .args(["-o"])
            .arg(&wav_path)
            .args(["--data-format=LEI16@16000", "forty two"])
            .status();
        let Ok(status) = status else {
            eprintln!("skipping: `say` unavailable");
            return;
        };
        assert!(status.success(), "say failed to synthesize test audio");

        let mut reader = hound::WavReader::open(&wav_path).expect("failed to read test wav");
        let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();

        vosk::set_log_level(vosk::LogLevel::Error);
        let model = Model::new(model_path.to_string_lossy()).expect("failed to load model");
        let mut rec = Recognizer::new_with_grammar(&model, 16000.0, &[GRAMMAR, "[unk]"])
            .expect("failed to create recognizer");
        rec.accept_waveform(&samples).expect("accept_waveform failed");
        let text = match rec.final_result() {
            CompleteResult::Single(r) => r.text.to_string(),
            CompleteResult::Multiple(_) => panic!("unexpected multiple results"),
        };
        assert_eq!(
            parse_answer(&text),
            Some(42),
            "recognized text was: {:?}",
            text
        );
    }

    /// Offline probe of partial-match latency: streams synthesized speech in
    /// 20ms chunks and reports how far past the end of speech the recognizer
    /// first produced a parsable match, plus per-chunk compute time.
    /// Run with: cargo test partial_latency_probe -- --ignored --nocapture
    #[test]
    #[ignore]
    fn partial_latency_probe() {
        let model_path = find_model().expect("no Vosk model available");
        let wav_path = std::env::temp_dir().join("mmath_latency_probe.wav");
        let status = std::process::Command::new("say")
            .args(["-o"])
            .arg(&wav_path)
            .args(["--data-format=LEI16@16000", "forty two"])
            .status()
            .expect("`say` unavailable");
        assert!(status.success());

        let mut reader = hound::WavReader::open(&wav_path).unwrap();
        let mut samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        // A live microphone keeps delivering silence after the user stops
        // speaking; emulate that so the decoder can settle.
        samples.extend(std::iter::repeat(0i16).take(16000));

        vosk::set_log_level(vosk::LogLevel::Error);
        let model = Model::new(model_path.to_string_lossy()).unwrap();
        let mut rec = Recognizer::new_with_grammar(&model, 16000.0, &[GRAMMAR, "[unk]"]).unwrap();
        rec.set_words(true);

        const CHUNK: usize = 320; // 20ms at 16kHz
        let mut samples_fed: u64 = 0;
        let mut speech_end_chunk = 0;
        let mut match_chunk = None;
        let mut latency_ms = None;
        let mut last_partial = String::new();
        let mut max_compute = std::time::Duration::ZERO;
        for (i, chunk) in samples.chunks(CHUNK).enumerate() {
            if rms(chunk) > SPEECH_RMS_THRESHOLD {
                speech_end_chunk = i;
            }
            samples_fed += chunk.len() as u64;
            let t = Instant::now();
            let state = rec.accept_waveform(chunk).unwrap();
            match state {
                DecodingState::Running => {
                    let partial = rec.partial_result().partial.to_string();
                    if partial != last_partial {
                        println!("chunk {:3}: partial {:?}", i, partial);
                        last_partial = partial;
                    }
                    if match_chunk.is_none() && parse_answer(&last_partial) == Some(42) {
                        match_chunk = Some(samples_fed);
                    }
                }
                DecodingState::Finalized => {
                    // Retroactive latency, exactly as the runtime computes it:
                    // when the match was applied minus the decoder's word-end.
                    if let (CompleteResult::Single(res), Some(match_samples)) =
                        (rec.result(), match_chunk)
                    {
                        let end = res
                            .result
                            .iter()
                            .rev()
                            .find(|w| is_number_word(w.word))
                            .map(|w| w.end as f64)
                            .expect("final result had no word timestamps");
                        latency_ms =
                            Some((match_samples as f64 / 16000.0 - end) * 1000.0);
                    }
                }
                _ => {}
            }
            max_compute = max_compute.max(t.elapsed());
        }

        let match_samples = match_chunk.expect("never matched 42 from partials");
        let latency_ms = latency_ms.expect("utterance never finalized");
        println!(
            "RMS speech end ~{:.2}s, partial match at {:.2}s stream time",
            speech_end_chunk as f64 * 0.02,
            match_samples as f64 / 16000.0,
        );
        println!(
            "decoder word-end latency: {:.0}ms (end of last word -> match applied)",
            latency_ms
        );
        println!("max per-chunk compute: {:?}", max_compute);
        // A large or negative value means the stream-time mapping is wrong
        // (e.g. timestamps are not stream-absolute).
        assert!(
            (0.0..=500.0).contains(&latency_ms),
            "implausible word-end latency: {:.0}ms",
            latency_ms
        );
    }
}
