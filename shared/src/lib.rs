//! Settings table + config file shared by the island, the settings app and the taskbar.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub mod widgets;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind { Toggle, Choice, Slider, Text, Color, Note, Button, Multi }

pub struct Def {
    pub page: &'static str,
    pub section: &'static str,
    pub key: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    pub kind: Kind,
    pub options: &'static [&'static str],
    pub default: &'static str,
    pub min: i32,
    pub max: i32,
    pub ready: bool,
}

pub const PAGES: &[&str] = &[
    "General", "Accessibility", "Appearance", "Island", "Taskbar", "Panels",
    "Gaming", "Widgets", "Tools", "Keys", "About",
];

pub const fn base(page: &'static str, section: &'static str, key: &'static str,
              title: &'static str, desc: &'static str, kind: Kind) -> Def {
    Def { page, section, key, title, desc, kind, options: &[], default: "", min: 0, max: 0, ready: true }
}
pub const fn tog(p: &'static str, s: &'static str, k: &'static str, t: &'static str, d: &'static str, on: bool, ready: bool) -> Def {
    let mut x = base(p, s, k, t, d, Kind::Toggle);
    x.default = if on { "1" } else { "0" };
    x.ready = ready;
    x
}
pub const fn cho(p: &'static str, s: &'static str, k: &'static str, t: &'static str, d: &'static str,
             o: &'static [&'static str], def: &'static str, ready: bool) -> Def {
    let mut x = base(p, s, k, t, d, Kind::Choice);
    x.options = o; x.default = def; x.ready = ready;
    x
}
pub const fn sli(p: &'static str, s: &'static str, k: &'static str, t: &'static str, d: &'static str,
             min: i32, max: i32, def: &'static str, ready: bool) -> Def {
    let mut x = base(p, s, k, t, d, Kind::Slider);
    x.min = min; x.max = max; x.default = def; x.ready = ready;
    x
}
pub const fn txt(p: &'static str, s: &'static str, k: &'static str, t: &'static str, d: &'static str,
             def: &'static str, ready: bool) -> Def {
    let mut x = base(p, s, k, t, d, Kind::Text);
    x.default = def; x.ready = ready;
    x
}
pub const fn col(p: &'static str, s: &'static str, k: &'static str, t: &'static str, d: &'static str,
             o: &'static [&'static str], def: &'static str) -> Def {
    let mut x = base(p, s, k, t, d, Kind::Color);
    x.options = o; x.default = def;
    x
}
pub const fn multi(p: &'static str, s: &'static str, k: &'static str, t: &'static str, d: &'static str,
             def: &'static str) -> Def {
    let mut x = base(p, s, k, t, d, Kind::Multi);
    x.default = def;
    x
}
pub const fn note(p: &'static str, s: &'static str, k: &'static str, text: &'static str) -> Def {
    base(p, s, k, "", text, Kind::Note)
}
pub const fn btn(p: &'static str, s: &'static str, k: &'static str, t: &'static str, d: &'static str, label: &'static str) -> Def {
    let mut x = base(p, s, k, t, d, Kind::Button);
    x.default = label;
    x
}

pub const LETTERS: &[&str] = &[
    "A","B","C","D","E","F","G","H","I","J","K","L","M",
    "N","O","P","Q","R","S","T","U","V","W","X","Y","Z",
];
pub const ACCENTS: &[&str] = &[
    "#7cf0c9","#6ea8ff","#b48cff","#ff7eb6","#ff6b6b","#ffb86b",
    "#ffe27a","#9be564","#4fd1c5","#e0e0e0","#ff9ff3","#54a0ff",
];
pub const SURFACES: &[&str] = &[
    "Black","Graphite","Midnight","Ocean","Forest","Wine","Sand","Glass","Frosted","Paper",
];

pub static DEFS: &[Def] = &[
    // ───────── General ─────────
    cho("General","App","language","Language","Language of the island and settings",
        &["English","Hindi","Spanish","French","German"],"English",false),
    tog("General","App","run_at_startup","Run at startup","Start the island when you sign in to Windows",false,true),
    tog("General","App","slim_strip","Slim strip","Show only a thin strip until you hover",false,true),
    cho("General","App","open_tab","Open on tab","Which page the island opens to",
        &["Home","Specs","Notifications","Quick settings","Calendar"],"Home",true),

    // ───────── Accessibility ─────────
    cho("Accessibility","Reading","text_size","Text size","Make island text larger",
        &["Small","Normal","Large","Extra large"],"Normal",true),
    tog("Accessibility","Reading","high_contrast","High contrast","Stronger borders and text colours",false,true),
    tog("Accessibility","Motion","reduce_motion","Reduce motion","Fewer animations and a calmer mascot",false,true),
    tog("Accessibility","Motion","longer_notices","Longer notices","Keep pop-ups on screen longer",false,true),
    note("Accessibility","Motion","acc_note","These options change how the island looks and moves, not how Windows itself behaves."),

    // ───────── Appearance ─────────
    cho("Appearance","Shape","shape","Shape","Overall style of the island",
        &["Notch","Pill","Floating"],"Pill",true),
    sli("Appearance","Shape","top_gap","Top gap","Distance from the top of the screen (px)",0,40,"8",true),
    cho("Appearance","Shape","resting_show","When idle show","What the collapsed island displays",
        &["Mascot","Now playing","Clock","Nothing"],"Mascot",true),
    cho("Appearance","Size","size","Size","Collapsed size",
        &["Small","Medium","Large"],"Medium",true),
    cho("Appearance","Size","open_size","Open size","Expanded size",
        &["Compact","Normal","Roomy"],"Normal",true),
    cho("Appearance","Look","motion","Motion","Animation style",
        &["Smooth","Snappy","Bouncy"],"Smooth",true),
    cho("Appearance","Look","visualizer","Visualizer","Music bars style",
        &["Bars","Wave","Dots","Off"],"Bars",true),
    cho("Appearance","Look","corners","Corners","Roundness of the island",
        &["Sharp","Rounded","Pill"],"Pill",true),
    cho("Appearance","Colour","surface","Surface","Island background",SURFACES,"Black",true),
    col("Appearance","Colour","accent","Accent colour","Highlights and mascot glow",ACCENTS,"#7cf0c9"),
    tog("Appearance","Colour","music_bg","Album colours","Tint the island with album art colours",true,true),
    cho("Appearance","Colour","progress_style","Progress bar","Look of the song progress bar",
        &["Line","Thick","Dot"],"Line",true),

    // ───────── Island ─────────
    cho("Island","Position","position","Position","Where the island sits on the top edge",
        &["Center","Left","Right"],"Center",true),
    sli("Island","Position","edge_margin","Edge margin","Gap from the screen side when left/right (px)",0,200,"24",true),
    cho("Island","Position","side_height","Side placement","Vertical placement for side docking",
        &["Top","Middle","Bottom"],"Top",false),
    cho("Island","Position","monitor","Monitor","Which screen shows the island",
        &["Primary","Cursor screen","Screen 2","Screen 3"],"Primary",true),
    tog("Island","Behaviour","hidden_until_hover","Hide until hover","Island is invisible until the cursor reaches the top",false,true),
    sli("Island","Behaviour","open_delay","Open delay","Hover time before opening (ms)",0,800,"120",true),
    tog("Island","Behaviour","reverse_scroll","Reverse scroll","Flip the mouse-wheel direction on the island",false,true),
    sli("Island","Behaviour","anim_speed","Animation speed","Percent of normal speed",50,200,"100",true),
    tog("Island","On-screen pop-ups (HUD)","hud_volume","Volume","Show a pop-up when volume changes",true,true),
    tog("Island","On-screen pop-ups (HUD)","hud_brightness","Brightness","Show a pop-up when brightness changes",true,true),
    tog("Island","On-screen pop-ups (HUD)","hud_caps","Caps / Num lock","Show a pop-up for lock keys",false,true),
    tog("Island","On-screen pop-ups (HUD)","hud_battery","Battery & charging","Show a pop-up when charging starts/stops",true,true),
    tog("Island","Live activities","live_timer","Timer","Show a running timer in the idle island",true,true),
    tog("Island","Live activities","live_downloads","Downloads","Show download progress",false,false),
    tog("Island","Live activities","live_mic","Mic / camera dot","Show a dot when mic or camera is in use",true,false),
    tog("Island","Live activities","live_weather","Weather","Show weather in the idle island",false,true),
    tog("Island","Now playing","np_artwork","Album art","Show cover art",true,true),
    tog("Island","Now playing","np_progress","Progress bar","Show the song progress",true,true),
    tog("Island","Now playing","np_lyrics","Lyrics","Show synced lyrics",false,false),
    tog("Island","Now playing","np_open_on_click","Click title to open","Click the title to jump to the source",true,true),
    tog("Island","Privacy","hide_capture","Hide from screenshots","Island won't appear in screenshots or screen sharing",false,true),

    // ───────── Taskbar ─────────
    tog("Taskbar","Taskbar","tb_enabled","Use custom taskbar","Starts the custom taskbar (with Windows too) and hides the Windows one",false,true),
    cho("Taskbar","Style","tb_size","Icon size","Size of app icons",&["Small","Medium","Large"],"Medium",true),
    cho("Taskbar","Style","tb_corners","Corners","Notch = flush with the screen edge, rounded top",&["Sharp","Rounded","Pill","Notch"],"Rounded",true),
    cho("Taskbar","Style","tb_surface","Surface","Bar background",&["Black","Graphite","Glass","Light"],"Black",true),
    col("Taskbar","Style","tb_accent","Accent colour","Active app indicator",ACCENTS,"#7cf0c9"),
    cho("Taskbar","Style","tb_indicator","Running indicator","How running apps are marked",&["Dot","Line","None"],"Dot",true),
    sli("Taskbar","Style","tb_margin","Edge gap","Distance from the bottom of the screen (px)",0,40,"10",true),
    cho("Taskbar","Hover & motion","tb_hover","Hover effect","What happens when you hover an app",&["None","Highlight","Lift"],"Lift",true),
    sli("Taskbar","Hover & motion","tb_speed","Animation speed","Percent of normal speed",50,200,"100",true),
    tog("Taskbar","Hover & motion","tb_bounce","Launch bounce","Icon bounces when you open an app",true,true),
    tog("Taskbar","Hover & motion","tb_tooltips","Name tooltips","Show the app name when hovering",true,true),
    tog("Taskbar","Behaviour","tb_hide_native","Hide the Windows taskbar","Recommended. Otherwise the two taskbars overlap",true,true),
    tog("Taskbar","Behaviour","tb_autohide","Auto-hide","Slide away until the cursor touches the bottom edge",false,true),
    tog("Taskbar","Behaviour","tb_hide_fullscreen","Hide in fullscreen","Hide while a game or video is fullscreen",true,true),
    tog("Taskbar","Items","tb_start","Start button","Opens the Windows Start menu",true,true),
    tog("Taskbar","Items","tb_pinned","Pinned apps","Show pinned apps even when closed",true,true),
    tog("Taskbar","Items","tb_running","Running apps","Show apps that are open",true,true),
    tog("Taskbar","Items","tb_arrow","Hidden icons arrow","Opens Windows' hidden tray icons box (WhatsApp, Security, Bluetooth...)",true,true),
    tog("Taskbar","Items","tb_tray","System icons","Wi-Fi, sound and battery (opens quick settings)",true,true),
    tog("Taskbar","Items","tb_bell","Notification button","Opens the Windows notification centre",true,true),
    tog("Taskbar","Items","tb_clock","Clock","Show the time",true,true),
    cho("Taskbar","Items","tb_clock_fmt","Clock format","12 or 24 hour",&["12 hour","24 hour"],"12 hour",true),
    tog("Taskbar","Items","tb_date","Show date","Date under the time",true,true),
    cho("Taskbar","Items","tb_date_fmt","Date format","Order of day and month",&["DD/MM/YYYY","MM/DD/YYYY"],"DD/MM/YYYY",true),
    txt("Taskbar","Items","tb_pinned_list","Pinned app list","Full paths to .exe files separated by ;  — clear this box to re-import from the Windows taskbar","",true),
    note("Taskbar","Help","tb_help","Tip: right-click an app in the taskbar to pin or unpin it. Middle-click closes it."),
    btn("Taskbar","Help","tb_restore","Restore Windows taskbar","Use this if the Windows taskbar is missing","Restore"),

    // ───────── Panels ─────────
    tog("Panels","Notifications","mirror_notifs","Mirror Windows notifications","Show Windows notifications in the island",true,true),
    sli("Panels","Notifications","notif_time","Notification time","How long a pop-up stays (seconds)",2,15,"5",true),
    tog("Panels","Notifications","notifs_tab","Notifications tab","Show the notifications page",true,true),
    tog("Panels","History","clip_history","Clipboard history","Keep copied text and images",true,true),
    sli("Panels","History","clip_count","Clipboard items","How many items to keep",5,100,"25",true),
    tog("Panels","History","files_history","Recent files","Show a files shelf",false,false),
    tog("Panels","History","captures_history","Recent captures","Show recent screenshots",false,false),
    tog("Panels","History","phone_history","Phone","Show phone notifications and files",false,false),
    tog("Panels","Panels","snippets","Snippets","Saved text snippets panel",false,false),
    tog("Panels","Panels","controls_tab","Quick settings tab","Wi-Fi, Bluetooth, brightness and more",true,true),
    tog("Panels","Panels","web_tab","Sites tab","Saved sites with logos",true,true),
    tog("Panels","Panels","extensions","Extensions","Third-party widgets",false,false),
    tog("Panels","Panels","deck","Deck","Button deck for shortcuts",false,false),
    tog("Panels","Launcher","launcher","Launcher","App launcher row in the island",false,false),
    txt("Panels","Launcher","launch_1","Slot 1","Path to an app","",false),
    txt("Panels","Launcher","launch_2","Slot 2","Path to an app","",false),
    txt("Panels","Launcher","launch_3","Slot 3","Path to an app","",false),
    txt("Panels","Launcher","launch_4","Slot 4","Path to an app","",false),
    txt("Panels","Launcher","launch_5","Slot 5","Path to an app","",false),
    txt("Panels","Launcher","launch_6","Slot 6","Path to an app","",false),

    // ───────── Gaming ─────────
    tog("Gaming","Fullscreen","hide_fullscreen","Hide in fullscreen","Hide the island when a fullscreen app runs",true,true),
    tog("Gaming","Gamer overlay","gamer_overlay","Gamer overlay","Show stats while gaming",false,true),
    tog("Gaming","Gamer overlay","g_cpu","CPU","Show CPU usage",true,true),
    tog("Gaming","Gamer overlay","g_gpu","GPU","Show GPU usage",true,true),
    tog("Gaming","Gamer overlay","g_ram","RAM","Show memory usage",true,true),
    tog("Gaming","Gamer overlay","g_net","Network speed","Show internet speed",false,true),
    tog("Gaming","Gamer overlay","g_fps","FPS counter","Show frames per second",false,false),
    cho("Gaming","Gamer overlay","strip_bg","Strip background","Background of the gaming strip",
        &["Solid","Transparent","Blur"],"Solid",true),
    note("Gaming","Per-app rules","rules_note","Per-app rules (hide or show the island for chosen apps) are coming soon."),

    // ───────── Widgets ─────────
    tog("Widgets","Desktop widgets","wd_enabled","Show widgets on the desktop","Starts the widgets program (also with Windows). Widgets sit behind your apps",false,true),
    tog("Widgets","Desktop widgets","wd_fullscreen","Hide in fullscreen","Hide widgets while a game or video is fullscreen",true,true),
    btn("Widgets","Desktop widgets","wd_restart","Restart widgets","Use this if a widget looks stuck","Restart"),

    // ───────── Tools ─────────
    txt("Tools","Export","export_captures","Captures folder","Where screenshots are saved","",false),
    txt("Tools","Export","export_files","Files folder","Where shelf files are saved","",false),
    tog("Tools","Night light","night_warning","Night light warning","Warn when night light is on",false,false),
    tog("Tools","Media","media_info_drop","Media info on drop","Show details when you drop a file",false,false),
    tog("Tools","Notify endpoint","notify_enabled","Notify endpoint","Let other apps send notices to the island",true,true),
    txt("Tools","Notify endpoint","notify_port","Port","Local port for the endpoint","8765",true),
    note("Tools","Notify endpoint","notify_example","curl -X POST http://127.0.0.1:8765/notify -d \"Build finished\""),

    // ───────── Keys ─────────
    cho("Keys","Shortcuts (Ctrl + Alt + letter)","key_open","Open / close island","Ctrl + Alt + letter",LETTERS,"N",true),
    cho("Keys","Shortcuts (Ctrl + Alt + letter)","key_play","Play / pause","Ctrl + Alt + letter",LETTERS,"P",true),
    cho("Keys","Shortcuts (Ctrl + Alt + letter)","key_timer","Start timer","Ctrl + Alt + letter",LETTERS,"T",true),
    cho("Keys","Shortcuts (Ctrl + Alt + letter)","key_hide","Hide / show island","Ctrl + Alt + letter",LETTERS,"H",true),
    cho("Keys","Shortcuts (Ctrl + Alt + letter)","key_mute","Mute microphone","Ctrl + Alt + letter",LETTERS,"M",false),
    cho("Keys","Shortcuts (Ctrl + Alt + letter)","key_audio","Audio output","Ctrl + Alt + letter",LETTERS,"O",false),
    cho("Keys","Shortcuts (Ctrl + Alt + letter)","key_color","Colour picker","Ctrl + Alt + letter",LETTERS,"C",false),

    // ───────── About ─────────
    btn("About","Project","open_github","GitHub","Source code and updates","Open"),
    btn("About","Project","open_config","Config folder","Where your settings are stored","Open"),
    btn("About","Support","save_report","Save report","Writes a text report you can share when reporting a bug","Save"),
    btn("About","Danger zone","reset_all","Reset all settings","Restore every setting to its default","Reset"),
];

// ───────── Config file ─────────

#[derive(Default)]
pub struct Config {
    map: BTreeMap<String, String>,
    touched: BTreeSet<String>,
    wiped: bool,
}

fn read_disk() -> BTreeMap<String, String> {
    std::fs::read_to_string(Config::path())
        .ok()
        .and_then(|s| serde_json::from_str::<BTreeMap<String, String>>(&s).ok())
        .unwrap_or_default()
}

impl Config {
    pub fn dir() -> PathBuf {
        let base = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
        base.join("DynamicIsland")
    }
    pub fn path() -> PathBuf { Self::dir().join("config.json") }

    pub fn load() -> Self {
        Self { map: read_disk(), touched: BTreeSet::new(), wiped: false }
    }

    /// Pick up changes other programs wrote, keeping what this program changed itself.
    pub fn sync(&mut self) {
        let mut m = read_disk();
        for k in &self.touched {
            if let Some(v) = self.map.get(k) { m.insert(k.clone(), v.clone()); }
        }
        self.map = m;
    }

    /// Saves only what this program changed, on top of what is on disk.
    pub fn save(&mut self) {
        if !self.wiped {
            let mut m = read_disk();
            for k in &self.touched {
                if let Some(v) = self.map.get(k) { m.insert(k.clone(), v.clone()); }
            }
            self.map = m;
        }
        let _ = std::fs::create_dir_all(Self::dir());
        if let Ok(text) = serde_json::to_string_pretty(&self.map) {
            let tmp = Self::dir().join("config.json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, Self::path());
            }
        }
        self.touched.clear();
        self.wiped = false;
    }

    pub fn get(&self, key: &str) -> String {
        if let Some(v) = self.map.get(key) { return v.clone(); }
        DEFS.iter().find(|d| d.key == key).map(|d| d.default.to_string()).unwrap_or_default()
    }
    pub fn on(&self, key: &str) -> bool { self.get(key) == "1" }
    pub fn num(&self, key: &str) -> i32 { self.get(key).parse().unwrap_or(0) }
    pub fn set(&mut self, key: &str, value: &str) {
        self.map.insert(key.to_string(), value.to_string());
        self.touched.insert(key.to_string());
    }
    pub fn reset(&mut self) {
        self.map.clear();
        self.touched.clear();
        self.wiped = true;
    }
    pub fn dump(&self) -> String {
        DEFS.iter()
            .filter(|d| !matches!(d.kind, Kind::Note | Kind::Button))
            .map(|d| format!("{}.{} = {}", d.page, d.key, self.get(d.key)))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// How many copies of a program (for example "taskbar-app.exe") are running.
pub fn process_count(name: &str) -> usize {
    let mut c = std::process::Command::new("tasklist");
    c.arg("/FI").arg(format!("IMAGENAME eq {}", name)).arg("/NH");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    match c.output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_lowercase().matches(&name.to_lowercase()).count(),
        Err(_) => 0,
    }
}