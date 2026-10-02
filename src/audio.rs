//! CPAL desktop audio host. The game emits IDs; this host synthesizes the
//! resource-defined tones on a bounded voice pool outside simulation.
use crate::{
    game::AudioEvent,
    resources::{ResourcePack, SoundAsset},
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
pub struct DesktopAudio {
    stream: cpal::Stream,
    tx: SyncSender<SoundAsset>,
    started: Arc<AtomicU64>,
    peak: Arc<AtomicU32>,
    failed: Arc<AtomicBool>,
    pub scheduled: u64,
    pub dropped: u64,
}
impl DesktopAudio {
    pub fn new() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let (tx, rx) = mpsc::sync_channel(128);
        let started = Arc::new(AtomicU64::new(0));
        let peak = Arc::new(AtomicU32::new(0));
        let failed = Arc::new(AtomicBool::new(false));
        let stream = match format {
            cpal::SampleFormat::F32 => stream::<f32>(
                &device,
                config,
                rx,
                started.clone(),
                peak.clone(),
                failed.clone(),
            ),
            cpal::SampleFormat::I16 => stream::<i16>(
                &device,
                config,
                rx,
                started.clone(),
                peak.clone(),
                failed.clone(),
            ),
            cpal::SampleFormat::U16 => stream::<u16>(
                &device,
                config,
                rx,
                started.clone(),
                peak.clone(),
                failed.clone(),
            ),
            cpal::SampleFormat::I32 => stream::<i32>(
                &device,
                config,
                rx,
                started.clone(),
                peak.clone(),
                failed.clone(),
            ),
            cpal::SampleFormat::F64 => stream::<f64>(
                &device,
                config,
                rx,
                started.clone(),
                peak.clone(),
                failed.clone(),
            ),
            _ => return Err(format!("unsupported audio sample format {format}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            stream,
            tx,
            started,
            peak,
            failed,
            scheduled: 0,
            dropped: 0,
        })
    }
    pub fn events(&mut self, events: &[AudioEvent], pack: &ResourcePack) {
        for event in events {
            if let Some(sound) = pack.sound(event.resource_id) {
                if self.tx.try_send(*sound).is_ok() {
                    self.scheduled += 1;
                } else {
                    self.dropped += 1;
                }
            }
        }
    }
    pub fn started(&self) -> u64 {
        self.started.load(Ordering::Relaxed)
    }
    pub fn peak(&self) -> f32 {
        f32::from_bits(self.peak.load(Ordering::Relaxed))
    }
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }
    pub fn pause(&self) -> Result<(), String> {
        self.stream.pause().map_err(|e| e.to_string())
    }
    pub fn resume(&self) -> Result<(), String> {
        self.stream.play().map_err(|e| e.to_string())
    }
}
#[derive(Clone, Copy)]
struct Voice {
    sound: SoundAsset,
    phase: f32,
    remaining: u32,
    total: u32,
}
fn stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    rx: Receiver<SoundAsset>,
    started: Arc<AtomicU64>,
    peak: Arc<AtomicU32>,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream, String> {
    let channels = usize::from(config.channels);
    let rate = config.sample_rate as f32;
    let mut voices: [Option<Voice>; 16] = [None; 16];
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                while let Ok(sound) = rx.try_recv() {
                    let slot = voices.iter().position(Option::is_none).unwrap_or(0);
                    let total = (sound.duration_ms as f32 * rate / 1000.0) as u32;
                    voices[slot] = Some(Voice {
                        sound,
                        phase: 0.0,
                        remaining: total,
                        total,
                    });
                    started.fetch_add(1, Ordering::Relaxed);
                }
                let mut maximum = 0.0f32;
                for frame in data.chunks_mut(channels) {
                    let mut sample = 0.0;
                    for slot in &mut voices {
                        if let Some(voice) = slot {
                            let wave = match voice.sound.waveform {
                                0 => {
                                    if voice.phase < 0.5 {
                                        1.0
                                    } else {
                                        -1.0
                                    }
                                }
                                1 => 1.0 - 4.0 * (voice.phase - 0.5).abs(),
                                _ => (voice.phase * std::f32::consts::TAU).sin(),
                            };
                            let envelope = (voice.remaining as f32 / voice.total.max(1) as f32)
                                .min(
                                    voice.total.saturating_sub(voice.remaining) as f32
                                        / (rate * 0.005),
                                )
                                .clamp(0.0, 1.0);
                            sample += wave * envelope * voice.sound.gain_q8 as f32 / 256.0;
                            voice.phase =
                                (voice.phase + voice.sound.frequency as f32 / rate).fract();
                            voice.remaining = voice.remaining.saturating_sub(1);
                            if voice.remaining == 0 {
                                *slot = None;
                            }
                        }
                    }
                    sample = sample.clamp(-0.8, 0.8);
                    maximum = maximum.max(sample.abs());
                    for out in frame {
                        *out = T::from_sample(sample);
                    }
                }
                peak.fetch_max(maximum.to_bits(), Ordering::Relaxed);
            },
            move |error| {
                failed.store(true, Ordering::Relaxed);
                eprintln!("Audio stream: {error}");
            },
            None,
        )
        .map_err(|e| e.to_string())
}
