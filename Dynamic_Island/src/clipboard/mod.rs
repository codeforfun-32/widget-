use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use slint::{ModelRc, SharedString, VecModel, Weak};
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;

use crate::app_state::CLIPBOARD_MAX;
use crate::MainWindow;

static HISTORY: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn history() -> &'static Mutex<Vec<String>> {
    HISTORY.get_or_init(|| Mutex::new(Vec::new()))
}

/// Short preview of a clip for the list.
fn label(text: &str) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    one_line.chars().take(120).collect()
}

/// Put the clip at `index` back on the clipboard.
pub fn copy(index: usize) -> bool {
    let text = match history().lock() {
        Ok(h) => h.get(index).cloned(),
        Err(_) => None,
    };
    let Some(text) = text else { return false };
    Clipboard::new().and_then(|mut cb| cb.set_text(text)).is_ok()
}

/// Sends the current history to the UI. Returns false when the UI is gone.
fn publish(ui: &Weak<MainWindow>) -> bool {
    let items: Vec<String> = match history().lock() {
        Ok(h) => h.iter().map(|s| label(s)).collect(),
        Err(_) => return true,
    };
    ui.upgrade_in_event_loop(move |ui| {
        let model: Vec<SharedString> = items.into_iter().map(SharedString::from).collect();
        ui.set_clips(ModelRc::new(VecModel::from(model)));
    })
    .is_ok()
}

/// Removes one clip from the list.
pub fn remove(ui: &Weak<MainWindow>, index: usize) {
    if let Ok(mut h) = history().lock() {
        if index < h.len() {
            h.remove(index);
        }
    }
    publish(ui);
}

/// Removes all clips from the list.
pub fn clear(ui: &Weak<MainWindow>) {
    if let Ok(mut h) = history().lock() {
        h.clear();
    }
    publish(ui);
}

/// Watches the clipboard and keeps the last few text entries (memory only, nothing is saved to disk).
/// The clipboard is only opened when Windows says it changed, so other apps are never blocked.
pub fn spawn_watcher(ui: Weak<MainWindow>) {
    thread::spawn(move || {
        let mut last_seq: u32 = 0;
        let mut first = true;
        loop {
            let seq = unsafe { GetClipboardSequenceNumber() };
            if seq != last_seq {
                if let Ok(mut cb) = Clipboard::new() {
                    last_seq = seq;
                    if let Ok(text) = cb.get_text() {
                        let t = text.trim().to_string();
                        if !t.is_empty() {
                            let mut added = false;
                            if let Ok(mut h) = history().lock() {
                                let is_new = h.first().map(|f| f != &t).unwrap_or(true);
                                if is_new {
                                    h.retain(|x| x != &t);
                                    h.insert(0, t.clone());
                                    h.truncate(CLIPBOARD_MAX);
                                    added = true;
                                }
                            }
                            if added && !publish(&ui) {
                                return;
                            }
                            // popup for every new copy (but not for what was already there at start-up)
                            if !first {
                                let preview: String = label(&t).chars().take(60).collect();
                                let _ = ui.upgrade_in_event_loop(move |ui| {
                                    ui.invoke_show_toast("Clipboard".into(), "Copied".into(), preview.into());
                                });
                            }
                        }
                    }
                    first = false;
                }
            }
            thread::sleep(Duration::from_millis(700));
        }
    });
}