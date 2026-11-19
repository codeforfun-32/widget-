#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

mod icons;
mod logic;
mod media;
mod music;
mod native;
mod render;
mod timefmt;
mod video;
mod weather;

use std::cell::RefCell;
use std::collections::HashMap;
use std::process::Command;
use std::rc::Rc;
use std::time::{Duration, Instant};

use shared::widgets::{self, Doc, Instance};
use shared::Config;
use slint::{ComponentHandle, LogicalSize, PhysicalPosition, Timer, TimerMode};
use windows::Win32::Foundation::HWND;

/// Things every widget needs from the outside world.
pub struct Ctx {
    pub cfg: Config,
    pub icons: icons::Icons,
    pub weather: weather::Hub,
    pub music: music::Music,
    pub app_rects: Vec<(i32, i32, i32, i32)>,
    pub fullscreen: bool,
}

/// One widget on the desktop.
pub struct Live {
    pub ui: WidgetWindow,
    pub inst: Instance,
    pub hwnd: Option<HWND>,
    pub styled: bool,
    pub hovered: bool,
    pub scale: f32,
    pub drag: Option<((i32, i32), (i32, i32))>,
    pub resize: Option<((i32, i32), (i32, i32))>,
    pub last_key: String,
    pub cal_offset: i32,
    pub tasks: Vec<(bool, String)>,
    pub links: Vec<String>,
    pub player: Option<media::Player>,
    pub media_key: String,
    pub music_ver: u64,
    pub body: String,
    pub body_dirty: Option<Instant>,
}

struct App {
    live: HashMap<u32, Live>,
    ctx: Ctx,
    doc_text: String,
    force: bool,
    last_doc: Instant,
    last_slow: Instant,
    last_dyn: Instant,
    last_cfg: Instant,
}

type Shared = Rc<RefCell<App>>;

// ───────── opening things ─────────

fn run_action(id: &str) {
    let go = |prog: &str, args: &[&str]| {
        let _ = Command::new(prog).args(args).spawn();
    };
    match id {
        "lock" => go("rundll32.exe", &["user32.dll,LockWorkStation"]),
        "sleep" => go("rundll32.exe", &["powrprof.dll,SetSuspendState", "0,1,0"]),
        "screenshot" => go("explorer.exe", &["ms-screenclip:"]),
        "calc" => go("calc.exe", &[]),
        "settings" => go("explorer.exe", &["ms-settings:"]),
        "taskmgr" => go("taskmgr.exe", &[]),
        "terminal" => {
            if Command::new("wt.exe").spawn().is_err() {
                go("cmd.exe", &[]);
            }
        }
        "notepad" => go("notepad.exe", &[]),
        "explorer" => go("explorer.exe", &[]),
        "mixer" => go("sndvol.exe", &[]),
        _ => {}
    }
}

fn open_target(t: &str) {
    if t.is_empty() {
        return;
    }
    if let Some(id) = t.strip_prefix("action:") {
        run_action(id);
        return;
    }
    let _ = Command::new("explorer.exe").arg(t).spawn();
}

// ───────── saving what a widget changed ─────────

fn save_prop(l: &mut Live, key: &str, value: &str) {
    l.inst.set(key, value);
    let id = l.inst.id;
    let (k, v) = (key.to_string(), value.to_string());
    widgets::update(|doc| {
        if let Some(i) = doc.get_mut(id) {
            i.set(&k, &v);
        }
    });
}

fn save_geometry(l: &Live) {
    let (id, x, y, w, h) = (l.inst.id, l.inst.x, l.inst.y, l.inst.w, l.inst.h);
    widgets::update(|doc| {
        if let Some(i) = doc.get_mut(id) {
            i.x = x;
            i.y = y;
            i.w = w;
            i.h = h;
        }
    });
}

fn save_tasks(l: &mut Live) {
    let text: String = l
        .tasks
        .iter()
        .map(|(d, t)| format!("{}|{}", if *d { 1 } else { 0 }, t.replace('\n', " ")))
        .collect::<Vec<_>>()
        .join("\n");
    save_prop(l, "tasks", &text);
}

/// Adds a line to a text setting of a widget, after letting the user pick a file.
fn pick_and_append(id: u32, key: &'static str, filter: &'static str) {
    std::thread::spawn(move || {
        if let Some(path) = widgets::pick_file(filter) {
            let name = std::path::Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            widgets::update(|doc| {
                if let Some(i) = doc.get_mut(id) {
                    let mut text = i.get(key);
                    if !text.is_empty() && !text.ends_with('\n') {
                        text.push('\n');
                    }
                    text.push_str(&format!("{} | {}\n", name, path));
                    i.set(key, &text);
                }
            });
        }
    });
}

fn pick_and_set(id: u32, key: &'static str, filter: &'static str) {
    std::thread::spawn(move || {
        if let Some(path) = widgets::pick_file(filter) {
            widgets::update(|doc| {
                if let Some(i) = doc.get_mut(id) {
                    i.set(key, &path);
                }
            });
        }
    });
}

// ───────── wiring the buttons of one widget ─────────

fn wire(ui: &WidgetWindow, app: &Shared, id: u32) {
    let d = ui.global::<D>();

    {
        let app = app.clone();
        d.on_remove(move || {
            widgets::update(|doc| doc.remove(id));
            if let Ok(mut a) = app.try_borrow_mut() {
                a.force = true;
            }
        });
    }

    // ----- move -----
    {
        let app = app.clone();
        d.on_drag_start(move || {
            if let Ok(mut a) = app.try_borrow_mut() {
                if let (Some(l), Some(c)) = (a.live.get_mut(&id), native::cursor()) {
                    l.drag = Some((c, (l.inst.x, l.inst.y)));
                }
            }
        });
    }
    {
        let app = app.clone();
        d.on_drag_move(move || {
            if let Ok(mut a) = app.try_borrow_mut() {
                let c = native::cursor();
                if let (Some(l), Some(c)) = (a.live.get_mut(&id), c) {
                    if let Some((start, pos)) = l.drag {
                        let nx = pos.0 + (c.0 - start.0);
                        let ny = pos.1 + (c.1 - start.1);
                        l.inst.x = nx;
                        l.inst.y = ny;
                        l.ui.window().set_position(PhysicalPosition::new(nx, ny));
                    }
                }
            }
        });
    }
    {
        let app = app.clone();
        d.on_drag_end(move || {
            if let Ok(mut a) = app.try_borrow_mut() {
                if let Some(l) = a.live.get_mut(&id) {
                    l.drag = None;
                    save_geometry(l);
                }
            }
        });
    }

    // ----- resize -----
    {
        let app = app.clone();
        d.on_resize_start(move || {
            if let Ok(mut a) = app.try_borrow_mut() {
                if let (Some(l), Some(c)) = (a.live.get_mut(&id), native::cursor()) {
                    l.resize = Some((c, (l.inst.w, l.inst.h)));
                }
            }
        });
    }
    {
        let app = app.clone();
        d.on_resize_move(move || {
            if let Ok(mut a) = app.try_borrow_mut() {
                let c = native::cursor();
                if let (Some(l), Some(c)) = (a.live.get_mut(&id), c) {
                    if let Some((start, size)) = l.resize {
                        let s = l.scale.max(0.5);
                        let nw = (size.0 as f32 + (c.0 - start.0) as f32 / s).round() as i32;
                        let nh = (size.1 as f32 + (c.1 - start.1) as f32 / s).round() as i32;
                        let nw = nw.clamp(100, 3000);
                        let nh = nh.clamp(60, 3000);
                        l.inst.w = nw;
                        l.inst.h = nh;
                        l.ui.window().set_size(LogicalSize::new(nw as f32, nh as f32));
                    }
                }
            }
        });
    }
    {
        let app = app.clone();
        d.on_resize_end(move || {
            if let Ok(mut a) = app.try_borrow_mut() {
                if let Some(l) = a.live.get_mut(&id) {
                    l.resize = None;
                    save_geometry(l);
                }
            }
        });
    }

    // ----- to-do -----
    {
        let app = app.clone();
        d.on_todo_toggle(move |i| {
            if let Ok(mut a) = app.try_borrow_mut() {
                if let Some(l) = a.live.get_mut(&id) {
                    if let Some(t) = l.tasks.get_mut(i as usize) {
                        t.0 = !t.0;
                    }
                    render::set_tasks(l);
                    save_tasks(l);
                }
            }
        });
    }
    {
        let app = app.clone();
        d.on_todo_remove(move |i| {
            if let Ok(mut a) = app.try_borrow_mut() {
                if let Some(l) = a.live.get_mut(&id) {
                    if (i as usize) < l.tasks.len() {
                        l.tasks.remove(i as usize);
                    }
                    render::set_tasks(l);
                    save_tasks(l);
                }
            }
        });
    }
    {
        let app = app.clone();
        d.on_todo_add(move |text| {
            let text = text.trim().to_string();
            if text.is_empty() {
                return;
            }
            if let Ok(mut a) = app.try_borrow_mut() {
                if let Some(l) = a.live.get_mut(&id) {
                    l.tasks.push((false, text));
                    render::set_tasks(l);
                    save_tasks(l);
                }
            }
        });
    }

    // ----- note -----
    {
        let app = app.clone();
        d.on_body_edited(move |text| {
            if let Ok(mut a) = app.try_borrow_mut() {
                if let Some(l) = a.live.get_mut(&id) {
                    l.body = text.to_string();
                    l.body_dirty = Some(Instant::now());
                }
            }
        });
    }

    // ----- links -----
    {
        let app = app.clone();
        d.on_open_item(move |i| {
            let target = match app.try_borrow() {
                Ok(a) => a.live.get(&id).and_then(|l| l.links.get(i as usize).cloned()),
                Err(_) => None,
            };
            if let Some(t) = target {
                open_target(&t);
            }
        });
    }
    d.on_add_item(move |_| {
        pick_and_append(id, "items", "Programs and files|*.exe;*.lnk;*.*|All files|*.*");
    });

    // ----- media, music, calendar -----
    d.on_media_choose(move || {
        pick_and_set(
            id,
            "path",
            "Pictures, GIFs and videos|*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp;*.mp4;*.m4v;*.mov;*.wmv|All files|*.*",
        );
    });
    d.on_music_cmd(move |cmd| {
        music::command(cmd.as_str());
    });
    {
        let app = app.clone();
        d.on_cal_step(move |n| {
            if let Ok(mut a) = app.try_borrow_mut() {
                if let Some(l) = a.live.get_mut(&id) {
                    if n == 0 {
                        l.cal_offset = 0;
                    } else {
                        l.cal_offset += n;
                    }
                    l.last_key.clear();
                }
            }
        });
    }
}

// ───────── creating, updating and removing windows ─────────

fn create_live(app: &Shared, ctx: &mut Ctx, inst: Instance) -> Option<Live> {
    let ui = WidgetWindow::new().ok()?;
    wire(&ui, app, inst.id);
    let mut live = Live {
        ui,
        inst,
        hwnd: None,
        styled: false,
        hovered: false,
        scale: 1.0,
        drag: None,
        resize: None,
        last_key: String::new(),
        cal_offset: 0,
        tasks: Vec::new(),
        links: Vec::new(),
        player: None,
        media_key: String::new(),
        music_ver: u64::MAX,
        body: String::new(),
        body_dirty: None,
    };
    render::apply(&mut live, ctx, true);
    let (x, y, w, h) = (live.inst.x, live.inst.y, live.inst.w, live.inst.h);
    live.ui.window().set_size(LogicalSize::new(w as f32, h as f32));
    live.ui.window().set_position(PhysicalPosition::new(x, y));
    if live.ui.show().is_err() {
        return None;
    }
    live.ui.window().set_position(PhysicalPosition::new(x, y));
    live.scale = live.ui.window().scale_factor();
    Some(live)
}

fn update_live(l: &mut Live, ctx: &mut Ctx, mut new: Instance) {
    let old = l.inst.clone();
    let busy = l.drag.is_some() || l.resize.is_some();

    // while a widget is open it owns what is typed into it
    for key in ["body", "tasks"] {
        match old.props.get(key) {
            Some(v) => {
                new.props.insert(key.to_string(), v.clone());
            }
            None => {
                new.props.remove(key);
            }
        }
    }

    if busy {
        new.x = old.x;
        new.y = old.y;
        new.w = old.w;
        new.h = old.h;
    } else {
        if new.w != old.w || new.h != old.h {
            l.ui.window().set_size(LogicalSize::new(new.w as f32, new.h as f32));
        }
        if new.x != old.x || new.y != old.y {
            l.ui.window().set_position(PhysicalPosition::new(new.x, new.y));
        }
    }

    let props_changed = new.props != old.props;
    l.inst = new;
    if props_changed {
        render::apply(l, ctx, false);
    }
}

fn reconcile(app: &Shared, a: &mut App) {
    let text = widgets::read_text();
    if text == a.doc_text {
        return;
    }
    a.doc_text = text.clone();
    let doc = Doc::from_text(&text);

    let gone: Vec<u32> = a
        .live
        .keys()
        .filter(|id| !doc.items.iter().any(|i| i.id == **id))
        .cloned()
        .collect();
    for id in gone {
        if let Some(l) = a.live.remove(&id) {
            let _ = l.ui.hide();
        }
    }

    for inst in doc.items {
        if widgets::kind_info(&inst.kind).is_none() {
            continue;
        }
        let id = inst.id;
        if let Some(l) = a.live.get_mut(&id) {
            if l.inst != inst {
                update_live(l, &mut a.ctx, inst);
            }
        } else if let Some(l) = create_live(app, &mut a.ctx, inst) {
            a.live.insert(id, l);
        }
    }
}

// ───────── the heartbeat ─────────

fn tick(app: &Shared) {
    let Ok(mut guard) = app.try_borrow_mut() else { return };
    let a = &mut *guard;
    let now = Instant::now();

    // the list of widgets changed (settings app, or the buttons on a widget)
    if a.force || now.duration_since(a.last_doc) > Duration::from_millis(400) {
        a.force = false;
        a.last_doc = now;
        reconcile(app, a);
    }

    // settings of the whole program
    if now.duration_since(a.last_cfg) > Duration::from_secs(2) {
        a.last_cfg = now;
        a.ctx.cfg = Config::load();
        if !a.ctx.cfg.on("wd_enabled") {
            let _ = slint::quit_event_loop();
            return;
        }
    }

    // slower housekeeping
    if now.duration_since(a.last_slow) > Duration::from_millis(500) {
        a.last_slow = now;
        a.ctx.app_rects = native::app_windows();
        a.ctx.fullscreen = native::fullscreen_app();

        if a.ctx.icons.dirty.swap(false, std::sync::atomic::Ordering::SeqCst) {
            a.ctx.icons.reload_missing();
            for l in a.live.values_mut() {
                if l.inst.kind == "websites" || l.inst.kind == "shortcuts" {
                    render::build_links(l, &mut a.ctx);
                }
            }
        }

        let fg = native::foreground();
        for l in a.live.values_mut() {
            if l.hwnd.is_none() {
                l.hwnd = native::hwnd_of(l.ui.window());
            }
            if let Some(h) = l.hwnd {
                if !l.styled {
                    native::sync_styles(h);
                    l.styled = true;
                }
                if l.drag.is_none() && l.resize.is_none() && fg != h.0 as isize {
                    native::keep_behind(h);
                }
            }
            l.scale = l.ui.window().scale_factor();
        }
    }

    // who is under the mouse
    let under = native::root_under_cursor();
    for l in a.live.values_mut() {
        let me = l.hwnd.map(|h| h.0 as isize).unwrap_or(-1);
        let hov = under == me || l.drag.is_some() || l.resize.is_some();
        if hov != l.hovered {
            l.hovered = hov;
            l.ui.global::<D>().set_hovered(hov);
        }
    }

    // notes: save the text a moment after typing stops
    for l in a.live.values_mut() {
        if let Some(t) = l.body_dirty {
            if now.duration_since(t) > Duration::from_millis(700) {
                l.body_dirty = None;
                let body = l.body.clone();
                save_prop(l, "body", &body);
            }
        }
    }

    // moving content: clocks, weather, music, animations
    let slow_due = now.duration_since(a.last_dyn) > Duration::from_millis(250);
    if slow_due {
        a.last_dyn = now;
    }
    for l in a.live.values_mut() {
        if slow_due || l.inst.kind == "media" {
            render::tick(l, &mut a.ctx);
        }
    }
}

fn main() -> Result<(), slint::PlatformError> {
    if shared::process_count("widgets-app.exe") > 1 {
        return Ok(());
    }
    let cfg = Config::load();
    if !cfg.on("wd_enabled") {
        return Ok(());
    }

    let long_ago = Instant::now() - Duration::from_secs(60);
    let app: Shared = Rc::new(RefCell::new(App {
        live: HashMap::new(),
        ctx: Ctx {
            cfg,
            icons: icons::Icons::new(),
            weather: weather::Hub::new(),
            music: music::Music::new(),
            app_rects: Vec::new(),
            fullscreen: false,
        },
        doc_text: String::new(),
        force: true,
        last_doc: long_ago,
        last_slow: long_ago,
        last_dyn: long_ago,
        last_cfg: Instant::now(),
    }));

    let timer = Timer::default();
    {
        let app = app.clone();
        timer.start(TimerMode::Repeated, Duration::from_millis(33), move || tick(&app));
    }

    slint::run_event_loop_until_quit()
}

