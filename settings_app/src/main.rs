#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

use shared::widgets::{self, Doc};
use shared::{Config, Def, Kind, DEFS, PAGES};
use slint::{Color, ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::cell::{Cell, RefCell};
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

fn parse_color(hex: &str) -> Color {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0x7cf0c9);
    Color::from_rgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn strings(list: &[&str]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(list.iter().map(|s| SharedString::from(*s)).collect::<Vec<_>>()))
}

fn make_row(d: &Def, v: String) -> Row {
    let desc = if d.ready {
        d.desc.to_string()
    } else if d.desc.is_empty() {
        "Coming soon".to_string()
    } else if d.kind == Kind::Note {
        d.desc.to_string()
    } else {
        format!("{} · Coming soon", d.desc)
    };
    let kind = match d.kind {
        Kind::Toggle => 0, Kind::Choice => 1, Kind::Slider => 2, Kind::Text => 3,
        Kind::Color => 4, Kind::Note => 5, Kind::Button => 6, Kind::Multi => 8,
    };
    let swatches: Vec<Color> = if d.kind == Kind::Color {
        d.options.iter().map(|h| parse_color(h)).collect()
    } else { vec![] };
    Row {
        kind,
        key: d.key.into(),
        title: d.title.into(),
        desc: desc.into(),
        on: v == "1",
        num: v.parse().unwrap_or(0),
        value: v.into(),
        options: strings(d.options),
        swatches: ModelRc::new(VecModel::from(swatches)),
        min: d.min,
        max: d.max,
        ready: d.ready || d.kind == Kind::Note || d.kind == Kind::Button || d.kind == Kind::Multi,
    }
}

fn note_row(text: &str) -> Row {
    Row { kind: 5, desc: text.into(), ready: true, ..Default::default() }
}

fn build(cfg: &Config, page: usize, q: &str) -> (Vec<Row>, Vec<Option<usize>>) {
    let q = q.trim().to_lowercase();
    let mut rows = Vec::new();
    let mut map = Vec::new();
    let mut last: (&str, &str) = ("", "");

    for (i, d) in DEFS.iter().enumerate() {
        if q.is_empty() {
            if d.page != PAGES[page] { continue; }
        } else {
            if d.kind == Kind::Note { continue; }
            let hay = format!("{} {} {}", d.title, d.desc, d.section).to_lowercase();
            if !hay.contains(&q) { continue; }
        }
        if last != (d.page, d.section) {
            last = (d.page, d.section);
            let title = if q.is_empty() { d.section.to_string() } else { format!("{} · {}", d.page, d.section) };
            rows.push(Row { kind: 7, title: title.into(), ready: true, ..Default::default() });
            map.push(None);
        }
        rows.push(make_row(d, cfg.get(d.key)));
        map.push(Some(i));
    }

    if rows.is_empty() {
        let msg = if q.is_empty() {
            "Widget settings are designed separately and will appear here soon."
        } else {
            "No settings match your search."
        };
        rows.push(note_row(msg));
        map.push(None);
    }
    (rows, map)
}

fn update_preview(ui: &SettingsApp, cfg: &Config) {
    ui.set_p_accent(parse_color(&cfg.get("accent")));
    ui.set_p_radius(match cfg.get("corners").as_str() { "Sharp" => 6.0, "Pill" => 17.0, _ => 12.0 });
    ui.set_p_width(match cfg.get("size").as_str() { "Small" => 150.0, "Large" => 300.0, _ => 220.0 });
}

fn open(target: &str) {
    let _ = Command::new("explorer").arg(target).spawn();
}

fn no_window(c: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let _ = c;
}

fn spawn_sibling(name: &str, args: &[&str]) {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = Command::new(dir.join(name)).args(args).spawn();
        }
    }
}

/// Add or remove a "run at Windows startup" entry for a program next to this one.
fn set_run(value_name: &str, exe_name: &str, on: bool) {
    let exe = std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|d| d.join(exe_name)));
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let mut c = Command::new("reg");
    if on {
        let Some(e) = exe else { return };
        c.args(["add", key, "/v", value_name, "/t", "REG_SZ", "/d"])
            .arg(format!("\"{}\"", e.display()))
            .arg("/f");
    } else {
        c.args(["delete", key, "/v", value_name, "/f"]);
    }
    no_window(&mut c);
    let _ = c.spawn();
}


// ───────── widgets page ─────────

fn kind_look(id: &str) -> (&'static str, Color, Color) {
    let c = |a: u32, b: u32| (Color::from_rgb_u8((a >> 16) as u8, (a >> 8) as u8, a as u8),
                               Color::from_rgb_u8((b >> 16) as u8, (b >> 8) as u8, b as u8));
    let (icon, a, b) = match id {
        "clock" => ("M12 3a9 9 0 1 0 0.01 0 M12 7v5l3 2", 0x4a6cf7, 0x7b3fe4),
        "weather" => ("M7 18a4 4 0 0 1 0-8 5 5 0 0 1 9.5-1A4.5 4.5 0 0 1 17 18z", 0x2b8ad6, 0x5ac8fa),
        "note" => ("M5 4h14v12l-4 4H5z M15 20v-4h4", 0xe0a100, 0xf5cf47),
        "todo" => ("M4 6l2 2 3-3 M12 7h8 M4 13l2 2 3-3 M12 14h8 M4 20h5", 0x23a26d, 0x56d39b),
        "websites" => ("M12 3a9 9 0 1 0 0.01 0 M3 12h18 M12 3c3 3 3 15 0 18 M12 3c-3 3-3 15 0 18", 0xd6406a, 0xff7a59),
        "shortcuts" => ("M4 4h6v6H4z M14 4h6v6h-6z M4 14h6v6H4z M14 14h6v6h-6z", 0x6a4cd6, 0xb06bff),
        "media" => ("M4 5h16v14H4z M4 16l5-5 4 4 3-3 4 4 M9 9.5h0.01", 0xd6568a, 0xff9a8b),
        "music" => ("M9 18V6l10-2v12 M9 18a2.5 2.5 0 1 1-0.01 0 M19 16a2.5 2.5 0 1 1-0.01 0", 0xd63a8a, 0x8a4cff),
        "calendar" => ("M4 6h16v14H4z M4 10h16 M8 3v4 M16 3v4", 0xe05a3a, 0xff9966),
        "worldclock" => ("M12 3a9 9 0 1 0 0.01 0 M12 7v5l3 2 M3 12h2 M19 12h2", 0x0f8b8d, 0x4fd1c5),
        _ => ("M12 5v14 M5 12h14", 0x555555, 0x888888),
    };
    let (c1, c2) = c(a, b);
    (icon, c1, c2)
}

struct W {
    insts: Rc<VecModel<InstItem>>,
    rows: Rc<VecModel<Row>>,
    map: RefCell<Vec<Option<&'static Def>>>,
    expanded: Cell<u32>,
    pending: RefCell<Vec<(u32, String, String)>>,
    sig: RefCell<String>,
    flush_timer: Timer,
}

fn inst_titles(doc: &Doc) -> Vec<String> {
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    doc.items.iter().map(|it| {
        let n = counts.entry(it.kind.clone()).or_insert(0);
        *n += 1;
        let t = widgets::kind_info(&it.kind).map(|k| k.title).unwrap_or("Widget");
        format!("{} {}", t, n)
    }).collect()
}

fn signature(doc: &Doc, expanded: u32) -> String {
    let titles = inst_titles(doc);
    let mut s = format!("{}|", expanded);
    for (it, t) in doc.items.iter().zip(titles) {
        s.push_str(&format!("{}:{}:{}x{};", it.id, t, it.w, it.h));
    }
    s
}

fn rebuild_insts(w: &W, doc: &Doc) {
    let titles = inst_titles(doc);
    let items: Vec<InstItem> = doc.items.iter().zip(titles).map(|(it, t)| {
        let (icon, c1, c2) = kind_look(&it.kind);
        InstItem {
            id: it.id as i32,
            title: t.into(),
            sub: format!("{} × {} px", it.w, it.h).into(),
            expanded: it.id == w.expanded.get(),
            icon: icon.into(),
            c1,
            c2,
        }
    }).collect();
    w.insts.set_vec(items);
    *w.sig.borrow_mut() = signature(doc, w.expanded.get());
}

fn rebuild_rows(w: &W, doc: &Doc) {
    let mut rows = Vec::new();
    let mut map: Vec<Option<&'static Def>> = Vec::new();
    if let Some(inst) = doc.get(w.expanded.get()) {
        let mut last = "";
        for d in widgets::defs_for(&inst.kind) {
            if last != d.section {
                last = d.section;
                rows.push(Row { kind: 7, title: d.section.into(), ready: true, ..Default::default() });
                map.push(None);
            }
            rows.push(make_row(d, inst.get(d.key)));
            map.push(Some(d));
        }
    }
    w.rows.set_vec(rows);
    *w.map.borrow_mut() = map;
}

/// Update rows in place from the file (so the desktop and settings stay in step).
fn sync_rows(w: &W, doc: &Doc) {
    let Some(inst) = doc.get(w.expanded.get()) else { return };
    let map = w.map.borrow();
    for (i, d) in map.iter().enumerate() {
        let Some(d) = d else { continue };
        if matches!(d.kind, Kind::Multi | Kind::Button | Kind::Note) { continue; }
        let v = inst.get(d.key);
        if let Some(mut r) = w.rows.row_data(i) {
            if r.value.as_str() != v {
                r.on = v == "1";
                r.num = v.parse().unwrap_or(0);
                r.value = v.into();
                w.rows.set_row_data(i, r);
            }
        }
    }
}

fn flush(w: &W) {
    let p: Vec<_> = w.pending.borrow_mut().drain(..).collect();
    if p.is_empty() { return; }
    widgets::update(|doc| {
        for (id, k, v) in &p {
            if let Some(i) = doc.get_mut(*id) { i.set(k, v); }
        }
    });
}

fn full_refresh(w: &W) {
    let doc = widgets::load();
    if doc.get(w.expanded.get()).is_none() { w.expanded.set(0); }
    rebuild_insts(w, &doc);
    rebuild_rows(w, &doc);
}

fn ensure_widgets_running(cfg: &Rc<RefCell<Config>>) {
    if !cfg.borrow().on("wd_enabled") {
        cfg.borrow_mut().set("wd_enabled", "1");
        cfg.borrow_mut().save();
        set_run("DynamicIslandWidgets", "widgets-app.exe", true);
    }
    if shared::process_count("widgets-app.exe") == 0 {
        spawn_sibling("widgets-app.exe", &[]);
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = SettingsApp::new()?;
    let cfg = Rc::new(RefCell::new(Config::load()));
    let model = Rc::new(VecModel::<Row>::default());
    let map: Rc<RefCell<Vec<Option<usize>>>> = Rc::new(RefCell::new(Vec::new()));
    let query = Rc::new(RefCell::new(String::new()));
    let page = Rc::new(Cell::new(0usize));
    let save_timer = Rc::new(Timer::default());

    let w = Rc::new(W {
        insts: Rc::new(VecModel::default()),
        rows: Rc::new(VecModel::default()),
        map: RefCell::new(Vec::new()),
        expanded: Cell::new(0),
        pending: RefCell::new(Vec::new()),
        sig: RefCell::new(String::new()),
        flush_timer: Timer::default(),
    });
    ui.set_insts(ModelRc::from(w.insts.clone()));
    ui.set_inst_rows(ModelRc::from(w.rows.clone()));
    ui.set_kinds(ModelRc::new(VecModel::from(
        widgets::KINDS.iter().map(|k| {
            let (icon, c1, c2) = kind_look(k.id);
            KindCard { id: k.id.into(), title: k.title.into(), desc: k.desc.into(), icon: icon.into(), c1, c2 }
        }).collect::<Vec<_>>(),
    )));
    ui.set_widgets_on(cfg.borrow().on("wd_enabled"));

    ui.set_pages(strings(PAGES));
    ui.set_rows(ModelRc::from(model.clone()));

    let refresh: Rc<dyn Fn()> = {
        let (cfg, model, map, query, page) =
            (cfg.clone(), model.clone(), map.clone(), query.clone(), page.clone());
        Rc::new(move || {
            cfg.borrow_mut().sync();
            let (rows, m) = build(&cfg.borrow(), page.get(), &query.borrow());
            model.set_vec(rows);
            *map.borrow_mut() = m;
        })
    };
    refresh();
    update_preview(&ui, &cfg.borrow());

    // sidebar page click
    {
        let (page, query, refresh, w) = (page.clone(), query.clone(), refresh.clone(), w.clone());
        let weak = ui.as_weak();
        ui.on_page_selected(move |i| {
            page.set(i as usize);
            query.borrow_mut().clear();
            if let Some(ui) = weak.upgrade() {
                ui.set_page(i);
                ui.set_query("".into());
                ui.set_widgets_page(PAGES[i as usize] == "Widgets");
            }
            if PAGES[i as usize] == "Widgets" { full_refresh(&w); }
            refresh();
        });
    }

    // search box
    {
        let (query, refresh) = (query.clone(), refresh.clone());
        ui.on_search(move |t| {
            *query.borrow_mut() = t.to_string();
            refresh();
        });
    }

    // a setting changed
    {
        let (cfg, model, map, refresh, save_timer) =
            (cfg.clone(), model.clone(), map.clone(), refresh.clone(), save_timer.clone());
        let weak = ui.as_weak();
        ui.on_changed(move |idx, val| {
            let Some(Some(di)) = map.borrow().get(idx as usize).copied() else { return };
            let d = &DEFS[di];

            if d.kind == Kind::Button {
                match d.key {
                    "open_github" => open("https://github.com/codeforfun-32/Dynamic-island_v3"),
                    "open_config" => {
                        let _ = std::fs::create_dir_all(Config::dir());
                        open(&Config::dir().to_string_lossy());
                    }
                    "save_report" => {
                        let _ = std::fs::create_dir_all(Config::dir());
                        let text = format!("Dynamic Island report\n\n{}\n", cfg.borrow().dump());
                        let _ = std::fs::write(Config::dir().join("report.txt"), text);
                        open(&Config::dir().to_string_lossy());
                    }
                    "tb_restore" => {
                        cfg.borrow_mut().set("tb_enabled", "0");
                        cfg.borrow_mut().save();
                        set_run("DynamicIslandTaskbar", "taskbar-app.exe", false);
                        let mut kill = Command::new("taskkill");
                        kill.args(["/IM", "taskbar-app.exe", "/F"]);
                        no_window(&mut kill);
                        let _ = kill.status();
                        spawn_sibling("taskbar-app.exe", &["--restore"]);
                        refresh();
                    }
                    "wd_restart" => {
                        let mut kill = Command::new("taskkill");
                        kill.args(["/IM", "widgets-app.exe", "/F"]);
                        no_window(&mut kill);
                        let _ = kill.status();
                        if cfg.borrow().on("wd_enabled") { spawn_sibling("widgets-app.exe", &[]); }
                    }
                    "reset_all" => {
                        cfg.borrow_mut().reset();
                        cfg.borrow_mut().save();
                        set_run("DynamicIsland", "dynamic-island.exe", false);
                        set_run("DynamicIslandTaskbar", "taskbar-app.exe", false);
                        set_run("DynamicIslandWidgets", "widgets-app.exe", false);
                        refresh();
                        if let Some(ui) = weak.upgrade() { update_preview(&ui, &cfg.borrow()); }
                    }
                    _ => {}
                }
                return;
            }

            cfg.borrow_mut().set(d.key, &val);

            // update just this row so dragging a slider stays smooth
            let i = idx as usize;
            if let Some(mut r) = model.row_data(i) {
                r.value = val.clone();
                r.on = val == "1";
                r.num = val.parse().unwrap_or(0);
                model.set_row_data(i, r);
            }

            if d.key == "run_at_startup" {
                set_run("DynamicIsland", "dynamic-island.exe", val == "1");
            }
            if d.key == "tb_enabled" {
                cfg.borrow_mut().save(); // save now so the taskbar sees it at once
                set_run("DynamicIslandTaskbar", "taskbar-app.exe", val == "1");
                if val == "1" && shared::process_count("taskbar-app.exe") == 0 {
                    spawn_sibling("taskbar-app.exe", &[]);
                }
            }
            if d.key == "wd_enabled" {
                cfg.borrow_mut().save();
                set_run("DynamicIslandWidgets", "widgets-app.exe", val == "1");
                if val == "1" && shared::process_count("widgets-app.exe") == 0 {
                    spawn_sibling("widgets-app.exe", &[]);
                }
                if let Some(ui) = weak.upgrade() { ui.set_widgets_on(val == "1"); }
            }
            if let Some(ui) = weak.upgrade() { update_preview(&ui, &cfg.borrow()); }

            // save shortly after the last change
            let cfg2 = cfg.clone();
            save_timer.start(TimerMode::SingleShot, Duration::from_millis(400), move || {
                cfg2.borrow_mut().save();
            });
        });
    }

        // ───── widgets page handlers ─────
    {
        let (w, cfg) = (w.clone(), cfg.clone());
        let weak = ui.as_weak();
        ui.on_add_widget(move |i| {
            let Some(k) = widgets::KINDS.get(i as usize) else { return };
            flush(&w);
            let id = widgets::update(|doc| doc.add(k.id));
            w.expanded.set(id);
            ensure_widgets_running(&cfg);
            full_refresh(&w);
            if let Some(ui) = weak.upgrade() { ui.set_widgets_on(true); }
        });
    }
    {
        let w = w.clone();
        ui.on_inst_toggle(move |id| {
            let id = id as u32;
            w.expanded.set(if w.expanded.get() == id { 0 } else { id });
            flush(&w);
            full_refresh(&w);
        });
    }
    {
        let w = w.clone();
        ui.on_inst_delete(move |id| {
            flush(&w);
            widgets::update(|doc| doc.remove(id as u32));
            full_refresh(&w);
        });
    }
    {
        let w = w.clone();
        ui.on_inst_dup(move |id| {
            flush(&w);
            let new = widgets::update(|doc| doc.duplicate(id as u32));
            if let Some(n) = new { w.expanded.set(n); }
            full_refresh(&w);
        });
    }
    {
        let w = w.clone();
        ui.on_inst_changed(move |idx, val| {
            let Some(Some(d)) = w.map.borrow().get(idx as usize).copied() else { return };
            let id = w.expanded.get();
            if id == 0 { return; }
            let val = val.to_string();

            if d.kind == Kind::Button {
                flush(&w);
                match d.key {
                    "add_file" => {
                        if let Some(p) = widgets::pick_file("All files|*.*") {
                            let name = std::path::Path::new(&p).file_stem()
                                .map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                            let line = format!("{} | {}", name, p);
                            widgets::update(|doc| {
                                if let Some(i) = doc.get_mut(id) {
                                    let old = i.get("items");
                                    let new = if old.trim().is_empty() { line.clone() } else { format!("{}\n{}", old.trim_end(), line) };
                                    i.set("items", &new);
                                }
                            });
                            full_refresh(&w);
                        }
                    }
                    "browse_media" => {
                        let f = "Pictures and videos|*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp;*.mp4;*.m4v;*.mov;*.wmv|All files|*.*";
                        if let Some(p) = widgets::pick_file(f) {
                            widgets::update(|doc| { if let Some(i) = doc.get_mut(id) { i.set("path", &p); } });
                            full_refresh(&w);
                        }
                    }
                    _ => {}
                }
                return;
            }

            if d.key == "add_action" {
                if let Some(line) = widgets::action_line(&val) {
                    flush(&w);
                    widgets::update(|doc| {
                        if let Some(i) = doc.get_mut(id) {
                            let old = i.get("items");
                            let new = if old.trim().is_empty() { line.clone() } else { format!("{}\n{}", old.trim_end(), line) };
                            i.set("items", &new);
                            i.set("add_action", "Add quick action...");
                        }
                    });
                    full_refresh(&w);
                }
                return;
            }

            // update the row at once, write to the file shortly after
            let i = idx as usize;
            if let Some(mut r) = w.rows.row_data(i) {
                r.value = val.clone().into();
                r.on = val == "1";
                r.num = val.parse().unwrap_or(0);
                w.rows.set_row_data(i, r);
            }
            w.pending.borrow_mut().push((id, d.key.to_string(), val));
            let w2 = w.clone();
            w.flush_timer.start(TimerMode::SingleShot, Duration::from_millis(120), move || flush(&w2));
        });
    }

    // keep in step with the desktop (a widget closed or moved there)
    let poll = Timer::default();
    {
        let w = w.clone();
        let page = page.clone();
        poll.start(TimerMode::Repeated, Duration::from_millis(600), move || {
            if PAGES[page.get()] != "Widgets" || !w.pending.borrow().is_empty() { return; }
            let doc = widgets::load();
            if doc.get(w.expanded.get()).is_none() && w.expanded.get() != 0 {
                w.expanded.set(0);
                rebuild_insts(&w, &doc);
                rebuild_rows(&w, &doc);
                return;
            }
            if *w.sig.borrow() != signature(&doc, w.expanded.get()) {
                rebuild_insts(&w, &doc);
            }
            sync_rows(&w, &doc);
        });
    }

    ui.run()
}
