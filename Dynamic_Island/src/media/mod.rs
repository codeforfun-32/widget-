use std::collections::HashMap;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use slint::{ModelRc, SharedString, VecModel, Weak, Image};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Storage::Streams::DataReader;

use crate::{MainWindow, SourceItem};

static SELECTED: Mutex<Option<String>> = Mutex::new(None);
static ORDER: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[derive(Clone, Default, PartialEq)]
struct MediaInfo {
    title: String,
    artist: String,
    source: String,
    playing: bool,
    progress: f32,
    has_media: bool,
    // We store the raw bytes here so it can be safely sent between threads!
    art_image: Option<(u32, u32, Vec<u8>)>,
    ambient_color: [u8; 3],
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

fn app_id(session: &Session) -> String {
    session.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default()
}

fn friendly(id: &str) -> String {
    let lower = id.to_lowercase();
    let known = [
        ("spotify", "Spotify"), ("chrome", "Chrome"), ("msedge", "Edge"), ("edge", "Edge"),
        ("firefox", "Firefox"), ("brave", "Brave"), ("opera", "Opera"), ("vlc", "VLC"),
        ("zunemusic", "Media Player"), ("applemusic", "Apple Music"), ("itunes", "iTunes"),
        ("telegram", "Telegram"), ("whatsapp", "WhatsApp"),
    ];
    for (needle, name) in known {
        if lower.contains(needle) { return name.to_string(); }
    }
    let base = id.rsplit(['!', '\\', '/']).next().unwrap_or(id);
    let base = base.trim_end_matches(".exe");
    let mut chars = base.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Player".to_string(),
    }
}

fn get_manager() -> Option<Manager> {
    Manager::RequestAsync().ok().and_then(|op| op.join().ok())
}

fn list_sessions(manager: &Manager) -> Vec<(String, Session)> {
    let Ok(sessions) = manager.GetSessions() else { return Vec::new() };
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for session in sessions {
        let id = app_id(&session);
        let n = counts.entry(id.clone()).or_insert(0);
        *n += 1;
        out.push((format!("{id}#{n}"), session));
    }
    out
}

fn update_order(list: &[(String, Session)]) {
    if let Ok(mut order) = ORDER.lock() {
        order.retain(|k| list.iter().any(|(key, _)| key == k));
        for (key, _) in list {
            if !order.contains(key) { order.push(key.clone()); }
        }
    }
}

fn pick_active(list: &[(String, Session)]) -> Option<usize> {
    let selected = SELECTED.lock().ok().and_then(|g| g.clone());
    if let Some(sel) = selected {
        if let Some(i) = list.iter().position(|(k, _)| *k == sel) { return Some(i); }
        if let Ok(mut s) = SELECTED.lock() { *s = None; }
    }
    if let Ok(order) = ORDER.lock() {
        for key in order.iter() {
            if let Some(i) = list.iter().position(|(k, _)| k == key) { return Some(i); }
        }
    }
    if list.is_empty() { None } else { Some(0) }
}

fn extract_art_and_color(session: &Session) -> (Option<(u32, u32, Vec<u8>)>, [u8; 3]) {
    let Ok(props) = session.TryGetMediaPropertiesAsync().and_then(|op| op.join()) else {
        return (None, [24, 24, 27]);
    };
    let Ok(thumb_ref) = props.Thumbnail() else {
        return (None, [24, 24, 27]);
    };
    let Ok(stream) = thumb_ref.OpenReadAsync().and_then(|op| op.join()) else {
        return (None, [24, 24, 27]);
    };
    
    let size = stream.Size().unwrap_or(0) as u32;
    if size == 0 { return (None, [24, 24, 27]); }

    let reader = DataReader::CreateDataReader(&stream).ok();
    let mut bytes = Vec::new();
    if let Some(r) = reader {
        if r.LoadAsync(size).and_then(|op| op.join()).is_ok() {
            let mut buf = vec![0u8; size as usize];
            if r.ReadBytes(&mut buf).is_ok() {
                bytes = buf;
            }
        }
    }

    if bytes.is_empty() {
        return (None, [24, 24, 27]);
    }

    if let Ok(img) = image::load_from_memory(&bytes) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        
        let mut r_sum = 0u64; let mut g_sum = 0u64; let mut b_sum = 0u64; let mut count = 0u64;
        for y in (0..h).step_by((h / 20).max(1) as usize) {
            for x in (0..w).step_by((w / 20).max(1) as usize) {
                let p = rgba.get_pixel(x, y);
                if p[3] > 128 {
                    r_sum += p[0] as u64; g_sum += p[1] as u64; b_sum += p[2] as u64; count += 1;
                }
            }
        }
        let color = if count > 0 {
            [(r_sum / count) as u8, (g_sum / count) as u8, (b_sum / count) as u8]
        } else { [24, 24, 27] };

        // Return raw bytes (safe for threading)
        return (Some((w, h, rgba.into_raw())), color);
    }

    (None, [24, 24, 27])
}

fn read_info(session: &Session, source: String) -> Option<MediaInfo> {
    let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let title = props.Title().ok()?.to_string();
    let artist = props.Artist().ok()?.to_string();
    let playing = session.GetPlaybackInfo().ok()?.PlaybackStatus().ok()? == Status::Playing;

    let timeline = session.GetTimelineProperties().ok()?;
    let start = timeline.StartTime().ok()?.Duration;
    let end = timeline.EndTime().ok()?.Duration;
    let mut pos = timeline.Position().ok()?.Duration;

    if playing {
        if let Ok(updated) = timeline.LastUpdatedTime() {
            let updated = updated.UniversalTime;
            if updated > 0 {
                let elapsed = (now_ticks() - updated).max(0);
                if elapsed < 432_000_000_000 { pos += elapsed; }
            }
        }
    }
    pos = pos.min(end);

    let total = (end - start).max(0) as f32;
    let progress = if total > 0.0 { (((pos - start).max(0)) as f32 / total).clamp(0.0, 1.0) } else { 0.0 };

    let (art_image, ambient_color) = if !title.is_empty() {
        extract_art_and_color(session)
    } else {
        (None, [24, 24, 27])
    };

    Some(MediaInfo { has_media: !title.is_empty(), title, artist, source, playing, progress, art_image, ambient_color })
}

pub fn spawn_poller(ui: Weak<MainWindow>) {
    thread::spawn(move || {
        init_com();
        let mut last = MediaInfo::default();
        let mut last_items: Vec<(String, String, bool)> = Vec::new();
        let mut round: u32 = 0;
        let mut manager: Option<Manager> = None;

        loop {
            if manager.is_none() { manager = get_manager(); }
            let list = match &manager { Some(m) => list_sessions(m), None => Vec::new() };
            update_order(&list);
            let active = pick_active(&list);

            let info = match active {
                Some(i) => {
                    let source = friendly(&app_id(&list[i].1));
                    read_info(&list[i].1, source).unwrap_or_default()
                }
                None => MediaInfo::default(),
            };

            if info != last {
                if info.has_media && info.playing && info.title != last.title {
                    crate::ticker::push(&ui, format!("Now playing: {}", info.title));
                }
                let send = info.clone();
                let result = ui.upgrade_in_event_loop(move |ui| {
                    ui.set_has_media(send.has_media);
                    ui.set_playing(send.playing);
                    ui.set_progress(send.progress);
                    ui.set_track_title(send.title.into());
                    ui.set_track_artist(send.artist.into());
                    ui.set_source_name(send.source.into());
                    
                    // Build the Image safely on the UI thread
                    if let Some((w, h, bytes)) = send.art_image {
                        let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
                        buffer.make_mut_bytes().copy_from_slice(&bytes);
                        ui.set_media_art(Image::from_rgba8(buffer));
                    }
                    
                    let c = send.ambient_color;
                    ui.set_media_ambient_color(slint::Color::from_rgb_u8(c[0], c[1], c[2]));
                });
                if result.is_err() { break; }
                last = info;
            }

            if round % 6 == 0 {
                let selected = SELECTED.lock().ok().and_then(|g| g.clone());
                let mut items: Vec<(String, String, bool)> =
                    vec![(String::new(), "Auto (first opened)".to_string(), selected.is_none())];
                for (key, session) in list.iter().take(4) {
                    let app = friendly(&app_id(session));
                    let title = session.TryGetMediaPropertiesAsync().ok().and_then(|op| op.join().ok())
                        .and_then(|p| p.Title().ok()).map(|t| t.to_string()).unwrap_or_default();
                    let label = if title.is_empty() { app } else { format!("{app} - {title}") };
                    let is_selected = selected.as_deref() == Some(key.as_str());
                    items.push((key.clone(), label, is_selected));
                }
                if items != last_items {
                    let send = items.clone();
                    let result = ui.upgrade_in_event_loop(move |ui| {
                        let model: Vec<SourceItem> = send.into_iter()
                            .map(|(key, label, active)| SourceItem {
                                key: SharedString::from(key), label: SharedString::from(label), active,
                            }).collect();
                        ui.set_sources(ModelRc::new(VecModel::from(model)));
                    });
                    if result.is_err() { break; }
                    last_items = items;
                }
            }
            round = round.wrapping_add(1);
            thread::sleep(Duration::from_millis(500));
        }
    });
}

pub fn select(key: &str) {
    if let Ok(mut s) = SELECTED.lock() {
        *s = if key.is_empty() { None } else { Some(key.to_string()) };
    }
}

fn with_active(f: impl FnOnce(&Session) + Send + 'static) {
    thread::spawn(move || {
        init_com();
        let Some(manager) = get_manager() else { return };
        let list = list_sessions(&manager);
        if let Some(i) = pick_active(&list) { f(&list[i].1); }
    });
}

pub fn toggle() { with_active(|s| { let _ = s.TryTogglePlayPauseAsync().and_then(|op| op.join()); }); }
pub fn next() { with_active(|s| { let _ = s.TrySkipNextAsync().and_then(|op| op.join()); }); }
pub fn previous() { with_active(|s| { let _ = s.TrySkipPreviousAsync().and_then(|op| op.join()); }); }

pub fn seek(fraction: f32) {
    let f = fraction.clamp(0.0, 1.0) as f64;
    with_active(move |s| {
        let Ok(timeline) = s.GetTimelineProperties() else { return };
        let (Ok(start), Ok(end)) = (timeline.StartTime(), timeline.EndTime()) else { return };
        let target = start.Duration + ((end.Duration - start.Duration) as f64 * f) as i64;
        let _ = s.TryChangePlaybackPositionAsync(target).and_then(|op| op.join());
    });
}