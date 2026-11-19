//! Fills a widget window with its look and its data.
use std::sync::Arc;

use shared::widgets::Instance;
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{logic, timefmt, CalCell, Ctx, HourItem, LinkItem, Live, Numeral, TaskItem, ZoneRow, D};

// ───────── colours ─────────

pub fn parse_color(hex: &str) -> Color {
    let (r, g, b) = rgb_of(hex);
    Color::from_rgb_u8(r, g, b)
}

fn rgb_of(hex: &str) -> (u8, u8, u8) {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0x7cf0c9);
    ((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn argb(a: u8, r: u8, g: u8, b: u8) -> Color {
    Color::from_argb_u8(a, r, g, b)
}

fn mix(c: (u8, u8, u8), base: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let m = |a: u8, b: u8| (a as f32 * t + b as f32 * (1.0 - t)).round() as u8;
    (m(c.0, base.0), m(c.1, base.1), m(c.2, base.2))
}

struct Look {
    bg1: Color,
    bg2: Color,
    edge: Color,
    fg: Color,
    fg2: Color,
    ink: Color,
}

fn look(inst: &Instance) -> Look {
    let acc = rgb_of(&inst.get("accent"));
    let mut choice = inst.get("bg");
    if choice == "Auto" {
        choice = match inst.kind.as_str() {
            "websites" | "shortcuts" | "worldclock" => "Dark",
            "calendar" => "Glass",
            "music" => {
                if inst.get("tint") == "Dark" { "Dark" } else { "Accent" }
            }
            "todo" => "Todo",
            "note" => "Note",
            _ => "None",
        }
        .to_string();
    }
    let dark_edge = argb(0x1c, 255, 255, 255);
    let light_edge = argb(0x18, 0, 0, 0);
    let dark = (argb(0xF2, 0x1b, 0x1c, 0x23), argb(0xF2, 0x12, 0x13, 0x19), dark_edge, false);
    let solid = |a: (u8, u8, u8), b: (u8, u8, u8), edge: Color, light: bool| {
        (argb(0xF2, a.0, a.1, a.2), argb(0xF2, b.0, b.1, b.2), edge, light)
    };

    let (bg1, bg2, edge, light) = match choice.as_str() {
        "Dark" => dark,
        "Glass" => (argb(0xB4, 0x2d, 0x30, 0x3c), argb(0xB4, 0x1a, 0x1c, 0x25), dark_edge, false),
        "Light" => solid((0xf6, 0xf7, 0xfa), (0xe7, 0xe9, 0xef), light_edge, true),
        "Accent" => solid(
            mix(acc, (0x12, 0x12, 0x18), 0.55),
            mix(acc, (0x0c, 0x0c, 0x10), 0.8),
            dark_edge,
            false,
        ),
        "Todo" => match inst.get("theme").as_str() {
            "Light" => solid((0xf6, 0xf4, 0xea), (0xec, 0xe8, 0xd8), light_edge, true),
            "Cream" => solid((0xfb, 0xf3, 0xdc), (0xf3, 0xe7, 0xc3), light_edge, true),
            "Purple" => solid((0x3a, 0x2a, 0x68), (0x24, 0x1a, 0x45), dark_edge, false),
            _ => dark,
        },
        "Note" => match inst.get("color").as_str() {
            "Pink" => solid((0xff, 0xd1, 0xdc), (0xf7, 0xb6, 0xc8), light_edge, true),
            "Mint" => solid((0xd2, 0xf5, 0xe3), (0xa9, 0xe5, 0xc6), light_edge, true),
            "Sky" => solid((0xd4, 0xea, 0xfb), (0xa9, 0xd2, 0xf3), light_edge, true),
            "Lavender" => solid((0xe4, 0xdb, 0xfa), (0xc8, 0xb9, 0xf0), light_edge, true),
            "Cream" => solid((0xf9, 0xf6, 0xec), (0xef, 0xe9, 0xdc), light_edge, true),
            "Dark" => dark,
            _ => solid((0xff, 0xf1, 0x9a), (0xff, 0xe0, 0x7a), light_edge, true),
        },
        _ => (argb(0, 0, 0, 0), argb(0, 0, 0, 0), argb(0, 0, 0, 0), false),
    };

    let (fg, fg2, ink) = if light {
        (argb(255, 0x1b, 0x1d, 0x22), argb(0xA0, 0x1b, 0x1d, 0x22), argb(255, 0x3b, 0x33, 0x20))
    } else {
        (argb(255, 255, 255, 255), argb(0xB4, 255, 255, 255), argb(255, 255, 255, 255))
    };
    Look { bg1, bg2, edge, fg, fg2, ink }
}

fn font_for(inst: &Instance) -> String {
    let f = inst.get("font");
    if f != "Auto" && !f.is_empty() {
        return f;
    }
    if inst.kind == "clock" {
        return match inst.get("style").as_str() {
            "Saturday" | "Stack" | "Flip" => "Bahnschrift",
            "Retro" => "Consolas",
            "Gothic" | "Words" | "Analog Roman" => "Georgia",
            _ => "Segoe UI",
        }
        .to_string();
    }
    "Segoe UI".to_string()
}

// ───────── small helpers ─────────

fn hour12(inst: &Instance, ctx: &Ctx) -> bool {
    match inst.get("time_fmt").as_str() {
        "12 hour" => true,
        "24 hour" => false,
        _ => ctx.cfg.get("tb_clock_fmt") != "24 hour",
    }
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

/// "Name | target" lines.
fn parse_lines(text: &str, web: bool) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        let (name, target) = match l.split_once('|') {
            Some((a, b)) => (a.trim().to_string(), b.trim().to_string()),
            None => (l.to_string(), l.to_string()),
        };
        let mut target = target;
        if web && !target.contains("://") {
            target = format!("https://{}", target);
        }
        let name = if name.is_empty() { target.clone() } else { name };
        out.push((name, target));
    }
    out
}

fn domain_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split('/').next().unwrap_or(rest);
    host.trim_start_matches("www.").to_string()
}

fn action_glyph(id: &str) -> &'static str {
    match id {
        "lock" => "M7 11 L17 11 L17 20 L7 20 Z M9 11 L9 8 C9 3 15 3 15 8 L15 11",
        "sleep" => "M19 14 C13 19 5 15 7 7 C3 9 2 15 6 19 C10 22 17 21 19 14 Z",
        "screenshot" => "M4 8 L4 4 L8 4 M16 4 L20 4 L20 8 M20 16 L20 20 L16 20 M8 20 L4 20 L4 16 M9 12 L15 12",
        "calc" => "M6 3 L18 3 L18 21 L6 21 Z M9 7 L15 7 M9 12 L10 12 M12 12 L13 12 M9 16 L10 16 M12 16 L13 16",
        "settings" => "M12 8 C14 8 16 10 16 12 C16 14 14 16 12 16 C10 16 8 14 8 12 C8 10 10 8 12 8 Z M12 2 L12 5 M12 19 L12 22 M2 12 L5 12 M19 12 L22 12 M5 5 L7 7 M17 17 L19 19 M5 19 L7 17 M17 7 L19 5",
        "taskmgr" => "M4 18 L4 12 M9 18 L9 6 M14 18 L14 10 M19 18 L19 4",
        "terminal" => "M4 6 L20 6 L20 18 L4 18 Z M7 10 L10 12 L7 14 M12 15 L16 15",
        "notepad" => "M6 3 L18 3 L18 21 L6 21 Z M9 8 L15 8 M9 12 L15 12 M9 16 L13 16",
        "explorer" => "M3 7 L10 7 L12 9 L21 9 L21 19 L3 19 Z",
        "mixer" => "M4 9 L8 9 L13 5 L13 19 L8 15 L4 15 Z M16 9 C18 11 18 13 16 15",
        _ => "M5 5 L19 5 L19 19 L5 19 Z",
    }
}

// ───────── static settings -> window ─────────

pub fn apply(live: &mut Live, ctx: &mut Ctx, creating: bool) {
    let inst = live.inst.clone();
    let lk = look(&inst);
    let font = font_for(&inst);
    let accent = parse_color(&inst.get("accent"));
    live.last_key.clear();

    {
        let d = live.ui.global::<D>();
        d.set_kind(SharedString::from(inst.kind.as_str()));
        d.set_style(SharedString::from(inst.get("style").as_str()));
        d.set_accent(accent);
        d.set_fg(lk.fg);
        d.set_fg2(lk.fg2);
        d.set_ink(lk.ink);
        d.set_bg1(lk.bg1);
        d.set_bg2(lk.bg2);
        d.set_edge(lk.edge);
        d.set_radius(inst.num("radius") as f32);
        d.set_font(SharedString::from(font.as_str()));
        d.set_opacity(inst.num("opacity").clamp(20, 100) as f32 / 100.0);
        d.set_locked(inst.on("locked"));
    }

    match inst.kind.as_str() {
        "note" => {
            let d = live.ui.global::<D>();
            d.set_title(SharedString::from(inst.get("title").as_str()));
            d.set_text_size(inst.num("text_size") as f32);
            let tf = if inst.get("font") == "Auto" { "Segoe Script".to_string() } else { font.clone() };
            d.set_title_font(SharedString::from(tf.as_str()));
            if creating {
                live.body = inst.get("body");
                d.set_body(SharedString::from(live.body.as_str()));
            }
        }
        "todo" => {
            {
                let d = live.ui.global::<D>();
                d.set_title(SharedString::from(inst.get("title").as_str()));
                d.set_show_progress(inst.on("show_progress"));
            }
            if creating {
                live.tasks = inst
                    .get("tasks")
                    .lines()
                    .filter_map(|l| {
                        let (flag, text) = l.split_once('|')?;
                        Some((flag == "1", text.to_string()))
                    })
                    .collect();
            }
            set_tasks(live);
        }
        "websites" | "shortcuts" => {
            {
                let d = live.ui.global::<D>();
                d.set_title(SharedString::from(inst.get("title").as_str()));
                let layout = inst.get("layout");
                d.set_link_layout(SharedString::from(layout.as_str()));
                let cols = if inst.kind == "shortcuts" {
                    inst.num("cols")
                } else {
                    ((inst.w / 95).clamp(2, 6)) as i32
                };
                d.set_cols(cols.max(1));
                d.set_banner(inst.kind == "shortcuts" && inst.on("banner"));
            }
            build_links(live, ctx);
        }
        "media" => {
            live.ui.global::<D>().set_media_fit(match inst.get("fit").as_str() {
                "Contain" => 1,
                "Fill" => 2,
                _ => 0,
            });
            load_media(live, &inst);
        }
        "music" => {
            live.ui.global::<D>().set_show_art(inst.on("show_art"));
            ctx.music.start();
            live.music_ver = u64::MAX;
        }
        "weather" => {
            live.ui.global::<D>().set_w_layout(SharedString::from(inst.get("layout").as_str()));
        }
        _ => {}
    }
}

pub fn set_tasks(live: &mut Live) {
    let items: Vec<TaskItem> = live
        .tasks
        .iter()
        .map(|(done, text)| TaskItem { text: SharedString::from(text.as_str()), done: *done })
        .collect();
    let total = items.len();
    let done = items.iter().filter(|t| t.done).count();
    let d = live.ui.global::<D>();
    d.set_tasks(model(items));
    d.set_progress(if total == 0 { 0.0 } else { done as f32 / total as f32 });
}

pub fn build_links(live: &mut Live, ctx: &mut Ctx) {
    let inst = live.inst.clone();
    let web = inst.kind == "websites";
    let entries = parse_lines(&inst.get(if web { "sites" } else { "items" }), web);
    let mut items: Vec<LinkItem> = Vec::new();
    let mut targets: Vec<String> = Vec::new();

    for (name, target) in &entries {
        let letter: String = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
        let mut glyph = String::new();
        let mut icon: Option<slint::Image> = None;
        if web {
            icon = ctx.icons.favicon(&domain_of(target));
        } else if let Some(id) = target.strip_prefix("action:") {
            glyph = action_glyph(id).to_string();
        } else {
            icon = ctx.icons.file(target);
        }
        items.push(LinkItem {
            name: SharedString::from(name.as_str()),
            has_icon: icon.is_some(),
            icon: icon.unwrap_or_default(),
            glyph: SharedString::from(glyph.as_str()),
            letter: SharedString::from(letter.as_str()),
            empty: false,
        });
        targets.push(target.clone());
    }

    if !web {
        // empty tiles with a + (the first one is always there)
        let slots = inst.num("slots").max(1) as usize;
        let mut first = true;
        while first || items.len() < slots {
            first = false;
            items.push(LinkItem {
                name: SharedString::default(),
                icon: slint::Image::default(),
                has_icon: false,
                glyph: SharedString::default(),
                letter: SharedString::default(),
                empty: true,
            });
            targets.push(String::new());
        }
    }

    live.links = targets;
    live.ui.global::<D>().set_items(model(items));
}

fn load_media(live: &mut Live, inst: &Instance) {
    let path = inst.get("path");
    let flip = inst.on("flip");
    let key = format!("{}|{}", path, flip);
    if key == live.media_key {
        return;
    }
    live.media_key = key;
    live.player = None;
    let d = live.ui.global::<D>();
    d.set_has_media(false);
    match crate::media::load(&path, flip) {
        Ok((player, first)) => {
            if let Some(img) = first {
                d.set_media_image(img);
                d.set_has_media(true);
            } else {
                d.set_media_msg(SharedString::from("Loading video..."));
            }
            live.player = Some(player);
        }
        Err(msg) => d.set_media_msg(SharedString::from(msg.as_str())),
    }
}

// ───────── changing content (every few hundred ms) ─────────

pub fn tick(live: &mut Live, ctx: &mut Ctx) {
    match live.inst.kind.as_str() {
        "clock" => clock_tick(live, ctx),
        "worldclock" => world_tick(live, ctx),
        "calendar" => calendar_tick(live, ctx),
        "weather" => weather_tick(live, ctx),
        "music" => music_tick(live, ctx),
        "media" => media_tick(live, ctx),
        _ => {}
    }
}

fn clock_tick(live: &mut Live, ctx: &mut Ctx) {
    let inst = live.inst.clone();
    let zone = {
        let c = inst.get("tz_custom");
        if c.trim().is_empty() { inst.get("tz") } else { c.trim().to_string() }
    };
    let p = timefmt::now_in(&zone, inst.num("shift") as i64);
    let show_sec = inst.on("seconds");
    let key = format!("{}:{}:{}:{}", p.day, p.hour, p.minute, if show_sec { p.second } else { 0 });
    if key == live.last_key {
        return;
    }
    live.last_key = key;

    let style = inst.get("style");
    let h12 = hour12(&inst, ctx);
    let (hh, ampm) = if h12 {
        let h = if p.hour % 12 == 0 { 12 } else { p.hour % 12 };
        (h.to_string(), if p.hour < 12 { "AM" } else { "PM" })
    } else {
        (logic::pad2(p.hour), "")
    };
    let mm = logic::pad2(p.minute);
    let time = if show_sec {
        format!("{}:{}:{}", hh, mm, logic::pad2(p.second))
    } else {
        format!("{}:{}", hh, mm)
    };

    let month_name = logic::MONTHS[(p.month - 1) as usize];
    let day_name = logic::DAYS[p.weekday as usize];
    let upper_day = style == "Saturday" || style == "Gothic";
    let long = format!("{} {} {}", p.day, month_name, p.year);
    let date = match inst.get("date_fmt").as_str() {
        "21 January 2023" => long,
        "21/01/2023" => format!("{:02}/{:02}/{}", p.day, p.month, p.year),
        "01/21/2023" => format!("{:02}/{:02}/{}", p.month, p.day, p.year),
        "2023-01-21" => format!("{}-{:02}-{:02}", p.year, p.month, p.day),
        _ => {
            if style == "Saturday" {
                format!("{} {}, {}.", p.day, month_name.to_uppercase(), p.year)
            } else {
                long
            }
        }
    };

    let hour_deg = ((p.hour % 12) as f32 + p.minute as f32 / 60.0 + p.second as f32 / 3600.0) * 30.0;
    let min_deg = (p.minute as f32 + p.second as f32 / 60.0) * 6.0;
    let sec_deg = p.second as f32 * 6.0;
    let numerals: Vec<Numeral> = if style == "Analog Roman" {
        (0..12)
            .map(|i| {
                let (x, y) = logic::numeral_pos(i);
                Numeral { text: SharedString::from(logic::ROMAN[i]), x, y }
            })
            .collect()
    } else {
        Vec::new()
    };
    let ticks = match style.as_str() {
        "Analog" => logic::ticks(true),
        "Analog Roman" => logic::ticks(false),
        _ => String::new(),
    };

    let d = live.ui.global::<D>();
    d.set_time(SharedString::from(time.as_str()));
    d.set_time_chars(time.chars().count() as i32);
    d.set_hh(SharedString::from(hh.as_str()));
    d.set_mm(SharedString::from(mm.as_str()));
    d.set_secs(SharedString::from(logic::pad2(p.second).as_str()));
    d.set_ampm(SharedString::from(ampm));
    let day_text = if upper_day { day_name.to_uppercase() } else { day_name.to_string() };
    d.set_day(SharedString::from(day_text.as_str()));
    d.set_date(SharedString::from(date.as_str()));
    d.set_month_name(SharedString::from(month_name.to_uppercase().as_str()));
    d.set_day_num(SharedString::from(p.day.to_string().as_str()));
    d.set_word_time(SharedString::from(logic::word_time(p.hour, p.minute).as_str()));
    d.set_show_seconds(show_sec);
    d.set_hand_h(SharedString::from(logic::hand(hour_deg, 24.0, 0.0).as_str()));
    d.set_hand_m(SharedString::from(logic::hand(min_deg, 36.0, 0.0).as_str()));
    d.set_hand_s(SharedString::from(logic::hand(sec_deg, 40.0, 8.0).as_str()));
    d.set_ticks(SharedString::from(ticks.as_str()));
    d.set_numerals(model(numerals));
}

fn world_tick(live: &mut Live, ctx: &mut Ctx) {
    let inst = live.inst.clone();
    let mut zones: Vec<String> = (1..=6)
        .map(|i| inst.get(&format!("zone{}", i)))
        .filter(|z| z != "None" && !z.is_empty())
        .collect();
    let extra = inst.get("zone_custom");
    if !extra.trim().is_empty() {
        zones.push(extra.trim().to_string());
    }
    let h12 = hour12(&inst, ctx);
    let local = timefmt::now_in("Local", 0);
    let local_day = logic::day_number(local.year, local.month, local.day);

    let mut rows: Vec<ZoneRow> = Vec::new();
    let mut key = String::new();
    for z in &zones {
        let p = timefmt::now_in(z, 0);
        let time = if h12 {
            let h = if p.hour % 12 == 0 { 12 } else { p.hour % 12 };
            format!("{}:{:02} {}", h, p.minute, if p.hour < 12 { "AM" } else { "PM" })
        } else {
            format!("{:02}:{:02}", p.hour, p.minute)
        };
        let diff_days = logic::day_number(p.year, p.month, p.day) - local_day;
        let day = match diff_days {
            0 => "Today",
            1 => "Tomorrow",
            -1 => "Yesterday",
            _ => "",
        };
        let off = logic::fmt_offset(p.offset_min - local.offset_min);
        let sub = if z == "Local" { "This PC".to_string() } else { format!("{}, {} h", day, off) };
        key.push_str(&format!("{}{}|", z, time));
        rows.push(ZoneRow {
            city: SharedString::from(logic::zone_label(z).as_str()),
            time: SharedString::from(time.as_str()),
            sub: SharedString::from(sub.as_str()),
        });
    }
    if key == live.last_key {
        return;
    }
    live.last_key = key;
    let d = live.ui.global::<D>();
    d.set_title(SharedString::from(inst.get("title").as_str()));
    d.set_zones(model(rows));
}

fn calendar_tick(live: &mut Live, _ctx: &mut Ctx) {
    let inst = live.inst.clone();
    let today = timefmt::now_in("Local", inst.num("shift") as i64 * 1440);
    let week_start = if inst.get("week_start") == "Monday" { 1 } else { 0 };
    let weekend = inst.on("weekend");
    let key = format!(
        "{}-{}-{}-{}-{}-{}",
        today.year, today.month, today.day, live.cal_offset, week_start, weekend
    );
    if key == live.last_key {
        return;
    }
    live.last_key = key;

    let (y, m) = logic::add_months(today.year, today.month, live.cal_offset);
    let (cells, rows) = logic::calendar_cells(y, m, week_start, (today.year, today.month, today.day));
    let names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let heads: Vec<SharedString> = (0..7)
        .map(|i| SharedString::from(names[((i + week_start) % 7) as usize]))
        .collect();
    let cells: Vec<CalCell> = cells
        .into_iter()
        .map(|c| CalCell {
            text: SharedString::from(c.text.as_str()),
            dim: c.dim,
            today: c.today,
            weekend: c.weekend && weekend,
        })
        .collect();
    let d = live.ui.global::<D>();
    d.set_cal_title(SharedString::from(
        format!("{} {}", logic::MONTHS[(m - 1) as usize].to_uppercase(), y).as_str(),
    ));
    d.set_cal_heads(model(heads));
    d.set_cal_cells(model(cells));
    d.set_cal_rows(rows);
}

fn weather_tick(live: &mut Live, ctx: &mut Ctx) {
    let inst = live.inst.clone();
    let city = inst.get("city");
    let manual = inst.get("loc_mode") == "Manual" && !city.trim().is_empty();
    let key = if manual { format!("city:{}", city.trim()) } else { "auto".to_string() };
    let poll = ctx.weather.poll(&key);
    let stamp = poll.report.as_ref().map(|r| Arc::as_ptr(r) as usize).unwrap_or(0);
    let sig = format!(
        "{}|{}|{}|{}|{}|{}",
        key,
        stamp,
        poll.error,
        inst.get("units"),
        inst.get("wind_unit"),
        inst.get("face")
    );
    if sig == live.last_key {
        return;
    }
    live.last_key = sig;

    let d = live.ui.global::<D>();
    let Some(r) = poll.report else {
        let msg = if poll.error {
            if manual {
                "Couldn't load the weather. Check your internet connection and the city name."
            } else {
                "Couldn't load the weather. Check your internet connection."
            }
        } else {
            "Loading weather..."
        };
        d.set_w_status(SharedString::from(msg));
        return;
    };

    let f = inst.get("units") == "Fahrenheit";
    let wind_unit = inst.get("wind_unit");
    let auto_face = logic::wmo_face(r.code, r.is_day);
    let face = logic::face_from_name(&inst.get("face")).unwrap_or(auto_face);
    let (c1, c2) = logic::face_colors(face);
    let col = |v: u32| argb(0xF0, (v >> 16) as u8, (v >> 8) as u8, v as u8);

    let hours: Vec<HourItem> = r
        .hours
        .iter()
        .map(|(label, t, code, day)| HourItem {
            label: SharedString::from(label.as_str()),
            temp: SharedString::from(logic::fmt_temp_short(*t, f).as_str()),
            code: logic::wmo_face(*code, *day),
        })
        .collect();
    let days: Vec<HourItem> = r
        .days
        .iter()
        .skip(1)
        .take(4)
        .map(|(label, hi, _lo, code)| HourItem {
            label: SharedString::from(label.as_str()),
            temp: SharedString::from(logic::fmt_temp(*hi, f).as_str()),
            code: logic::wmo_face(*code, true),
        })
        .collect();
    let hilo = match r.days.first() {
        Some((_, hi, lo, _)) => format!("H {}  L {}", logic::fmt_temp_short(*hi, f), logic::fmt_temp_short(*lo, f)),
        None => String::new(),
    };

    d.set_w_status(SharedString::default());
    d.set_w_temp(SharedString::from(logic::fmt_temp(r.temp, f).as_str()));
    d.set_w_feels(SharedString::from(logic::fmt_temp(r.feels, f).as_str()));
    d.set_w_cond(SharedString::from(logic::wmo_text(r.code)));
    d.set_w_wind(SharedString::from(logic::fmt_wind(r.wind, &wind_unit).as_str()));
    d.set_w_humidity(SharedString::from(format!("{:.0}%", r.humidity).as_str()));
    d.set_w_place(SharedString::from(r.place.as_str()));
    d.set_w_hilo(SharedString::from(hilo.as_str()));
    d.set_w_code(face);
    d.set_face1(col(c1));
    d.set_face2(col(c2));
    d.set_w_hours(model(hours));
    d.set_w_days(model(days));
}

fn music_tick(live: &mut Live, ctx: &mut Ctx) {
    let snap = ctx.music.snapshot();
    if snap.version == live.music_ver {
        return;
    }
    live.music_ver = snap.version;
    let d = live.ui.global::<D>();
    d.set_m_title(SharedString::from(if snap.title.is_empty() { "Nothing playing" } else { snap.title.as_str() }));
    d.set_m_artist(SharedString::from(snap.artist.as_str()));
    d.set_m_playing(snap.playing);
    d.set_m_progress(snap.progress);
    match snap.art {
        Some((w, h, bytes)) => {
            let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&bytes, w, h);
            d.set_m_art(slint::Image::from_rgba8(buf));
            d.set_m_has_art(true);
        }
        None => d.set_m_has_art(false),
    }
}

/// Should this widget's animation stop right now?
fn media_paused(live: &Live, ctx: &Ctx) -> bool {
    let inst = &live.inst;
    let mode = inst.get("pause_mode");
    if mode == "Never" {
        return false;
    }
    if ctx.fullscreen && ctx.cfg.on("wd_fullscreen") {
        return true;
    }
    if mode == "An app covers it" {
        let s = live.scale;
        let (l, t) = (inst.x, inst.y);
        let (r, b) = (l + (inst.w as f32 * s) as i32, t + (inst.h as f32 * s) as i32);
        let area = ((r - l).max(1) as i64) * ((b - t).max(1) as i64);
        return ctx.app_rects.iter().any(|&(al, at, ar, ab)| {
            let w = (r.min(ar) - l.max(al)).max(0) as i64;
            let h = (b.min(ab) - t.max(at)).max(0) as i64;
            w * h * 100 / area >= 25
        });
    }
    !ctx.app_rects.is_empty()
}

fn media_tick(live: &mut Live, ctx: &mut Ctx) {
    let paused = media_paused(live, ctx);
    let Some(p) = live.player.as_mut() else { return };
    let img = p.tick(paused);
    let err = p.error();
    let d = live.ui.global::<D>();
    if let Some(img) = img {
        d.set_media_image(img);
        d.set_has_media(true);
    }
    if let Some(e) = err {
        d.set_has_media(false);
        d.set_media_msg(SharedString::from(e.as_str()));
    }
}
