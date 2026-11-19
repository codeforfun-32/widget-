//! Notification list: real Windows notifications (read by `wpn`) plus the
//! island's own short messages ("Copied to clipboard", "Timer done!", ...).

mod wpn;

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use slint::{ModelRc, VecModel, Weak};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};

use crate::{MainWindow, NotifItem};

const MAX_ITEMS: usize = 60;
const MAX_INTERNAL: usize = 12;

#[derive(Clone)]
pub struct Entry {
    /// Windows notification id (empty for the island's own messages).
    pub id: String,
    /// Unix time in seconds.
    pub ts: i64,
    pub app: String,
    pub title: String,
    pub body: String,
}

/// The island's own messages, newest first.
static INTERNAL: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
/// Notifications read from Windows, newest first.
static SYSTEM: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
/// Anything older than this was cleared by the user.
static CLEARED_AT: AtomicI64 = AtomicI64::new(0);

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Seconds between UTC and local time right now.
fn tz_offset_secs() -> i64 {
    let local = unsafe { GetLocalTime() };
    let utc = unsafe { GetSystemTime() };
    let l = local.wHour as i64 * 3600 + local.wMinute as i64 * 60 + local.wSecond as i64;
    let u = utc.wHour as i64 * 3600 + utc.wMinute as i64 * 60 + utc.wSecond as i64;
    let mut d = l - u;
    if d > 12 * 3600 {
        d -= 86400;
    } else if d < -12 * 3600 {
        d += 86400;
    }
    d
}

/// "22:37", "Yesterday 22:37" or "3d ago".
fn fmt_time(ts: i64) -> String {
    let off = tz_offset_secs();
    let local = ts + off;
    let now_local = now_unix() + off;
    let days = now_local.div_euclid(86400) - local.div_euclid(86400);
    let secs = local.rem_euclid(86400);
    let hm = format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60);
    match days {
        d if d <= 0 => hm,
        1 => format!("Yesterday {hm}"),
        d => format!("{d}d ago"),
    }
}

/// Adds one of the island's own messages to the list.
pub fn record(text: &str) {
    if let Ok(mut log) = INTERNAL.lock() {
        log.insert(
            0,
            Entry {
                id: String::new(),
                ts: now_unix(),
                app: "Dynamic Island".to_string(),
                title: text.to_string(),
                body: String::new(),
            },
        );
        log.truncate(MAX_INTERNAL);
    }
}

/// Windows notifications and the island's messages together, newest first.
pub fn merged() -> Vec<Entry> {
    let mut all: Vec<Entry> = Vec::new();
    if let Ok(sys) = SYSTEM.lock() {
        all.extend(sys.iter().cloned());
    }
    if let Ok(own) = INTERNAL.lock() {
        all.extend(own.iter().cloned());
    }
    all.sort_by(|a, b| b.ts.cmp(&a.ts));
    all.truncate(MAX_ITEMS);
    all
}

/// Must be called on the UI thread.
pub fn to_model(items: Vec<Entry>) -> ModelRc<NotifItem> {
    let list: Vec<NotifItem> = items
        .into_iter()
        .map(|e| NotifItem {
            time: fmt_time(e.ts).into(),
            app: e.app.into(),
            title: e.title.into(),
            body: e.body.into(),
        })
        .collect();
    ModelRc::new(VecModel::from(list))
}

pub fn clear(ui: &Weak<MainWindow>) {
    CLEARED_AT.store(now_unix(), Ordering::Relaxed);
    if let Ok(mut log) = INTERNAL.lock() {
        log.clear();
    }
    if let Ok(mut sys) = SYSTEM.lock() {
        sys.clear();
    }
    let _ = ui.upgrade_in_event_loop(|ui| {
        ui.set_notifs(to_model(Vec::new()));
        ui.set_notif_unread(0);
    });
}

/// Starts watching the Windows notifications.
pub fn spawn(ui: Weak<MainWindow>) {
    wpn::spawn(ui);
}