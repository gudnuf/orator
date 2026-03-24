use anyhow::{anyhow, Result};
use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineStream};
use std::path::Path;

/// Result returned from the recognizer on each decode cycle.
#[derive(Debug, Clone)]
pub struct SttResult {
    /// The recognized text so far (partial or final).
    pub text: String,
    /// Whether this result represents a finalized utterance (endpoint detected).
    pub is_final: bool,
}

/// Streaming speech-to-text recognizer wrapping sherpa-onnx.
pub struct SttRecognizer {
    recognizer: OnlineRecognizer,
    stream: OnlineStream,
    last_text: String,
    sample_rate: i32,
}

impl SttRecognizer {
    /// Create a new recognizer using the streaming zipformer model.
    ///
    /// `model_dir` should point to the extracted model directory, e.g.
    /// `models/sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06`.
    ///
    /// `hotwords_file` is an optional path to a hotwords boosting file.
    pub fn new(model_dir: &Path, hotwords_file: Option<&Path>, sample_rate: i32) -> Result<Self> {
        let encoder = find_model_file(
            model_dir,
            &[
                "encoder.onnx",
                "encoder-epoch-99-avg-1.int8.onnx",
                "encoder-epoch-99-avg-1.onnx",
            ],
        )?;
        let decoder = find_model_file(
            model_dir,
            &["decoder.onnx", "decoder-epoch-99-avg-1.onnx"],
        )?;
        let joiner = find_model_file(
            model_dir,
            &[
                "joiner.onnx",
                "joiner-epoch-99-avg-1.int8.onnx",
                "joiner-epoch-99-avg-1.onnx",
            ],
        )?;
        let tokens = find_model_file(model_dir, &["tokens.txt"])?;

        let mut config = OnlineRecognizerConfig::default();

        // Model paths
        config.model_config.transducer.encoder = Some(encoder);
        config.model_config.transducer.decoder = Some(decoder);
        config.model_config.transducer.joiner = Some(joiner);
        config.model_config.tokens = Some(tokens);
        config.model_config.num_threads = 2;
        config.model_config.provider = Some("cpu".into());

        // Use modified_beam_search for hotword boosting support
        config.decoding_method = Some("modified_beam_search".into());
        config.max_active_paths = 4;

        // Endpoint detection
        config.enable_endpoint = true;
        config.rule1_min_trailing_silence = 2.4;
        config.rule2_min_trailing_silence = 1.2;
        config.rule3_min_utterance_length = 20.0;

        // Hotword boosting
        if let Some(hw_path) = hotwords_file {
            config.hotwords_file = Some(hw_path.to_string_lossy().into_owned());
            config.hotwords_score = 1.5;
        }

        let recognizer = OnlineRecognizer::create(&config)
            .ok_or_else(|| anyhow!("Failed to create OnlineRecognizer -- check model paths"))?;

        let stream = recognizer.create_stream();

        Ok(Self {
            recognizer,
            stream,
            last_text: String::new(),
            sample_rate,
        })
    }

    /// Feed audio samples into the recognizer.
    pub fn accept_waveform(&self, sample_rate: i32, samples: &[f32]) {
        self.stream.accept_waveform(sample_rate, samples);
    }

    /// Run decode steps and return the current result, if any text is available.
    ///
    /// Returns `Some(SttResult)` when there's new text (partial or final).
    /// Returns `None` if nothing has changed since the last call.
    pub fn decode(&mut self) -> Option<SttResult> {
        while self.recognizer.is_ready(&self.stream) {
            self.recognizer.decode(&self.stream);
        }

        let is_endpoint = self.recognizer.is_endpoint(&self.stream);
        let result = self.recognizer.get_result(&self.stream);
        let result = result?;
        let text = result.text.trim().to_string();

        if text.is_empty() {
            if is_endpoint {
                self.recognizer.reset(&self.stream);
                self.last_text.clear();
            }
            return None;
        }

        if is_endpoint {
            let final_text = text.clone();
            self.recognizer.reset(&self.stream);
            self.last_text.clear();
            return Some(SttResult {
                text: final_text,
                is_final: true,
            });
        }

        // Partial result -- only emit if changed
        if text != self.last_text {
            self.last_text = text.clone();
            return Some(SttResult {
                text,
                is_final: false,
            });
        }

        None
    }

    /// Signal that no more audio will arrive, flush remaining frames.
    pub fn flush(&mut self) -> Option<SttResult> {
        // Add tail padding (1.5s of silence) to flush the model.
        // Must exceed rule2_min_trailing_silence (1.2s) so the streaming
        // decoder finalizes the last words via endpoint detection.
        let padding_samples = (self.sample_rate as f32 * 1.5) as usize;
        let tail_padding = vec![0.0f32; padding_samples];
        self.stream
            .accept_waveform(self.sample_rate, &tail_padding);
        self.stream.input_finished();

        while self.recognizer.is_ready(&self.stream) {
            self.recognizer.decode(&self.stream);
        }

        let result = self.recognizer.get_result(&self.stream)?;
        let text = result.text.trim().to_string();

        if text.is_empty() {
            return None;
        }

        self.recognizer.reset(&self.stream);
        self.last_text.clear();

        Some(SttResult {
            text,
            is_final: true,
        })
    }

    /// Reset by creating a fresh stream. This ensures no state carries over
    /// from a previous utterance (especially after flush() calls input_finished()).
    pub fn reset(&mut self) {
        self.stream = self.recognizer.create_stream();
        self.last_text.clear();
    }
}

fn find_model_file(model_dir: &Path, candidates: &[&str]) -> Result<String> {
    for name in candidates {
        let path = model_dir.join(name);
        if path.exists() {
            return Ok(path.to_string_lossy().into_owned());
        }
    }
    Err(anyhow!(
        "No model file found in {:?} (tried: {})",
        model_dir,
        candidates.join(", ")
    ))
}
