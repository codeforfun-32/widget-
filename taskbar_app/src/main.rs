#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

mod icons;
mod native;

use shared::Config;
use slint::{Color, ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

const WINDOW_H: f32 = 260.0; // logical px, matches preferred-height in main.slint
const MAX_APPS: usize = 24;

/// Reads the apps pinned to the real Windows taskbar.
const PINS_SCRIPT: &str = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$d = Join-Path $env:APPDATA 'Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar'
if (Test-Path $d) {
  $s = New-Object -ComObject WScript.Shell
  Get-ChildItem $d -Filter *.lnk | Sort-Object CreationTime | ForEach-Object {
    $t = $s.CreateShortcut($_.FullName).TargetPath
    if ($t) { [Environment]::ExpandEnvironmentVariables($t) }
  }
}
"#;

#[derive(Clone)]
struct Entry {
    exe: String,
    pinned: bool,
    wins: Vec<isize>,
}

#[derive(Clone, PartialEq)]
struct Snap {
    exe: String,
    name: String,
    running: bool,
    active: bool,
    count: i32,
    has_icon: bool,
}

struct State {
    cfg: Config,
    sig: String,
    order: Vec<String>,
    entries: Vec<Entry>,
    icons: icons::Icons,
    snap: Vec<Snap>,
    native_orig: Option<isize>,
    hwnd: Option<HWND>,
    last_region: (i32, i32, i32, i32),
    fullscreen: bool,
    last_near: Instant,
    // while Windows' hidden-icons box is open, the real taskbar must stay available (but see-through)
    peek_start: Option<Instant>,
    peek_until: Option<Instant>,
    alpha_dirty: bool,
    // arrow state
    overflow_was_open: bool,
    overflow_closed_at: Option<Instant>,
    // real app-bar registration (keeps maximised windows above the bar)
    reserved_px: i32,
    appbar_registered: bool,
    tick: u32,
}

fn argb(a: u8, r: u8, g: u8, b: u8) -> Color {
    Color::from_argb_u8(a, r, g, b)
}

fn no_window(c: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let _ = c;
}

fn default_pins() -> Vec<String> {
    [
        r"C:\Windows\explorer.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Mozilla Firefox\firefox.exe",
    ]
    .iter()
    .filter(|p| Path::new(p).exists())
    .map(|p| p.to_string())
    .collect()
}

/// Apps pinned to the Windows taskbar (desktop apps only).
fn import_windows_pins() -> Vec<String> {
    let dir = Config::dir();
    let _ = std::fs::create_dir_all(&dir);
    let script = dir.join("pins.ps1");
    let _ = std::fs::write(&script, PINS_SCRIPT);

    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
        .arg(&script);
    no_window(&mut c);
    let Ok(out) = c.output() else { return vec![] };

    let mut pins: Vec<String> = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let l = line.trim().trim_start_matches('\u{feff}').to_string();
        if l.to_lowercase().ends_with(".exe")
            && Path::new(&l).exists()
            && !pins.iter().any(|p| p.eq_ignore_ascii_case(&l))
        {
            pins.push(l);
        }
    }
    pins
}

/// When the pinned list is empty, copy it from the Windows taskbar (once).
fn ensure_pins(st: &mut State) {
    if !st.cfg.get("tb_pinned_list").trim().is_empty() {
        return;
    }
    let mut pins = import_windows_pins();
    if pins.is_empty() {
        pins = default_pins();
    }
    let v = if pins.is_empty() { "-".to_string() } else { pins.join(";") };
    st.cfg.set("tb_pinned_list", &v);
    st.cfg.save();
    st.sig = st.cfg.dump();
}

fn pinned_list(cfg: &Config) -> Vec<String> {
    cfg.get("tb_pinned_list")
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "-")
        .map(|s| s.to_string())
        .collect()
}

fn stem(exe: &str) -> String {
    let s = Path::new(exe)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "App".into());
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s,
    }
}

/// Push every setting into the window.
fn apply(ui: &TaskbarWindow, cfg: &Config) {
    let size = cfg.get("tb_size");
    let icon: f32 = match size.as_str() {
        "Small" => 24.0,
        "Large" => 38.0,
        _ => 30.0,
    };

    let surface = cfg.get("tb_surface");
    let (bg, edge, fg) = match surface.as_str() {
        "Graphite" => (argb(0xF0, 0x1d, 0x20, 0x28), argb(0x30, 255, 255, 255), argb(255, 255, 255, 255)),
        "Glass" => (argb(0x48, 0xe6, 0xee, 0xf8), argb(0x80, 255, 255, 255), argb(255, 255, 255, 255)),
        "Light" => (argb(0xF2, 0xf3, 0xf5, 0xf9), argb(0x2a, 0, 0, 0), argb(255, 0x1b, 0x1d, 0x22)),
        _ => (argb(0xF2, 0x0a, 0x0a, 0x0d), argb(0x22, 255, 255, 255), argb(255, 255, 255, 255)),
    };

    let corners = cfg.get("tb_corners");
    let notch = corners == "Notch";
    let radius: f32 = match corners.as_str() {
        "Sharp" => 6.0,
        "Pill" => 999.0,
        "Notch" => 20.0,
        _ => 14.0,
    };

    let hover = match cfg.get("tb_hover").as_str() {
        "None" => "none",
        "Highlight" => "highlight",
        _ => "lift",
    };

    let hex = cfg.get("tb_accent");
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0x7cf0c9);
    let accent = argb(255, (v >> 16) as u8, (v >> 8) as u8, v as u8);

    ui.set_icon_size(icon);
    ui.set_radius(radius);
    ui.set_notch(notch);
    ui.set_bg(bg);
    ui.set_edge(edge);
    ui.set_fg(fg);
    ui.set_accent(accent);
    ui.set_hover_mode(SharedString::from(hover));
    ui.set_speed((cfg.num("tb_speed") as f32 / 100.0).max(0.3));
    ui.set_indicator(SharedString::from(cfg.get("tb_indicator")));
    // notch style sits flush with the screen edge
    ui.set_margin(if notch { 0.0 } else { cfg.num("tb_margin") as f32 });
    ui.set_show_start(cfg.on("tb_start"));
    ui.set_show_arrow(cfg.on("tb_arrow"));
    ui.set_show_tray(cfg.on("tb_tray"));
    ui.set_show_bell(cfg.on("tb_bell"));
    ui.set_show_clock(cfg.on("tb_clock"));
    ui.set_show_date(cfg.on("tb_date"));
    ui.set_tooltips(cfg.on("tb_tooltips"));
    ui.set_bounce(cfg.on("tb_bounce"));
}

fn update_clock(ui: &TaskbarWindow, cfg: &Config) {
    let t = native::now();
    let time = if cfg.get("tb_clock_fmt") == "24 hour" {
        format!("{:02}:{:02}", t.h, t.m)
    } else {
        let h12 = if t.h % 12 == 0 { 12 } else { t.h % 12 };
        format!("{}:{:02} {}", h12, t.m, if t.h < 12 { "AM" } else { "PM" })
    };
    let date = if cfg.get("tb_date_fmt") == "MM/DD/YYYY" {
        format!("{}/{}/{}", t.month, t.day, t.year)
    } else {
        format!("{:02}/{:02}/{}", t.day, t.month, t.year)
    };
    ui.set_time_text(time.into());
    ui.set_date_text(date.into());
}

fn make_item(img: Option<slint::Image>, name: &str, snap: &Snap) -> AppItem {
    AppItem {
        name: name.into(),
        letter: name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default().into(),
        has_icon: img.is_some(),
        icon: img.unwrap_or_default(),
        running: snap.running,
        active: snap.active,
        count: snap.count,
    }
}

/// Rebuild the app list from open windows + pinned apps.
fn refresh_apps(st: &mut State, model: &VecModel<AppItem>) {
    let wins = native::list_windows();
    let fg = native::foreground();
    let pins = pinned_list(&st.cfg);
    let show_pinned = st.cfg.on("tb_pinned");
    let show_running = st.cfg.on("tb_running");

    let mut entries: Vec<Entry> = Vec::new();
    if show_pinned {
        for p in &pins {
            entries.push(Entry { exe: p.clone(), pinned: true, wins: vec![] });
        }
    }

    let mut extra: Vec<(String, Vec<isize>)> = Vec::new();
    for w in &wins {
        if let Some(e) = entries.iter_mut().find(|e| e.exe.eq_ignore_ascii_case(&w.exe)) {
            e.wins.push(w.hwnd);
        } else if let Some(x) = extra.iter_mut().find(|x| x.0.eq_ignore_ascii_case(&w.exe)) {
            x.1.push(w.hwnd);
        } else {
            extra.push((w.exe.clone(), vec![w.hwnd]));
        }
    }

    // keep running (unpinned) apps in a stable order
    st.order.retain(|o| extra.iter().any(|x| x.0.eq_ignore_ascii_case(o)));
    for x in &extra {
        if !st.order.iter().any(|o| o.eq_ignore_ascii_case(&x.0)) {
            st.order.push(x.0.clone());
        }
    }
    if show_running {
        for o in st.order.clone() {
            if let Some(x) = extra.iter().find(|x| x.0.eq_ignore_ascii_case(&o)) {
                entries.push(Entry { exe: x.0.clone(), pinned: false, wins: x.1.clone() });
            }
        }
    }
    for e in entries.iter_mut() {
        e.pinned = pins.iter().any(|p| p.eq_ignore_ascii_case(&e.exe));
    }
    entries.truncate(MAX_APPS);

    let mut items: Vec<AppItem> = Vec::new();
    let mut snaps: Vec<Snap> = Vec::new();
    for e in &entries {
        let (img, nm) = st.icons.get(&e.exe);
        let name = nm.unwrap_or_else(|| stem(&e.exe));
        let snap = Snap {
            exe: e.exe.clone(),
            name: name.clone(),
            running: !e.wins.is_empty(),
            active: e.wins.iter().any(|w| *w == fg),
            count: e.wins.len() as i32,
            has_icon: img.is_some(),
        };
        items.push(make_item(img, &name, &snap));
        snaps.push(snap);
    }

    if model.row_count() != items.len() || st.snap.len() != items.len() {
        model.set_vec(items);
    } else {
        for (i, item) in items.into_iter().enumerate() {
            if st.snap[i] != snaps[i] {
                model.set_row_data(i, item);
            }
        }
    }
    st.snap = snaps;
    st.entries = entries;
}

fn launch(exe: &str) {
    let _ = Command::new(exe).spawn();
}

fn toggle_pin(st: &mut State, i: usize) {
    let Some(e) = st.entries.get(i) else { return };
    let exe = e.exe.clone();
    let mut pins = pinned_list(&st.cfg);
    if let Some(p) = pins.iter().position(|p| p.eq_ignore_ascii_case(&exe)) {
        pins.remove(p);
    } else {
        pins.push(exe);
    }
    let v = if pins.is_empty() { "-".to_string() } else { pins.join(";") };
    st.cfg.set("tb_pinned_list", &v);
    st.cfg.save();
    st.sig = st.cfg.dump();
}

fn main() -> Result<(), slint::PlatformError> {
    // safety switch: give the user their normal Windows taskbar back
    if std::env::args().any(|a| a == "--restore") {
        let s = native::native_state();
        native::set_native_state(s & !1);
        native::set_native_alpha(255);
        native::set_native_visible(true);
        return Ok(());
    }
    if shared::process_count("taskbar-app.exe") > 1 {
        return Ok(());
    }
    let cfg = Config::load();
    if !cfg.on("tb_enabled") {
        return Ok(());
    }

    let ui = TaskbarWindow::new()?;
    let model = Rc::new(VecModel::<AppItem>::default());
    ui.set_apps(ModelRc::from(model.clone()));

    let sig = cfg.dump();
    apply(&ui, &cfg);
    update_clock(&ui, &cfg);

    let st = Rc::new(RefCell::new(State {
        cfg,
        sig,
        order: Vec::new(),
        entries: Vec::new(),
        icons: icons::Icons::new(),
        snap: Vec::new(),
        native_orig: None,
        hwnd: None,
        last_region: (0, 0, 0, 0),
        fullscreen: false,
        last_near: Instant::now(),
        peek_start: None,
        peek_until: None,
        alpha_dirty: false,
        overflow_was_open: false,
        overflow_closed_at: None,
        reserved_px: -1,
        appbar_registered: false,
        tick: 0,
    }));

    let bounce_timer = Rc::new(Timer::default());
    let enter_timer = Rc::new(Timer::default());

    // ---- clicks ----
    {
        let (st, model, bounce_timer) = (st.clone(), model.clone(), bounce_timer.clone());
        let weak = ui.as_weak();
        ui.on_app_clicked(move |i| {
            let mut s = st.borrow_mut();
            let Some(e) = s.entries.get(i as usize).cloned() else { return };
            if e.wins.is_empty() {
                launch(&e.exe);
                if s.cfg.on("tb_bounce") {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_bounce_idx(i);
                        let w2 = weak.clone();
                        bounce_timer.start(TimerMode::SingleShot, Duration::from_millis(900), move || {
                            if let Some(ui) = w2.upgrade() { ui.set_bounce_idx(-1); }
                        });
                    }
                }
                return;
            }
            let fg = native::foreground();
            match e.wins.iter().position(|w| *w == fg) {
                Some(_) if e.wins.len() == 1 => native::minimize(fg),
                Some(pos) => native::activate(e.wins[(pos + 1) % e.wins.len()]),
                None => native::activate(e.wins[0]),
            }
            refresh_apps(&mut s, &model);
        });
    }
    {
        let (st, model) = (st.clone(), model.clone());
        ui.on_app_context(move |i| {
            let mut s = st.borrow_mut();
            toggle_pin(&mut s, i as usize);
            refresh_apps(&mut s, &model);
        });
    }
    {
        let (st, model) = (st.clone(), model.clone());
        ui.on_app_middle(move |i| {
            let mut s = st.borrow_mut();
            if let Some(e) = s.entries.get(i as usize) {
                if let Some(w) = e.wins.first() { native::close(*w); }
            }
            refresh_apps(&mut s, &model);
        });
    }
    {
        let (st, enter_timer) = (st.clone(), enter_timer.clone());
        ui.on_tray(move |name| match name.as_str() {
            "start" => native::tap_start(),
            "quick" => native::tap_quick_settings(),
            "clock" | "bell" => native::tap_notifications(),
            "overflow" => {
                // box already open: close it
                if native::overflow_open() {
                    native::tap_escape();
                    return;
                }
                {
                    let mut s = st.borrow_mut();
                    // this click was the one that just closed the box: don't reopen it
                    if s.overflow_closed_at
                        .map(|t| t.elapsed() < Duration::from_millis(500))
                        .unwrap_or(false)
                    {
                        return;
                    }
                    // bring the real taskbar back, but see-through, and open its hidden-icons box
                    let now = Instant::now();
                    s.peek_start = Some(now);
                    s.peek_until = Some(now + Duration::from_secs(4));
                    s.alpha_dirty = true;
                }
                native::set_native_alpha(0);
                native::set_native_visible(true);
                native::tap_win_b();
                enter_timer.start(TimerMode::SingleShot, Duration::from_millis(350), || {
                    native::tap_enter();
                });
            }
            _ => {}
        });
    }

    // ---- slow tick: config, windows, clock, icons ----
    let slow_timer = Timer::default();
    {
        let (st, model) = (st.clone(), model.clone());
        let weak = ui.as_weak();
        let slow = move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut s = st.borrow_mut();

            let fresh = Config::load();
            let sig = fresh.dump();
            if sig != s.sig {
                s.cfg = fresh;
                s.sig = sig;
                apply(&ui, &s.cfg);
            }
            if !s.cfg.on("tb_enabled") {
                let _ = slint::quit_event_loop();
                return;
            }
            ensure_pins(&mut s);

            // hide / show the real Windows taskbar
            let want = s.cfg.on("tb_hide_native");
            match (want, s.native_orig) {
                (true, None) => {
                    let cur = native::native_state();
                    s.native_orig = Some(cur);
                    native::set_native_state(cur | 1);
                }
                (false, Some(orig)) => {
                    native::set_native_state(orig);
                    native::set_native_visible(true);
                    s.native_orig = None;
                }
                _ => {}
            }

            // keep the real taskbar available while Windows' hidden-icons box is open
            let now = Instant::now();
            if let Some(start) = s.peek_start {
                if native::overflow_open() && start.elapsed() < Duration::from_secs(30) {
                    s.peek_until = Some(now + Duration::from_millis(1500));
                }
                if s.peek_until.map(|t| now >= t).unwrap_or(true) {
                    s.peek_start = None;
                    s.peek_until = None;
                }
            }
            if want && s.peek_start.is_none() {
                native::set_native_visible(false);
            }
            // back to normal opacity once the box is closed (taskbar is hidden again by now)
            if s.peek_start.is_none() && s.alpha_dirty {
                native::set_native_alpha(255);
                s.alpha_dirty = false;
            }

            s.fullscreen = s.cfg.on("tb_hide_fullscreen") && native::fullscreen_app();
            update_clock(&ui, &s.cfg);

            if s.icons.dirty.swap(false, Ordering::SeqCst) {
                s.icons.reload_missing();
                s.snap.clear();
            }
            refresh_apps(&mut s, &model);
        };
        slow();
        slow_timer.start(TimerMode::Repeated, Duration::from_millis(500), slow);
    }

    // ---- fast tick: placement, click-through region, auto-hide, arrow ----
    let fast_timer = Timer::default();
    {
        let st = st.clone();
        let weak = ui.as_weak();
        fast_timer.start(TimerMode::Repeated, Duration::from_millis(33), move || {
            let Some(ui) = weak.upgrade() else { return };
            let window = ui.window();
            let size = window.size();
            if size.width == 0 { return; }
            let scale = window.scale_factor();
            let mut s = st.borrow_mut();

            let (sw, sh) = native::screen();
            let hwnd = match s.hwnd {
                Some(h) => h,
                None => {
                    let Some(h) = native::hwnd_of(window) else { return };
                    let wh = (WINDOW_H * scale) as i32;
                    window.set_size(slint::PhysicalSize::new(sw as u32, wh as u32));
                    native::place(h, 0, sh - wh);
                    s.hwnd = Some(h);
                    return;
                }
            };
            native::sync_styles(hwnd);

            // every ~200 ms: stay visible and on top of everything (Win+D, other topmost apps)
            s.tick = s.tick.wrapping_add(1);
            if s.tick % 6 == 0 {
                native::keep_on_top(hwnd);
            }

            // overlap mode: never reserve screen space (gives it back if it was reserved)
            if s.reserved_px != 0 {
                let mut reg = s.appbar_registered;
                native::reserve(hwnd, sw, sh, 0, &mut reg);
                s.appbar_registered = reg;
                s.reserved_px = 0;
            }

            let now = Instant::now();

            // arrow follows whether Windows' hidden-icons box is open
            let open = native::overflow_open();
            if open != s.overflow_was_open {
                if !open {
                    s.overflow_closed_at = Some(now);
                }
                s.overflow_was_open = open;
            }
            if open != ui.get_overflow_open() {
                ui.set_overflow_open(open);
            }

            let dx = ui.get_dock_x();
            let dy = ui.get_dock_y();
            let dw = ui.get_dock_w();
            let dh = ui.get_dock_h();
            let head = ui.get_head();
            let margin = ui.get_margin();
            let win_top = (sh - size.height as i32) as f32;

            if let Some((cx, cy)) = native::cursor() {
                let rx = cx as f32 / scale;
                let ry = (cy as f32 - win_top) / scale;
                let inside = rx >= dx - 8.0 && rx <= dx + dw + 8.0
                    && ry >= dy - head - 8.0 && ry <= dy + dh + margin;
                let at_edge = cy >= sh - 2
                    && (rx - sw as f32 / scale / 2.0).abs() < dw / 2.0 + 140.0;
                if inside || at_edge || ui.get_hover_idx() >= 0 {
                    s.last_near = now;
                }
            }
            let autohide = s.cfg.on("tb_autohide");
            let want_hidden = s.fullscreen
                || (autohide && now.duration_since(s.last_near) > Duration::from_millis(600));
            if want_hidden != ui.get_hidden() { ui.set_hidden(want_hidden); }

            // only the bar itself receives clicks; everything else passes through
            let l = (((dx - 6.0) * scale).floor().max(0.0)) as i32;
            let r = (((dx + dw + 6.0) * scale).ceil() as i32).min(size.width as i32);
            let t = (((dy - head - 6.0) * scale).floor().max(0.0)) as i32;
            let b = (((dy + dh + 4.0) * scale).ceil() as i32).min(size.height as i32);
            let region = if t >= b || l >= r {
                (0, size.height as i32 - 1, 1, size.height as i32)
            } else {
                (l, t, r, b)
            };
            if region != s.last_region {
                native::set_region(hwnd, region.0, region.1, region.2, region.3);
                s.last_region = region;
            }
        });
    }

    let result = ui.run();

    // free the reserved screen space and give the Windows taskbar back
    {
        let mut s = st.borrow_mut();
        if let Some(h) = s.hwnd {
            let (sw, sh) = native::screen();
            let mut reg = s.appbar_registered;
            native::reserve(h, sw, sh, 0, &mut reg);
            s.appbar_registered = reg;
        }
    }
    if let Some(orig) = st.borrow().native_orig {
        native::set_native_state(orig);
    }
    native::set_native_alpha(255);
    native::set_native_visible(true);
    result
}