use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;

use arboard::Clipboard;
use slint::{Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel, Weak};

use crate::{links, MainWindow, SiteItem};

const MAX_SITES: usize = 10;
const DEFAULTS: [&str; 3] = [
    "https://www.youtube.com",
    "https://www.instagram.com",
    "https://github.com",
];

#[derive(Clone)]
struct Site {
    url: String,
    name: String,
    /// width, height, RGBA bytes
    icon: Option<(u32, u32, Vec<u8>)>,
}

static SITES: Mutex<Vec<Site>> = Mutex::new(Vec::new());

fn file_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    let dir = PathBuf::from(base).join("DynamicIsland");
    let _ = fs::create_dir_all(&dir);
    Some(dir.join("sites.txt"))
}

fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split(['/', '?', '#']).next().unwrap_or("").to_string()
}

/// Turns pasted text into a site entry, or None if it does not look like a link.
fn normalize(input: &str) -> Option<Site> {
    let t = input.trim();
    if t.is_empty() || t.len() > 200 || t.contains(char::is_whitespace) {
        return None;
    }
    let url = if t.starts_with("http://") || t.starts_with("https://") {
        t.to_string()
    } else {
        format!("https://{t}")
    };
    let host = host_of(&url);
    if !host.contains('.') {
        return None;
    }
    let clean = host.trim_start_matches("www.");
    let labels: Vec<&str> = clean.split('.').collect();
    let base = if labels.len() >= 2 { labels[labels.len() - 2] } else { labels[0] };
    let mut chars = base.chars();
    let name = match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => return None,
    };
    Some(Site { url, name, icon: None })
}

fn save() {
    let Some(path) = file_path() else { return };
    let text = SITES
        .lock()
        .map(|s| s.iter().map(|x| x.url.clone()).collect::<Vec<_>>().join("\n"))
        .unwrap_or_default();
    let _ = fs::write(path, text);
}

fn decode_icon(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some((w, h, img.into_raw()))
}

fn push_to_ui(ui: &Weak<MainWindow>) {
    let snapshot: Vec<Site> = SITES.lock().map(|s| s.clone()).unwrap_or_default();
    let _ = ui.upgrade_in_event_loop(move |ui| {
        let items: Vec<SiteItem> = snapshot
            .into_iter()
            .map(|s| {
                let letter = s.name.chars().next().map(|c| c.to_string()).unwrap_or_default();
                let (icon, has_icon) = match s.icon {
                    Some((w, h, bytes)) => {
                        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
                        buffer.make_mut_bytes().copy_from_slice(&bytes);
                        (Image::from_rgba8(buffer), true)
                    }
                    None => (Image::default(), false),
                };
                SiteItem {
                    name: s.name.into(),
                    url: s.url.into(),
                    letter: letter.into(),
                    icon,
                    has_icon,
                }
            })
            .collect();
        ui.set_sites(ModelRc::new(VecModel::from(items)));
    });
}

fn fetch_icon(url: &str) {
    let host = host_of(url);
    let api = format!("https://www.google.com/s2/favicons?domain={host}&sz=64");
    let Some(bytes) = links::http_get_bytes(&api, 400_000) else { return };
    let Some(icon) = decode_icon(&bytes) else { return };
    if let Ok(mut sites) = SITES.lock() {
        if let Some(site) = sites.iter_mut().find(|s| s.url == url) {
            site.icon = Some(icon);
        }
    }
}

/// Loads the saved sites and downloads their logos in the background.
pub fn init(ui: Weak<MainWindow>) {
    let mut list: Vec<Site> = Vec::new();
    let saved = file_path().and_then(|p| fs::read_to_string(p).ok());
    match saved {
        Some(text) => {
            for line in text.lines() {
                if list.len() < MAX_SITES {
                    if let Some(site) = normalize(line) {
                        list.push(site);
                    }
                }
            }
        }
        None => {
            for d in DEFAULTS {
                if let Some(site) = normalize(d) {
                    list.push(site);
                }
            }
        }
    }
    if let Ok(mut g) = SITES.lock() {
        *g = list;
    }
    push_to_ui(&ui);

    thread::spawn(move || {
        let urls: Vec<String> = SITES
            .lock()
            .map(|s| s.iter().filter(|x| x.icon.is_none()).map(|x| x.url.clone()).collect())
            .unwrap_or_default();
        for url in urls {
            fetch_icon(&url);
            push_to_ui(&ui);
        }
    });
}

/// Saves the link that is currently on the clipboard.
pub fn add_from_clipboard(ui: &Weak<MainWindow>) {
    let text = Clipboard::new()
        .ok()
        .and_then(|mut c| c.get_text().ok())
        .unwrap_or_default();
    let Some(site) = normalize(&text) else {
        crate::ticker::push(ui, "Copy a website link first".to_string());
        return;
    };

    let result: Result<(), &str> = match SITES.lock() {
        Ok(mut sites) => {
            if sites.len() >= MAX_SITES {
                Err("Site list is full")
            } else if sites.iter().any(|s| s.url == site.url) {
                Err("Already saved")
            } else {
                sites.push(site.clone());
                Ok(())
            }
        }
        Err(_) => Err("Could not save"),
    };

    match result {
        Ok(()) => {
            save();
            push_to_ui(ui);
            crate::ticker::push(ui, format!("Saved {}", site.name));
            let ui2 = ui.clone();
            let url = site.url.clone();
            thread::spawn(move || {
                fetch_icon(&url);
                push_to_ui(&ui2);
            });
        }
        Err(msg) => crate::ticker::push(ui, msg.to_string()),
    }
}

pub fn remove(ui: &Weak<MainWindow>, index: usize) {
    if let Ok(mut sites) = SITES.lock() {
        if index < sites.len() {
            sites.remove(index);
        }
    }
    save();
    push_to_ui(ui);
}

pub fn open(index: usize) {
    let url = SITES.lock().ok().and_then(|s| s.get(index).map(|x| x.url.clone()));
    if let Some(url) = url {
        links::open_url(&url);
    }
}