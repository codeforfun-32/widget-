//! What is playing right now (Windows media controls), plus play / pause / skip.
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

#[derive(Clone, Default)]
pub struct Snapshot {
    pub version: u64,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub progress: f32,
    pub art: Option<(u32, u32, Vec<u8>)>,
}

#[derive(Clone, Default)]
pub struct Music {
    state: Arc<Mutex<Snapshot>>,
    started: Arc<Mutex<bool>>,
}

fn init_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn now_ticks() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_nanos() / 100) as i64 + 116_444_736_000_000_000)
        .unwrap_or(0)
}

fn get_manager() -> Option<Manager> {
    Manager::RequestAsync().ok().and_then(|op| op.join().ok())
}

fn current_session(manager: &Manager) -> Option<Session> {
    if let Ok(s) = manager.GetCurrentSession() {
        return Some(s);
    }
    let sessions = manager.GetSessions().ok()?;
    for s in sessions {
        return Some(s);
    }
    None
}

fn read_art(session: &Session) -> Option<(u32, u32, Vec<u8>)> {
    let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let thumb = props.Thumbnail().ok()?;
    let stream = thumb.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()? as u32;
    if size == 0 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size).ok()?.join().ok()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;
    let img = image::load_from_memory(&buf).ok()?;
    let img = img.thumbnail(400, 400).to_rgba8();
    let (w, h) = img.dimensions();
    Some((w, h, img.into_raw()))
}

fn read_snapshot(session: &Session, last_title: &str, last_art: &Option<(u32, u32, Vec<u8>)>) -> Option<Snapshot> {
    let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let title = props.Title().ok()?.to_string();
    let artist = props.Artist().ok()?.to_string();
    let playing = session.GetPlaybackInfo().ok()?.PlaybackStatus().ok()? == Status::Playing;

    let mut progress = 0.0f32;
    if let Ok(t) = session.GetTimelineProperties() {
        let start = t.StartTime().map(|x| x.Duration).unwrap_or(0);
        let end = t.EndTime().map(|x| x.Duration).unwrap_or(0);
        let mut pos = t.Position().map(|x| x.Duration).unwrap_or(0);
        if playing {
            if let Ok(updated) = t.LastUpdatedTime() {
                let u = updated.UniversalTime;
                if u > 0 {
                    let elapsed = (now_ticks() - u).max(0);
                    if elapsed < 432_000_000_000 {
                        pos += elapsed;
                    }
                }
            }
        }
        pos = pos.min(end);
        let total = (end - start).max(0) as f32;
        if total > 0.0 {
            progress = (((pos - start).max(0)) as f32 / total).clamp(0.0, 1.0);
        }
    }

    // only fetch the picture again when the song changes
    let art = if title == last_title && last_art.is_some() { last_art.clone() } else { read_art(session) };

    Some(Snapshot { version: 0, title, artist, playing, progress, art })
}

impl Music {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts the background reader (once).
    pub fn start(&self) {
        {
            let Ok(mut s) = self.started.lock() else { return };
            if *s {
                return;
            }
            *s = true;
        }
        let state = self.state.clone();
        thread::spawn(move || {
            init_com();
            let mut manager: Option<Manager> = None;
            let mut version = 0u64;
            loop {
                if manager.is_none() {
                    manager = get_manager();
                }
                let (last_title, last_art) = match state.lock() {
                    Ok(s) => (s.title.clone(), s.art.clone()),
                    Err(_) => (String::new(), None),
                };
                let snap = manager
                    .as_ref()
                    .and_then(current_session)
                    .and_then(|s| read_snapshot(&s, &last_title, &last_art))
                    .unwrap_or_default();
                if let Ok(mut s) = state.lock() {
                    let changed = s.title != snap.title
                        || s.artist != snap.artist
                        || s.playing != snap.playing
                        || (s.progress - snap.progress).abs() > 0.004
                        || s.art.is_some() != snap.art.is_some();
                    if changed {
                        version += 1;
                        *s = Snapshot { version, ..snap };
                    }
                }
                thread::sleep(Duration::from_millis(700));
            }
        });
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

fn with_session(f: impl FnOnce(&Session) + Send + 'static) {
    thread::spawn(move || {
        init_com();
        let Some(manager) = get_manager() else { return };
        if let Some(s) = current_session(&manager) {
            f(&s);
        }
    });
}

pub fn command(cmd: &str) {
    match cmd {
        "toggle" => with_session(|s| {
            let _ = s.TryTogglePlayPauseAsync().and_then(|op| op.join());
        }),
        "next" => with_session(|s| {
            let _ = s.TrySkipNextAsync().and_then(|op| op.join());
        }),
        "prev" => with_session(|s| {
            let _ = s.TrySkipPreviousAsync().and_then(|op| op.join());
        }),
        _ => {}
    }
}
