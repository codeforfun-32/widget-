use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONASTERISK, MB_ICONEXCLAMATION};

pub const BARS: usize = 9;

/// Sound effects. For now these are Windows system sounds.
pub fn play_sfx(name: &str) {
    let style = match name {
        "dizzy" => MB_ICONEXCLAMATION,
        "done" => MB_ICONASTERISK,
        _ => return, // "happy" and anything else stays silent
    };
    unsafe {
        let _ = MessageBeep(style);
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Source {
    /// Whatever is playing through your speakers (used for the music visualizer).
    Loopback,
}

/// Level meter with 9 bars. The audio device is only opened while the meter is switched on.
#[derive(Clone)]
pub struct Meter {
    enabled: Arc<AtomicBool>,
    levels: Arc<Mutex<[f32; BARS]>>,
}

impl Meter {
    pub fn spawn(_source: Source, gain: f32) -> Meter {
        let meter = Meter {
            enabled: Arc::new(AtomicBool::new(false)),
            levels: Arc::new(Mutex::new([0.0; BARS])),
        };
        let worker = meter.clone();
        thread::spawn(move || {
            let mut stream: Option<cpal::Stream> = None;
            loop {
                let want = worker.enabled.load(Ordering::Relaxed);
                if want && stream.is_none() {
                    stream = build_stream(gain, worker.levels.clone());
                    if stream.is_none() {
                        // no device or access denied: switch off and wait a bit before any retry
                        worker.enabled.store(false, Ordering::Relaxed);
                        thread::sleep(Duration::from_secs(2));
                    }
                } else if !want && stream.is_some() {
                    stream = None;
                    if let Ok(mut l) = worker.levels.lock() {
                        *l = [0.0; BARS];
                    }
                }
                thread::sleep(Duration::from_millis(100));
            }
        });
        meter
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn levels(&self) -> [f32; BARS] {
        match self.levels.lock() {
            Ok(l) => *l,
            Err(_) => [0.0; BARS],
        }
    }
}

fn build_stream(gain: f32, levels: Arc<Mutex<[f32; BARS]>>) -> Option<cpal::Stream> {
    let host = cpal::default_host();
    // on Windows, listening to an output device captures what it is playing
    let device = host.default_output_device()?;
    let supported = device.default_output_config().ok()?;
    let channels = supported.channels() as usize;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let err_fn = |e: cpal::StreamError| eprintln!("audio error: {e}");

    let stream = match format {
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| process(data, channels, gain, &levels),
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let f: Vec<f32> = data.iter().map(|s| *s as f32 / 32768.0).collect();
                process(&f, channels, gain, &levels)
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                let f: Vec<f32> = data.iter().map(|s| (*s as f32 - 32768.0) / 32768.0).collect();
                process(&f, channels, gain, &levels)
            },
            err_fn,
            None,
        ),
        _ => return None,
    }
    .ok()?;

    stream.play().ok()?;
    Some(stream)
}

/// Splits one audio buffer into 9 slices and uses the loudness of each slice as a bar height.
fn process(data: &[f32], channels: usize, gain: f32, levels: &Mutex<[f32; BARS]>) {
    if data.is_empty() || channels == 0 {
        return;
    }
    let mono: Vec<f32> = data
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    let seg = (mono.len() / BARS).max(1);

    if let Ok(mut l) = levels.lock() {
        for i in 0..BARS {
            let start = i * seg;
            if start >= mono.len() {
                break;
            }
            let end = ((i + 1) * seg).min(mono.len());
            let slice = &mono[start..end];
            let rms = (slice.iter().map(|s| s * s).sum::<f32>() / slice.len() as f32).sqrt();
            let target = (rms * gain).min(1.0);
            l[i] = if target > l[i] { target } else { l[i] * 0.85 };
        }
    }
}