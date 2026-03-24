use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub struct AudioCapture {
    _stream: cpal::Stream,
    pub sample_rate: i32,
    pub receiver: mpsc::Receiver<Vec<f32>>,
    pub recording: Arc<AtomicBool>,
}

impl AudioCapture {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No default input device found"))?;

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0 as i32;
        let channels = config.channels() as usize;

        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let recording = Arc::new(AtomicBool::new(false));
        let recording_clone = recording.clone();

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !recording_clone.load(Ordering::Relaxed) {
                    return;
                }
                let mono: Vec<f32> = data
                    .chunks(channels)
                    .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                    .collect();
                let _ = tx.send(mono);
            },
            |err| eprintln!("Audio error: {}", err),
            None,
        )?;

        stream.play()?;

        Ok(Self {
            _stream: stream,
            sample_rate,
            receiver: rx,
            recording,
        })
    }

    pub fn stop_recording(&self) {
        self.recording.store(false, Ordering::Relaxed);
    }

    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::Relaxed)
    }
}
