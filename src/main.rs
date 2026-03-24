mod audio;
mod hotkey;
mod inject;
mod overlay;
mod recognizer;

use anyhow::Result;
use overlay::OverlayMsg;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

fn main() -> Result<()> {
    let (overlay_tx, overlay_rx) = mpsc::channel::<OverlayMsg>();

    // Spawn the voice processing loop on a background thread.
    // macOS requires the UI (overlay) to run on the main thread.
    let voice_tx = overlay_tx.clone();
    std::thread::spawn(move || {
        if let Err(e) = voice_loop(voice_tx) {
            eprintln!("Voice loop error: {}", e);
        }
    });

    // Run the overlay on the main thread (blocks until quit).
    let state = overlay::OverlayState {
        receiver: overlay_rx,
    };
    overlay::run_overlay(state);

    Ok(())
}

/// Resolve the base directory for runtime resources (models, hotwords).
///
/// Priority:
///   1. `ORATOR_DATA_DIR` environment variable
///   2. The directory containing the executable (works for nix run / .app bundles)
///   3. Current working directory (fallback for cargo run during development)
fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ORATOR_DATA_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            // In a nix package the binary lives at $out/bin/orator and the
            // model lives at $out/share/orator/models/... — go up one level
            // from bin/ to reach the package root, then into share/orator.
            let share_dir = parent.join("../share/orator");
            if share_dir.exists() {
                return share_dir;
            }
            return parent.to_path_buf();
        }
    }
    PathBuf::from(".")
}

fn voice_loop(overlay_tx: mpsc::Sender<OverlayMsg>) -> Result<()> {
    let base = data_dir();
    let model_dir = base.join("models/sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06");

    // Also check relative to cwd for development
    let model_dir = if model_dir.exists() {
        model_dir
    } else {
        let cwd_model = Path::new("models/sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06");
        if cwd_model.exists() {
            cwd_model.to_path_buf()
        } else {
            eprintln!(
                "Model not found. Searched:\n  {}\n  {}\nRun: ./scripts/download-model.sh",
                model_dir.display(),
                cwd_model.display()
            );
            std::process::exit(1);
        }
    };

    let hotwords_candidates = [
        base.join("hotwords.txt"),
        PathBuf::from("hotwords.txt"),
    ];
    let hotwords_file = hotwords_candidates.iter().find(|p| p.exists());

    if hotwords_file.is_none() {
        eprintln!("Note: hotwords.txt not found, running without hotword boosting");
    }

    let capture = audio::AudioCapture::new()?;
    let sample_rate = capture.sample_rate;

    let mut stt = recognizer::SttRecognizer::new(
        &model_dir,
        hotwords_file.map(|p| p.as_path()),
        sample_rate,
    )?;

    let mut injector = inject::TextInjector::new()?;

    // Start global hotkey listener, sharing the recording AtomicBool from AudioCapture
    hotkey::start_hotkey_listener(capture.recording.clone());

    eprintln!("Orator ready. Hold Right Option to speak. Release to commit. Ctrl+C to quit.");

    // Install Ctrl+C handler for clean shutdown
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    let quit_tx = overlay_tx.clone();
    ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = quit_tx.send(OverlayMsg::Quit);
    })?;

    let mut was_recording = false;
    // Accumulates finalized text segments from endpoint resets during a single
    // recording session. When the recognizer hits an endpoint (e.g. after 20s
    // of continuous speech), it resets its stream and subsequent get_result()
    // calls only return text since the reset. We must accumulate the finalized
    // segments so we don't lose earlier text.
    let mut committed_text = String::new();
    // Holds the latest decoded text (partial or final) from the current recording
    // session, to be injected into the active window on key release.
    let mut pending_text: Option<String> = None;

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        let is_recording = capture.is_recording();

        // Detect transition: recording -> not recording (key released)
        if was_recording && !is_recording {
            let _ = overlay_tx.send(OverlayMsg::Hide);

            // Drain any remaining buffered audio
            while let Ok(samples) = capture.receiver.recv_timeout(Duration::from_millis(5)) {
                stt.accept_waveform(sample_rate, &samples);
            }

            // Flush STT to get any remaining text
            let flush_result = stt.flush();

            eprint!("\r\x1b[2K");
            io::stderr().flush().ok();

            // Choose the best tail text: flush result if available (most
            // complete), otherwise the last streaming partial we stashed.
            let tail = flush_result
                .map(|r| r.text)
                .or(pending_text.take())
                .unwrap_or_default();

            // Combine committed segments with the tail
            let final_text = build_full_text(&committed_text, &tail);

            if !final_text.is_empty() {
                if let Err(e) = injector.type_text(&final_text) {
                    eprintln!("Warning: text injection failed: {}", e);
                }
            }

            // Reset state for next utterance
            pending_text = None;
            committed_text.clear();
            stt.reset();
        }

        // Detect transition: not recording -> recording (key pressed)
        if !was_recording && is_recording {
            let _ = overlay_tx.send(OverlayMsg::Show);

            // Fresh start -- reset recognizer and drain any stale audio from the channel
            stt.reset();
            while capture.receiver.try_recv().is_ok() {}
            pending_text = None;
            committed_text.clear();
        }

        was_recording = is_recording;

        if is_recording {
            // Drain all available audio chunks without blocking
            while let Ok(samples) = capture.receiver.try_recv() {
                stt.accept_waveform(sample_rate, &samples);
            }

            // Always call decode -- the recognizer may have buffered frames
            // from previous audio that still need processing.
            if let Some(result) = stt.decode() {
                if result.is_final {
                    // Endpoint fired mid-recording (e.g. rule3 after 20s).
                    // Commit this segment so it's not lost when the stream resets.
                    if !committed_text.is_empty() {
                        committed_text.push(' ');
                    }
                    committed_text.push_str(&result.text);
                    pending_text = None;
                    eprint!("\r\x1b[2K");
                    io::stderr().flush().ok();
                } else {
                    pending_text = Some(result.text.clone());
                    eprint!("\r\x1b[2K{}", result.text);
                    io::stderr().flush().ok();
                }

                // Send full accumulated transcript to overlay for live display
                let display_text =
                    build_full_text(&committed_text, pending_text.as_deref().unwrap_or(""));
                let _ = overlay_tx.send(OverlayMsg::UpdateText(display_text));
            }

            // Pace the loop: ~30ms gives ~33 decode cycles per second
            std::thread::sleep(Duration::from_millis(30));
        } else {
            // Not recording -- sleep briefly to avoid busy loop
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    capture.stop_recording();
    Ok(())
}

/// Combine committed (finalized) segments with a trailing partial/tail string.
fn build_full_text(committed: &str, tail: &str) -> String {
    let tail = tail.trim();
    if committed.is_empty() {
        tail.to_string()
    } else if tail.is_empty() {
        committed.to_string()
    } else {
        format!("{} {}", committed, tail)
    }
}
