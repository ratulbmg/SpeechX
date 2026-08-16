//! `cpal` stream lifecycle. Runs on its own dedicated thread because
//! `cpal::Stream` is not `Send` on macOS — it must never leave the thread
//! that created it (mirrors the hotkey listener's approach in
//! `hotkey::listener`).

use std::sync::mpsc as std_mpsc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use ringbuf::traits::{Consumer, Observer, Producer};
use tracing::error;

use super::ring_buffer::{self, SampleConsumer, SampleProducer};

#[derive(Debug, Clone, Copy)]
pub struct StreamInfo {
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct CaptureSession {
    stop_tx: std_mpsc::Sender<()>,
    join: std::thread::JoinHandle<()>,
    consumer: SampleConsumer,
    pub info: StreamInfo,
}

impl CaptureSession {
    /// Starts capturing from the default input device immediately.
    /// Buffering begins before the caller knows whether this will become
    /// a real recording (PROMPT.md hazard #2: waiting for the 150 ms
    /// chord window to resolve first would clip the first syllable).
    pub fn start() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "no default input device".to_string())?;
        let supported = device
            .default_input_config()
            .map_err(|e| format!("no input config available: {e}"))?;

        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let info = StreamInfo {
            sample_rate: config.sample_rate.0,
            channels: config.channels,
        };

        // Headroom past the 120 s hard cap (PROMPT.md §5 rule 4), sized
        // for this device's actual rate and channel count.
        let capacity = info.sample_rate as usize * info.channels.max(1) as usize * 130;
        let (producer, consumer) = ring_buffer::new(capacity);

        let (stop_tx, stop_rx) = std_mpsc::channel::<()>();
        let (ready_tx, ready_rx) = std_mpsc::channel::<Result<(), String>>();

        let join = std::thread::Builder::new()
            .name("speechx-audio-capture".into())
            .spawn(move || run_capture_thread(device, config, sample_format, producer, stop_rx, ready_tx))
            .map_err(|e| format!("failed to spawn audio thread: {e}"))?;

        ready_rx
            .recv()
            .map_err(|_| "audio thread exited before starting".to_string())??;

        Ok(Self {
            stop_tx,
            join,
            consumer,
            info,
        })
    }

    /// Stops the stream and returns every raw sample captured, at the
    /// device's native rate/channel count (not yet resampled).
    pub fn stop(self) -> (StreamInfo, Vec<f32>) {
        let _ = self.stop_tx.send(());
        let _ = self.join.join();

        let mut consumer = self.consumer;
        let available = consumer.occupied_len();
        let mut samples = vec![0.0_f32; available];
        let popped = consumer.pop_slice(&mut samples);
        samples.truncate(popped);
        (self.info, samples)
    }
}

fn run_capture_thread(
    device: cpal::Device,
    config: cpal::StreamConfig,
    sample_format: SampleFormat,
    mut producer: SampleProducer,
    stop_rx: std_mpsc::Receiver<()>,
    ready_tx: std_mpsc::Sender<Result<(), String>>,
) {
    let err_fn = |err| error!(?err, "cpal stream error");

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let _ = producer.push_slice(data);
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let converted: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                let _ = producer.push_slice(&converted);
            },
            err_fn,
            None,
        ),
        other => {
            let _ = ready_tx.send(Err(format!("unsupported sample format: {other:?}")));
            return;
        }
    };

    let stream = match stream {
        Ok(stream) => stream,
        Err(cpal::BuildStreamError::DeviceNotAvailable) => {
            // On Windows this is almost always the per-app microphone
            // privacy toggle (Settings > Privacy & Security > Microphone),
            // not a hardware problem — say so (WINDOWS_SUPPORT.md §13).
            let hint = if cfg!(target_os = "windows") {
                "microphone unavailable — check Windows microphone privacy settings"
            } else {
                "microphone unavailable"
            };
            let _ = ready_tx.send(Err(hint.to_string()));
            return;
        }
        Err(err) => {
            let _ = ready_tx.send(Err(format!("failed to build input stream: {err}")));
            return;
        }
    };

    if let Err(err) = stream.play() {
        let _ = ready_tx.send(Err(format!("failed to start stream: {err}")));
        return;
    }

    let _ = ready_tx.send(Ok(()));

    // Block until told to stop; the stream stays alive (and capturing)
    // for as long as this thread is parked here.
    let _ = stop_rx.recv();
    drop(stream);
}
