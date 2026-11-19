//! Calendar page and the clock under the mascot.

use std::sync::Mutex;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};
use windows::Win32::System::SystemInformation::GetLocalTime;

use crate::{CalCell, MainWindow};

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
];
const WEEKDAYS: [&str; 7] = [
    "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
];

/// Month being shown: (year, month 1..=12).
static VIEW: Mutex<(i32, u32)> = Mutex::new((2026, 1));

fn today() -> (i32, u32, u32) {
    let t = unsafe { GetLocalTime() };
    (t.wYear as i32, t.wMonth as u32, t.wDay as u32)
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 29 } else { 28 }
        }
    }
}

/// 0 = Sunday ... 6 = Saturday
fn weekday(y: i32, m: u32, d: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    (y + y / 4 - y / 100 + y / 400 + T[(m - 1) as usize] + d as i32).rem_euclid(7) as u32
}

fn shift(y: i32, m: u32, delta: i32) -> (i32, u32) {
    let idx = y * 12 + (m as i32 - 1) + delta;
    (idx.div_euclid(12), (idx.rem_euclid(12) + 1) as u32)
}

fn render(ui: &MainWindow, y: i32, m: u32) {
    let (ty, tm, td) = today();
    let first = weekday(y, m, 1) as i32;
    let dim = days_in_month(y, m) as i32;
    let (py, pm) = shift(y, m, -1);
    let pdim = days_in_month(py, pm) as i32;

    let mut cells = Vec::with_capacity(42);
    for i in 0..42 {
        let offset = i - first;
        let (day, in_month) = if offset < 0 {
            (pdim + offset + 1, false)
        } else if offset < dim {
            (offset + 1, true)
        } else {
            (offset - dim + 1, false)
        };
        cells.push(CalCell {
            day,
            in_month,
            today: in_month && y == ty && m == tm && day as u32 == td,
        });
    }

    ui.set_cal_cells(ModelRc::new(VecModel::from(cells)));
    ui.set_cal_title(format!("{} {}", MONTHS[(m - 1) as usize], y).into());
    ui.set_cal_today_day(td as i32);
    ui.set_cal_today_name(WEEKDAYS[weekday(ty, tm, td) as usize].into());
}

/// Jump to the current month.
pub fn show_today(ui: &MainWindow) {
    let (y, m, _) = today();
    if let Ok(mut v) = VIEW.lock() {
        *v = (y, m);
    }
    render(ui, y, m);
}

/// Go to the previous (-1) or next (+1) month.
pub fn step(ui: &MainWindow, delta: i32) {
    let (y, m) = match VIEW.lock() {
        Ok(mut v) => {
            *v = shift(v.0, v.1, delta);
            *v
        }
        Err(_) => return,
    };
    render(ui, y, m);
}

// ------------------------------------------------------------------ clock

fn update_clock(ui: &MainWindow) {
    let t = unsafe { GetLocalTime() };
    ui.set_clock_time(format!("{:02}:{:02}", t.wHour, t.wMinute).into());
    ui.set_clock_date(format!("{:02}-{:02}-{}", t.wDay, t.wMonth, t.wYear).into());
}

/// Shows the time and date under the mascot and keeps them up to date.
pub fn start_clock(ui: &MainWindow) {
    update_clock(ui);
    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        if let Some(ui) = weak.upgrade() {
            update_clock(&ui);
        }
    });
    // keep the timer running for as long as the app runs
    std::mem::forget(timer);
}