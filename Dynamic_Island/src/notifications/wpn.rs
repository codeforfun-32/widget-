//! Reads the notifications Windows shows in its notification center.
//!
//! The official Windows API for this only works for Store-packaged apps, so
//! this reads the database Windows itself keeps for them:
//!   %LOCALAPPDATA%\Microsoft\Windows\Notifications\wpndatabase.db
//! A copy is opened (never the original), and only when the file changed.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use slint::Weak;

use super::{Entry, CLEARED_AT, SYSTEM};
use crate::MainWindow;

const POLL: Duration = Duration::from_secs(3);
const MIN_GAP: Duration = Duration::from_secs(5);

fn db_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("Microsoft").join("Windows").join("Notifications"))
}

/// Last time the database (or its journal) was written.
fn stamp(dir: &Path) -> Option<SystemTime> {
    let db = fs::metadata(dir.join("wpndatabase.db")).and_then(|m| m.modified()).ok()?;
    let wal = fs::metadata(dir.join("wpndatabase.db-wal")).and_then(|m| m.modified()).ok();
    Some(match wal {
        Some(w) if w > db => w,
        _ => db,
    })
}

fn decode_payload(bytes: &[u8]) -> String {
    let utf16 = bytes.len() >= 2 && ((bytes[0] == 0xFF && bytes[1] == 0xFE) || bytes[1] == 0);
    let text = if utf16 {
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).to_string()
    };
    text.trim_start_matches('\u{feff}').to_string()
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(end) = tail.find(';').filter(|e| *e <= 10) {
            let ent = &tail[1..end];
            let rep: Option<String> = match ent {
                "lt" => Some("<".to_string()),
                "gt" => Some(">".to_string()),
                "amp" => Some("&".to_string()),
                "quot" => Some("\"".to_string()),
                "apos" => Some("'".to_string()),
                _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
                    .map(|c| c.to_string()),
                _ if ent.starts_with('#') => ent[1..]
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .map(|c| c.to_string()),
                _ => None,
            };
            if let Some(r) = rep {
                out.push_str(&r);
                rest = &tail[end + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

/// Pulls the text lines out of a toast's XML: <text>Title</text><text>Body</text>
fn extract_texts(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<text") {
        let after = &rest[i + 5..];
        let next = after.chars().next().unwrap_or('>');
        if !(next == ' ' || next == '>' || next == '\n' || next == '\r' || next == '\t' || next == '/') {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        if after[..gt].ends_with('/') {
            rest = &after[gt + 1..];
            continue;
        }
        let body = &after[gt + 1..];
        let Some(end) = body.find("</text>") else { break };
        let text = unescape(&body[..end]).replace(['\r', '\n'], " ");
        let text = text.trim().to_string();
        if !text.is_empty() {
            out.push(text);
        }
        rest = &body[end + 7..];
    }
    out
}

/// Turns the app id Windows stores ("5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App") into a name.
fn app_name(handler: &str) -> String {
    let lower = handler.to_lowercase();
    let known = [
        ("screensketch", "Snipping Tool"),
        ("whatsapp", "WhatsApp"),
        ("windowsstore", "Microsoft Store"),
        ("claude", "Claude"),
        ("chrome", "Chrome"),
        ("msedge", "Edge"),
        ("microsoftedge", "Edge"),
        ("skydrive", "OneDrive"),
        ("onedrive", "OneDrive"),
        ("securityandmaintenance", "Security and Maintenance"),
        ("windowsdefender", "Windows Security"),
        ("securityhealth", "Windows Security"),
        ("immersivecontrolpanel", "Settings"),
        ("spotify", "Spotify"),
        ("discord", "Discord"),
        ("telegram", "Telegram"),
        ("slack", "Slack"),
        ("teams", "Teams"),
        ("outlook", "Outlook"),
        ("windowscommunicationsapps", "Mail"),
        ("photos", "Photos"),
        ("firefox", "Firefox"),
        ("brave", "Brave"),
        ("instagram", "Instagram"),
        ("github", "GitHub"),
    ];
    for (needle, name) in known {
        if lower.contains(needle) {
            return name.to_string();
        }
    }
    // generic: "Publisher.AppName_hash!App" -> "AppName", with spaces between words
    let id = handler.split('!').next().unwrap_or(handler);
    let id = id.split('_').next().unwrap_or(id);
    let last = id.rsplit(['.', '\\', '/']).find(|s| !s.is_empty()).unwrap_or(id);
    let last = last.trim_end_matches(".exe");
    if last.is_empty() {
        return "Windows".to_string();
    }
    let mut name = String::new();
    let mut prev_lower = false;
    for c in last.chars() {
        if c.is_uppercase() && prev_lower {
            name.push(' ');
        }
        prev_lower = c.is_lowercase();
        name.push(c);
    }
    name
}

/// Reads the newest notifications from a copy of the database.
fn read_all(dir: &Path) -> Option<Vec<Entry>> {
    let tmp = std::env::temp_dir().join("dynamic-island-wpn");
    fs::create_dir_all(&tmp).ok()?;
    fs::copy(dir.join("wpndatabase.db"), tmp.join("wpn.db")).ok()?;
    let wal_src = dir.join("wpndatabase.db-wal");
    let wal_dst = tmp.join("wpn.db-wal");
    if wal_src.exists() {
        fs::copy(&wal_src, &wal_dst).ok()?;
    } else {
        let _ = fs::remove_file(&wal_dst);
    }
    let _ = fs::remove_file(tmp.join("wpn.db-shm"));

    let conn = Connection::open_with_flags(tmp.join("wpn.db"), OpenFlags::SQLITE_OPEN_READ_WRITE).ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT n.Id, n.Payload, n.ArrivalTime, h.PrimaryId \
             FROM Notification n LEFT JOIN NotificationHandler h ON n.HandlerId = h.RecordId \
             WHERE n.Type = 'toast' ORDER BY n.ArrivalTime DESC LIMIT 80",
        )
        .ok()?;

    let rows = stmt
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let payload: Vec<u8> = match row.get_ref(1)? {
                ValueRef::Blob(b) => b.to_vec(),
                ValueRef::Text(t) => t.to_vec(),
                _ => Vec::new(),
            };
            let arrival: i64 = row.get::<_, Option<i64>>(2).ok().flatten().unwrap_or(0);
            let handler: String = row.get::<_, Option<String>>(3).ok().flatten().unwrap_or_default();
            Ok((id, payload, arrival, handler))
        })
        .ok()?;

    let mut out = Vec::new();
    for row in rows.flatten() {
        let (id, payload, arrival, handler) = row;
        // Windows stores the arrival time as FILETIME (100 ns since 1601)
        let ts = arrival / 10_000_000 - 11_644_473_600;
        if ts <= 0 {
            continue;
        }
        let texts = extract_texts(&decode_payload(&payload));
        if texts.is_empty() {
            continue;
        }
        out.push(Entry {
            id: id.to_string(),
            ts,
            app: app_name(&handler),
            title: texts[0].clone(),
            body: texts[1..].join(" "),
        });
    }
    Some(out)
}

pub fn spawn(ui: Weak<MainWindow>) {
    thread::spawn(move || {
        let Some(dir) = db_dir() else { return };
        let mut last: Option<SystemTime> = None;
        let mut seen: HashSet<String> = HashSet::new();
        let mut first = true;
        let mut last_read: Option<Instant> = None;

        loop {
            let now_stamp = stamp(&dir);
            let gap_ok = last_read.map_or(true, |t| t.elapsed() >= MIN_GAP);
            if now_stamp.is_some() && now_stamp != last && gap_ok {
                last_read = Some(Instant::now());
                match read_all(&dir) {
                    Some(entries) => {
                        last = now_stamp;
                        let cleared = CLEARED_AT.load(Ordering::Relaxed);
                        let entries: Vec<Entry> = entries.into_iter().filter(|e| e.ts > cleared).collect();

                        // entries are newest first, so the first unseen one is the newest
                        let fresh: Vec<&Entry> = entries.iter().filter(|e| !seen.contains(&e.id)).collect();
                        let newest = if first { None } else { fresh.first().map(|e| (*e).clone()) };
                        let count = if first { 0 } else { fresh.len() as i32 };
                        for e in &entries {
                            seen.insert(e.id.clone());
                        }
                        first = false;

                        if let Ok(mut sys) = SYSTEM.lock() {
                            *sys = entries;
                        }
                        let merged = super::merged();

                        let sent = ui.upgrade_in_event_loop(move |ui| {
                            ui.set_notifs(super::to_model(merged));
                            if let Some(e) = newest {
                                ui.invoke_show_toast(
                                    e.app.clone().into(),
                                    e.title.clone().into(),
                                    e.body.clone().into(),
                                );
                            }
                            if count > 0 {
                                ui.set_notif_unread(ui.get_notif_unread() + count);
                            }
                        });
                        if sent.is_err() {
                            break;
                        }
                    }
                    // could not read it this time (file busy): try again on the next round
                    None => last = None,
                }
            }
            thread::sleep(POLL);
        }
    });
}