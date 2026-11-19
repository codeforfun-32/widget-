#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod audio;
mod calendar;
mod clipboard;
mod companion;
mod links;
mod media;
mod notifications;
mod quick;
mod sites;
mod specs;
mod ticker;
mod weather;
mod webhook;
mod win32;

slint::include_modules!();

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, Timer, TimerMode, VecModel};
use windows::Win32::Foundation::HWND;

use companion::eyes::Eyes;
use companion::state_machine::{Companion, HoverTracker};
use companion::tasks::{Countdown, TimerEvent};

fn sync_bars(model: &VecModel<f32>, levels: &[f32; audio::BARS]) {
    for (i, v) in levels.iter().enumerate() {
        let current = model.row_data(i).unwrap_or(0.0);
        if (current - *v).abs() > 0.002 {
            model.set_row_data(i, *v);
        }
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    let weak = ui.as_weak();
    calendar::start_clock(&ui);

    let viz_model = Rc::new(VecModel::from(vec![0.0f32; audio::BARS]));
    ui.set_viz(viz_model.clone().into());

    let viz = audio::Meter::spawn(audio::Source::Loopback, 5.0);
    let companion = Rc::new(RefCell::new(Companion::new()));
    let countdown = Rc::new(RefCell::new(Countdown::default()));
    

    ui.on_media_prev(media::previous);
    ui.on_media_toggle(media::toggle);
    ui.on_media_next(media::next);
    ui.on_seek(media::seek);

    {
        let weak = weak.clone();
        ui.on_source_picked(move |key| {
            media::select(key.as_str());
            if let Some(ui) = weak.upgrade() { ui.set_source_menu(false); }
        });
    }

    {
        let weak = weak.clone();
        ui.on_title_tapped(move || {
            if let Some(ui) = weak.upgrade() {
                links::open_now_playing(
                    ui.get_source_name().to_string(),
                    ui.get_track_title().to_string(),
                    ui.get_track_artist().to_string(),
                );
            }
        });
    }

    {
        let weak = weak.clone();
        ui.on_copy_clip(move |index| {
            if clipboard::copy(index.max(0) as usize) {
                ticker::push(&weak, "Copied to clipboard".to_string());
            }
        });
    }
    { let weak = weak.clone(); ui.on_clip_remove(move |index| clipboard::remove(&weak, index.max(0) as usize)); }
    { let weak = weak.clone(); ui.on_clip_clear(move || clipboard::clear(&weak)); }
    { let weak = weak.clone(); ui.on_site_add(move || sites::add_from_clipboard(&weak)); }
    ui.on_site_open(|index| sites::open(index.max(0) as usize));
    { let weak = weak.clone(); ui.on_site_remove(move |index| sites::remove(&weak, index.max(0) as usize)); }
    { let weak = weak.clone(); ui.on_notifs_clear(move || notifications::clear(&weak)); }

    {
        let weak = weak.clone();
        ui.on_cal_step(move |delta| {
            if let Some(ui) = weak.upgrade() { calendar::step(&ui, delta); }
        });
    }
    {
        let weak = weak.clone();
        ui.on_cal_reset(move || {
            if let Some(ui) = weak.upgrade() { calendar::show_today(&ui); }
        });
    }

    ui.on_open_settings(|| {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let _ = std::process::Command::new(dir.join("settings-app.exe")).spawn();
            }
        }
    });

    {
        let q = ui.global::<QuickState>();
        q.on_set_wifi(|on| quick::send(quick::Cmd::Wifi(on)));
        q.on_set_bluetooth(|on| quick::send(quick::Cmd::Bluetooth(on)));
        q.on_set_airplane(|on| quick::send(quick::Cmd::Airplane(on)));
        q.on_set_volume(|v| quick::send(quick::Cmd::Volume(v)));
        q.on_set_brightness(|v| quick::send(quick::Cmd::Brightness(v)));
        q.on_open_setting(|uri| quick::open_setting(uri.as_str()));
    }

    { let countdown = countdown.clone(); ui.on_timer_start(move |minutes| { countdown.borrow_mut().start(minutes.max(1) as u64); }); }
    {
        let countdown = countdown.clone();
        let weak = weak.clone();
        ui.on_timer_stop(move || {
            countdown.borrow_mut().stop();
            if let Some(ui) = weak.upgrade() { ui.set_timer_running(false); }
            ticker::push(&weak, "Timer stopped".to_string());
        });
    }

    {
        let companion = companion.clone();
        let weak = weak.clone();
        ui.on_poked(move || {
            let mood = companion.borrow_mut().poke(Instant::now());
            if let Some(ui) = weak.upgrade() { ui.set_mood(mood.as_int()); }
            audio::play_sfx(mood.sfx());
        });
    }

    ui.on_quit_app(|| { let _ = slint::quit_event_loop(); });

    media::spawn_poller(weak.clone());
    clipboard::spawn_watcher(weak.clone());
    webhook::spawn(weak.clone());
    weather::spawn(weak.clone());
    specs::spawn(weak.clone());
    sites::init(weak.clone());
    quick::spawn(weak.clone());
    notifications::spawn(weak.clone());

    let hwnd_cell: Cell<Option<HWND>> = Cell::new(None);
    let origin: Cell<(i32, i32)> = Cell::new((0, 0));
    let last_region: Cell<(i32, i32, i32, i32)> = Cell::new((-1, -1, -1, -1));
    let cal_was_visible: Cell<bool> = Cell::new(false);

    let app_active: Cell<bool> = Cell::new(false);
    let last_fg_check: Cell<Option<Instant>> = Cell::new(None);
    let revealed: Cell<bool> = Cell::new(false);
    let edge_since: Cell<Option<Instant>> = Cell::new(None);
    let last_near: Cell<Instant> = Cell::new(Instant::now());

    let hover = RefCell::new(HoverTracker::new());
    let eyes = RefCell::new(Eyes::new());
    let tick_weak = weak.clone();

    let tick_timer = Timer::default();
    tick_timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        let Some(ui) = tick_weak.upgrade() else { return };
        let window = ui.window();
        let size = window.size();
        if size.width == 0 { return; }
        let scale = window.scale_factor();
        let now = Instant::now();

        let hwnd = match hwnd_cell.get() {
            Some(h) => h,
            None => {
                let Some(h) = win32::window::hwnd_of(window) else { return };
                let (x, y) = win32::position::top_center(size.width as i32);
                win32::window::place(h, x, y);
                origin.set((x, y));
                hwnd_cell.set(Some(h));
                h
            }
        };
        win32::window::sync_styles(hwnd);

        let Some((cx, cy)) = win32::cursor::position() else { return };
        let (ox, oy) = origin.get();
        let rel_x = (cx - ox) as f32 / scale;
        let rel_y = (cy - oy) as f32 / scale;

        let win_w = size.width as f32 / scale;
        let island_w = ui.get_island_w();
        let island_h = ui.get_island_h();
        let island_y = ui.get_island_y();
        let left = (win_w - island_w) / 2.0;
        let tucked = ui.get_tucked();

        let was_expanded = ui.get_expanded();
        let pad = if was_expanded { 8.0 } else { 4.0 };
        let near = rel_x >= left - pad && rel_x <= left + island_w + pad && rel_y >= -2.0 && rel_y <= island_y + island_h + pad;

        if last_fg_check.get().map_or(true, |t| now.duration_since(t) >= Duration::from_millis(300)) {
            app_active.set(win32::foreground::other_app_in_use());
            last_fg_check.set(Some(now));
        }

        let at_edge = rel_y <= 2.0 && (rel_x - win_w / 2.0).abs() <= 84.0;
        if tucked && at_edge {
            let since = edge_since.get().unwrap_or(now);
            edge_since.set(Some(since));
            if now.duration_since(since) >= Duration::from_secs(2) {
                revealed.set(true);
                edge_since.set(None);
                last_near.set(now);
            }
        } else {
            edge_since.set(None);
        }

        if near || was_expanded { last_near.set(now); }
        if !app_active.get() {
            revealed.set(false);
        } else if revealed.get() && now.duration_since(last_near.get()) >= Duration::from_secs(3) {
            revealed.set(false);
        }

        let want_hidden = app_active.get() && !revealed.get() && !was_expanded && !ui.get_spec_pill();
        if want_hidden != ui.get_hidden() { ui.set_hidden(want_hidden); }

        let inside = near && !tucked;
        let expanded = hover.borrow_mut().update(inside, now);
        if expanded != was_expanded { ui.set_expanded(expanded); }
        if !expanded && ui.get_source_menu() { ui.set_source_menu(false); }

        let rpad = if tucked { 0.0 } else { app_state::REGION_PAD };
        let rl = (((left - rpad) * scale).floor() as i32).clamp(0, size.width as i32);
        let rr = (((left + island_w + rpad) * scale).ceil() as i32).clamp(0, size.width as i32);
        let rb = (((island_y + island_h + rpad).max(1.0) * scale).ceil() as i32).clamp(0, size.height as i32);
        let region = (rl, 0, rr, rb);
        if region != last_region.get() {
            win32::window::set_region(hwnd, rl, 0, rr, rb);
            last_region.set(region);
        }

        let (mx, my) = if expanded { (left + 18.0 + 38.0, app_state::ISLAND_TOP + 52.0 + 38.0) } else { (left + 10.0 + 13.0, app_state::ISLAND_TOP + 5.0 + 13.0) };
        if let Some((lx, ly)) = eyes.borrow_mut().update(rel_x - mx, rel_y - my) {
            ui.set_look_x(lx);
            ui.set_look_y(ly);
        }

        let mood = companion.borrow_mut().tick(now);
        ui.set_mood(mood.as_int());

        let event = countdown.borrow_mut().poll(now);
        match event {
            TimerEvent::None => {}
            TimerEvent::Tick(text) => {
                ui.set_timer_text(text.into());
                ui.set_timer_running(true);
            }
            TimerEvent::Finished => {
                ui.set_timer_running(false);
                ui.set_timer_text("00:00".into());
                ticker::push(&tick_weak, "Timer done!".to_string());
                let m = companion.borrow_mut().cheer(now);
                ui.set_mood(m.as_int());
                audio::play_sfx("done");
            }
        }

        quick::set_wanted(expanded && ui.get_page() == 6);

        let cal_visible = expanded && ui.get_page() == 8;
        if cal_visible && !cal_was_visible.get() { calendar::show_today(&ui); }
        cal_was_visible.set(cal_visible);

        let viz_wanted = expanded && ui.get_playing() && ui.get_page() == 0 && !ui.get_source_menu();
        viz.set_enabled(viz_wanted);
        sync_bars(&viz_model, &viz.levels());

        specs::set_wanted((expanded && ui.get_page() == 2) || ui.get_spec_pill());

        if expanded && ui.get_page() == 3 && ui.get_notif_unread() > 0 { ui.set_notif_unread(0); }
    });

    ui.run()
}