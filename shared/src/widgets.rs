//! Desktop widgets: the saved list of widgets and the settings every kind offers.
//! Shared by the settings app (edits the list) and the widgets app (shows it).
use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::{btn, cho, col, multi, sli, tog, txt, Config, Def, ACCENTS};

// ───────── what kinds of widget exist ─────────

pub struct KindInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    pub w: i32,
    pub h: i32,
}

pub const KINDS: &[KindInfo] = &[
    KindInfo { id: "clock", title: "Clock", desc: "13 styles, analog and digital", w: 380, h: 190 },
    KindInfo { id: "weather", title: "Weather", desc: "Live faces for sun, rain, snow...", w: 340, h: 540 },
    KindInfo { id: "note", title: "Note", desc: "Sticky notes in many colours", w: 280, h: 250 },
    KindInfo { id: "todo", title: "To-do", desc: "Task list with progress", w: 300, h: 340 },
    KindInfo { id: "websites", title: "My websites", desc: "Links with their logos", w: 270, h: 380 },
    KindInfo { id: "shortcuts", title: "Shortcuts", desc: "Apps, files and quick actions", w: 350, h: 270 },
    KindInfo { id: "media", title: "Image / GIF / Video", desc: "Loops, pauses when you work", w: 380, h: 250 },
    KindInfo { id: "music", title: "Music player", desc: "Controls for what is playing", w: 400, h: 110 },
    KindInfo { id: "calendar", title: "Calendar", desc: "Month view, week start choice", w: 330, h: 310 },
    KindInfo { id: "worldclock", title: "World clock", desc: "Several time zones at once", w: 310, h: 270 },
];

pub fn kind_info(id: &str) -> Option<&'static KindInfo> {
    KINDS.iter().find(|k| k.id == id)
}

// ───────── lists used by the settings ─────────

pub const ZONES: &[&str] = &[
    "Local", "UTC", "Pacific/Honolulu", "America/Anchorage", "America/Los_Angeles",
    "America/Denver", "America/Chicago", "America/New_York", "America/Toronto",
    "America/Mexico_City", "America/Bogota", "America/Lima", "America/Santiago",
    "America/Sao_Paulo", "America/Argentina/Buenos_Aires", "Atlantic/Reykjavik",
    "Europe/London", "Europe/Dublin", "Europe/Lisbon", "Europe/Paris", "Europe/Berlin",
    "Europe/Madrid", "Europe/Rome", "Europe/Amsterdam", "Europe/Zurich", "Europe/Stockholm",
    "Europe/Warsaw", "Europe/Athens", "Europe/Istanbul", "Europe/Kyiv", "Europe/Moscow",
    "Africa/Cairo", "Africa/Lagos", "Africa/Nairobi", "Africa/Johannesburg", "Asia/Jerusalem",
    "Asia/Riyadh", "Asia/Tehran", "Asia/Dubai", "Asia/Karachi", "Asia/Kolkata",
    "Asia/Kathmandu", "Asia/Dhaka", "Asia/Bangkok", "Asia/Jakarta", "Asia/Ho_Chi_Minh",
    "Asia/Singapore", "Asia/Kuala_Lumpur", "Asia/Manila", "Asia/Hong_Kong", "Asia/Shanghai",
    "Asia/Taipei", "Asia/Seoul", "Asia/Tokyo", "Australia/Perth", "Australia/Adelaide",
    "Australia/Sydney", "Pacific/Auckland", "Pacific/Fiji",
];

pub const ZONES_OPT: &[&str] = &[
    "None", "Local", "UTC", "Pacific/Honolulu", "America/Anchorage", "America/Los_Angeles",
    "America/Denver", "America/Chicago", "America/New_York", "America/Toronto",
    "America/Mexico_City", "America/Bogota", "America/Lima", "America/Santiago",
    "America/Sao_Paulo", "America/Argentina/Buenos_Aires", "Atlantic/Reykjavik",
    "Europe/London", "Europe/Dublin", "Europe/Lisbon", "Europe/Paris", "Europe/Berlin",
    "Europe/Madrid", "Europe/Rome", "Europe/Amsterdam", "Europe/Zurich", "Europe/Stockholm",
    "Europe/Warsaw", "Europe/Athens", "Europe/Istanbul", "Europe/Kyiv", "Europe/Moscow",
    "Africa/Cairo", "Africa/Lagos", "Africa/Nairobi", "Africa/Johannesburg", "Asia/Jerusalem",
    "Asia/Riyadh", "Asia/Tehran", "Asia/Dubai", "Asia/Karachi", "Asia/Kolkata",
    "Asia/Kathmandu", "Asia/Dhaka", "Asia/Bangkok", "Asia/Jakarta", "Asia/Ho_Chi_Minh",
    "Asia/Singapore", "Asia/Kuala_Lumpur", "Asia/Manila", "Asia/Hong_Kong", "Asia/Shanghai",
    "Asia/Taipei", "Asia/Seoul", "Asia/Tokyo", "Australia/Perth", "Australia/Adelaide",
    "Australia/Sydney", "Pacific/Auckland", "Pacific/Fiji",
];

pub const FONTS: &[&str] = &[
    "Auto", "Segoe UI", "Bahnschrift", "Georgia", "Times New Roman", "Palatino Linotype",
    "Consolas", "Courier New", "Impact", "Arial Black", "Trebuchet MS", "Verdana",
    "Segoe Script", "Ink Free", "Gabriola", "Old English Text MT", "Lucida Handwriting",
    "Candara", "Corbel", "Calibri",
];

pub const CLOCK_STYLES: &[&str] = &[
    "Saturday", "Bold", "Stack", "Neon", "Retro", "Gothic", "Minimal", "Words",
    "Date tile", "Flip", "Analog", "Analog Roman", "Analog Minimal",
];

const BGS: &[&str] = &["Auto", "None", "Dark", "Glass", "Light", "Accent"];
const TIME_FMTS: &[&str] = &["Auto", "12 hour", "24 hour"];
const DATE_FMTS: &[&str] = &["Auto", "21 January 2023", "21/01/2023", "01/21/2023", "2023-01-21"];
const NOTE_COLORS: &[&str] = &["Yellow", "Pink", "Mint", "Sky", "Lavender", "Cream", "Dark"];
const TODO_THEMES: &[&str] = &["Dark", "Light", "Cream", "Purple"];
const FACES: &[&str] = &["Auto", "Sunny", "Night", "Partly cloudy", "Cloudy", "Foggy", "Rain", "Snow", "Storm"];
const ADD_ACTIONS: &[&str] = &[
    "Add quick action...", "Lock PC", "Sleep", "Screenshot", "Calculator", "Settings",
    "Task manager", "Terminal", "Notepad", "File explorer", "Volume mixer",
];

// ───────── the settings each widget offers ─────────
// page = the kind id, key = the property saved for that widget

static COMMON: [Def; 8] = [
    sli("all", "Size & look", "w", "Width", "Width of the widget (px)", 100, 1600, "300", true),
    sli("all", "Size & look", "h", "Height", "Height of the widget (px)", 60, 1200, "200", true),
    sli("all", "Size & look", "opacity", "Opacity", "How solid the widget is (%)", 20, 100, "100", true),
    cho("all", "Size & look", "bg", "Background", "Auto picks the best one for this widget", BGS, "Auto", true),
    sli("all", "Size & look", "radius", "Corner roundness", "Rounded corners (px)", 0, 60, "18", true),
    cho("all", "Size & look", "font", "Font", "Auto picks one that suits the style", FONTS, "Auto", true),
    col("all", "Size & look", "accent", "Accent colour", "Highlights and details", ACCENTS, "#7cf0c9"),
    tog("all", "Size & look", "locked", "Lock", "Hides the move, resize and close buttons on the desktop", false, true),
];

static CLOCK: [Def; 7] = [
    cho("clock", "Style", "style", "Clock style", "Pick the look", CLOCK_STYLES, "Saturday", true),
    cho("clock", "Time", "tz", "Time zone", "Local follows your PC", ZONES, "Local", true),
    txt("clock", "Time", "tz_custom", "Custom time zone", "Optional, for example Asia/Kolkata. Overrides the list", "", true),
    cho("clock", "Time", "time_fmt", "Time format", "Auto follows the taskbar setting", TIME_FMTS, "Auto", true),
    cho("clock", "Time", "date_fmt", "Date format", "Auto uses the style's own", DATE_FMTS, "Auto", true),
    tog("clock", "Time", "seconds", "Seconds", "Show seconds / the second hand", false, true),
    sli("clock", "Time", "shift", "Time adjust", "Shift the time by some minutes (-12h to +12h)", -720, 720, "0", true),
];

static WEATHER: [Def; 6] = [
    cho("weather", "Location", "loc_mode", "Location", "Auto finds your city from your internet connection", &["Auto", "Manual"], "Auto", true),
    txt("weather", "Location", "city", "City", "Used when Location is Manual (for example London)", "", true),
    cho("weather", "Look", "units", "Units", "Temperature units", &["Celsius", "Fahrenheit"], "Celsius", true),
    cho("weather", "Look", "wind_unit", "Wind speed", "Unit for the wind", &["m/s", "km/h", "mph"], "m/s", true),
    cho("weather", "Look", "layout", "Layout", "Full shows three cards", &["Full", "Compact", "Minimal"], "Full", true),
    cho("weather", "Look", "face", "Face", "Auto changes with the real weather", FACES, "Auto", true),
];

static NOTE: [Def; 3] = [
    txt("note", "Note", "title", "Title", "Shown at the top", "Note", true),
    cho("note", "Note", "color", "Colour", "Paper colour", NOTE_COLORS, "Yellow", true),
    sli("note", "Note", "text_size", "Text size", "Size of the writing (px)", 11, 30, "16", true),
];

static TODO: [Def; 3] = [
    txt("todo", "List", "title", "Title", "Shown at the top", "To-do", true),
    cho("todo", "List", "theme", "Theme", "Card colours", TODO_THEMES, "Dark", true),
    tog("todo", "List", "show_progress", "Progress bar", "Show how much is done", true, true),
];

static WEBSITES: [Def; 3] = [
    txt("websites", "List", "title", "Title", "Shown at the top", "My websites", true),
    cho("websites", "List", "layout", "Layout", "List or icon grid", &["List", "Grid"], "List", true),
    multi("websites", "Sites", "sites", "Sites", "One per line:  Name | https://address",
        "YouTube | https://youtube.com\nInstagram | https://instagram.com\nGmail | https://mail.google.com\nSpotify | https://open.spotify.com\nGoogle Translate | https://translate.google.com"),
];

static SHORTCUTS: [Def; 8] = [
    txt("shortcuts", "Shortcuts", "title", "Title", "Shown at the top", "My Apps", true),
    cho("shortcuts", "Shortcuts", "layout", "Layout", "Apps = big tiles. Actions = small with labels", &["Apps", "Actions"], "Apps", true),
    sli("shortcuts", "Shortcuts", "cols", "Columns", "Tiles per row", 2, 6, "3", true),
    sli("shortcuts", "Shortcuts", "slots", "Empty slots", "Minimum number of tiles shown (empty ones show +)", 3, 16, "6", true),
    tog("shortcuts", "Shortcuts", "banner", "Picture banner", "Show the sunset banner on top", true, true),
    btn("shortcuts", "Add", "add_file", "Add an app or file", "Pick any app, file or folder", "Choose..."),
    cho("shortcuts", "Add", "add_action", "Add a quick action", "Lock, sleep, screenshot and more", ADD_ACTIONS, "Add quick action...", true),
    multi("shortcuts", "Items", "items", "Items", "One per line:  Name | path or action:name",
        ""),
];

static MEDIA: [Def; 5] = [
    txt("media", "File", "path", "File", "Image, GIF or MP4 (full path)", "", true),
    btn("media", "File", "browse_media", "Choose a file", "Pick an image, GIF or MP4", "Choose..."),
    cho("media", "Look", "fit", "Fit", "How the picture fills the widget", &["Cover", "Contain", "Fill"], "Cover", true),
    cho("media", "Playback", "pause_mode", "Pause when", "GIFs and videos stop to save power", &["Any app is open", "An app covers it", "Never"], "Any app is open", true),
    tog("media", "Playback", "flip", "Flip video upside down", "Turn on only if a video appears upside down", false, true),
];

static MUSIC: [Def; 3] = [
    cho("music", "Look", "style", "Style", "Pill, big card or mini", &["Pill", "Card", "Mini"], "Pill", true),
    tog("music", "Look", "show_art", "Album art", "Show the cover picture", true, true),
    cho("music", "Look", "tint", "Colours", "Tint from the accent colour or stay dark", &["Accent", "Dark"], "Accent", true),
];

static CALENDAR: [Def; 4] = [
    cho("calendar", "Style", "style", "Style", "Month grid or compact", &["Month", "Compact"], "Month", true),
    cho("calendar", "Style", "week_start", "Week starts on", "First day of the week", &["Sunday", "Monday"], "Sunday", true),
    tog("calendar", "Style", "weekend", "Dim weekends", "Show Saturdays and Sundays softer", true, true),
    sli("calendar", "Style", "shift", "Date adjust", "Shift today by some days (-30 to +30)", -30, 30, "0", true),
];

static WORLD: [Def; 9] = [
    txt("worldclock", "Zones", "title", "Title", "Shown at the top", "World clock", true),
    cho("worldclock", "Zones", "zone1", "City 1", "Time zone", ZONES_OPT, "Local", true),
    cho("worldclock", "Zones", "zone2", "City 2", "Time zone", ZONES_OPT, "Asia/Tokyo", true),
    cho("worldclock", "Zones", "zone3", "City 3", "Time zone", ZONES_OPT, "Europe/London", true),
    cho("worldclock", "Zones", "zone4", "City 4", "Time zone", ZONES_OPT, "America/New_York", true),
    cho("worldclock", "Zones", "zone5", "City 5", "Time zone", ZONES_OPT, "None", true),
    cho("worldclock", "Zones", "zone6", "City 6", "Time zone", ZONES_OPT, "None", true),
    cho("worldclock", "Time", "time_fmt", "Time format", "Auto follows the taskbar setting", TIME_FMTS, "Auto", true),
    txt("worldclock", "Zones", "zone_custom", "Extra zone", "Optional, for example Asia/Kolkata", "", true),
];

fn specific(kind: &str) -> &'static [Def] {
    match kind {
        "clock" => &CLOCK,
        "weather" => &WEATHER,
        "note" => &NOTE,
        "todo" => &TODO,
        "websites" => &WEBSITES,
        "shortcuts" => &SHORTCUTS,
        "media" => &MEDIA,
        "music" => &MUSIC,
        "calendar" => &CALENDAR,
        "worldclock" => &WORLD,
        _ => &[],
    }
}

/// Every setting a widget of this kind has (size and look first).
pub fn defs_for(kind: &str) -> Vec<&'static Def> {
    let mut v: Vec<&'static Def> = COMMON.iter().collect();
    // clock-like kinds do not need the font/background rows duplicated, they are in COMMON already
    v.extend(specific(kind).iter());
    v
}

// ───────── one widget ─────────

#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub id: u32,
    pub kind: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub props: BTreeMap<String, String>,
}

impl Instance {
    pub fn get(&self, key: &str) -> String {
        match key {
            "w" => return self.w.to_string(),
            "h" => return self.h.to_string(),
            "x" => return self.x.to_string(),
            "y" => return self.y.to_string(),
            _ => {}
        }
        if let Some(v) = self.props.get(key) {
            return v.clone();
        }
        defs_for(&self.kind)
            .iter()
            .find(|d| d.key == key)
            .map(|d| d.default.to_string())
            .unwrap_or_default()
    }
    pub fn num(&self, key: &str) -> i32 {
        self.get(key).parse().unwrap_or(0)
    }
    pub fn on(&self, key: &str) -> bool {
        self.get(key) == "1"
    }
    pub fn set(&mut self, key: &str, value: &str) {
        match key {
            "w" => self.w = value.parse().unwrap_or(self.w).clamp(60, 3000),
            "h" => self.h = value.parse().unwrap_or(self.h).clamp(40, 3000),
            "x" => self.x = value.parse().unwrap_or(self.x),
            "y" => self.y = value.parse().unwrap_or(self.y),
            _ => {
                self.props.insert(key.to_string(), value.to_string());
            }
        }
    }
}

// ───────── the saved list ─────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Doc {
    pub next_id: u32,
    pub items: Vec<Instance>,
}

pub fn path() -> PathBuf {
    Config::dir().join("widgets.json")
}

/// Raw file text, handy to see whether anything changed.
pub fn read_text() -> String {
    std::fs::read_to_string(path()).unwrap_or_default()
}

impl Doc {
    pub fn from_text(text: &str) -> Doc {
        let mut doc = Doc { next_id: 1, items: Vec::new() };
        let Ok(v) = serde_json::from_str::<Value>(text) else { return doc };
        doc.next_id = v["next_id"].as_u64().unwrap_or(1) as u32;
        if let Some(list) = v["items"].as_array() {
            for it in list {
                let Some(id) = it["id"].as_u64() else { continue };
                let Some(kind) = it["kind"].as_str() else { continue };
                let mut props = BTreeMap::new();
                if let Some(obj) = it["props"].as_object() {
                    for (k, val) in obj {
                        if let Some(s) = val.as_str() {
                            props.insert(k.clone(), s.to_string());
                        }
                    }
                }
                doc.items.push(Instance {
                    id: id as u32,
                    kind: kind.to_string(),
                    x: it["x"].as_i64().unwrap_or(100) as i32,
                    y: it["y"].as_i64().unwrap_or(100) as i32,
                    w: it["w"].as_i64().unwrap_or(300) as i32,
                    h: it["h"].as_i64().unwrap_or(200) as i32,
                    props,
                });
                if id as u32 >= doc.next_id {
                    doc.next_id = id as u32 + 1;
                }
            }
        }
        doc
    }

    pub fn to_text(&self) -> String {
        let items: Vec<Value> = self
            .items
            .iter()
            .map(|i| {
                let mut props = Map::new();
                for (k, v) in &i.props {
                    props.insert(k.clone(), Value::String(v.clone()));
                }
                json!({
                    "id": i.id, "kind": i.kind, "x": i.x, "y": i.y, "w": i.w, "h": i.h,
                    "props": Value::Object(props)
                })
            })
            .collect();
        serde_json::to_string_pretty(&json!({ "next_id": self.next_id, "items": items })).unwrap_or_default()
    }

    pub fn get(&self, id: u32) -> Option<&Instance> {
        self.items.iter().find(|i| i.id == id)
    }
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Instance> {
        self.items.iter_mut().find(|i| i.id == id)
    }

    /// Adds a new widget of this kind and returns its id.
    pub fn add(&mut self, kind: &str) -> u32 {
        let info = kind_info(kind);
        let n = self.items.len() as i32 % 8;
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        self.items.push(Instance {
            id,
            kind: kind.to_string(),
            x: 140 + n * 40,
            y: 120 + n * 40,
            w: info.map(|i| i.w).unwrap_or(300),
            h: info.map(|i| i.h).unwrap_or(200),
            props: BTreeMap::new(),
        });
        id
    }

    pub fn duplicate(&mut self, id: u32) -> Option<u32> {
        let mut copy = self.get(id)?.clone();
        let new_id = self.next_id.max(1);
        self.next_id = new_id + 1;
        copy.id = new_id;
        copy.x += 36;
        copy.y += 36;
        self.items.push(copy);
        Some(new_id)
    }

    pub fn remove(&mut self, id: u32) {
        self.items.retain(|i| i.id != id);
    }
}

pub fn load() -> Doc {
    Doc::from_text(&read_text())
}

pub fn save(doc: &Doc) {
    let _ = std::fs::create_dir_all(Config::dir());
    let tmp = Config::dir().join("widgets.json.tmp");
    if std::fs::write(&tmp, doc.to_text()).is_ok() {
        let _ = std::fs::rename(&tmp, path());
    }
}

/// Reads the newest list from disk, lets you change it, and saves it again.
/// Doing it this way keeps the settings app and the widgets app from overwriting each other.
pub fn update<R>(f: impl FnOnce(&mut Doc) -> R) -> R {
    let mut doc = load();
    let r = f(&mut doc);
    save(&doc);
    r
}

// ───────── helpers used by both programs ─────────

/// Turns a choice from "Add a quick action" into a line for the shortcuts list.
pub fn action_line(choice: &str) -> Option<String> {
    let id = match choice {
        "Lock PC" => "lock",
        "Sleep" => "sleep",
        "Screenshot" => "screenshot",
        "Calculator" => "calc",
        "Settings" => "settings",
        "Task manager" => "taskmgr",
        "Terminal" => "terminal",
        "Notepad" => "notepad",
        "File explorer" => "explorer",
        "Volume mixer" => "mixer",
        _ => return None,
    };
    Some(format!("{} | action:{}", choice, id))
}

/// Shows the Windows "open file" window and returns the chosen path.
/// `filter` looks like: "Pictures and videos|*.png;*.jpg;*.gif;*.mp4|All files|*.*"
pub fn pick_file(filter: &str) -> Option<String> {
    let script = format!(
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
         Add-Type -AssemblyName System.Windows.Forms; \
         $f = New-Object System.Windows.Forms.Form; $f.TopMost = $true; \
         $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Filter = '{}'; \
         if ($d.ShowDialog($f) -eq 'OK') {{ [Console]::Out.Write($d.FileName) }}",
        filter.replace('\'', "")
    );
    let mut c = std::process::Command::new("powershell");
    c.args(["-NoProfile", "-STA", "-WindowStyle", "Hidden", "-Command"]).arg(script);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = c.output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}
