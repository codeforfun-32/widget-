use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use slint::Weak;
use sysinfo::{Networks, System};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

use crate::MainWindow;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const GB: f64 = 1024.0 * 1024.0 * 1024.0;

/// How often CPU, RAM, GPU and battery are measured.
const BASIC_EVERY: Duration = Duration::from_secs(5);
/// How often the network speed is measured (averaged over this time).
const NET_EVERY: Duration = Duration::from_secs(10);
/// How often the GPU reading is refreshed.
const GPU_EVERY: Duration = Duration::from_secs(10);

/// Sampling only runs while the Specs tab is open or pinned to the pill.
static WANTED: AtomicBool = AtomicBool::new(false);
/// GPU load in tenths of a percent. u32::MAX = unknown.
static GPU_TENTHS: AtomicU32 = AtomicU32::new(u32::MAX);

pub fn set_wanted(on: bool) {
    WANTED.store(on, Ordering::Relaxed);
}

fn speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1_048_576.0 {
        format!("{:.1} MB/s", bytes_per_sec / 1_048_576.0)
    } else if bytes_per_sec >= 1024.0 {
        format!("{:.0} KB/s", bytes_per_sec / 1024.0)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

/// (charge 0..1, plugged in, has battery)
fn battery() -> (f32, bool, bool) {
    let mut status = SYSTEM_POWER_STATUS::default();
    let ok = unsafe { GetSystemPowerStatus(&mut status).is_ok() };
    if !ok {
        return (0.0, false, false);
    }
    let present = status.BatteryFlag != 128 && status.BatteryLifePercent != 255;
    (status.BatteryLifePercent as f32 / 100.0, status.ACLineStatus == 1, present)
}

fn net_totals() -> (u64, u64) {
    let networks = Networks::new_with_refreshed_list();
    let mut rx = 0u64;
    let mut tx = 0u64;
    for (name, data) in networks.iter() {
        if name.to_lowercase().contains("loopback") {
            continue;
        }
        rx += data.total_received();
        tx += data.total_transmitted();
    }
    (rx, tx)
}

/// Reads the 3D engine load of all GPUs through the Windows `typeperf` tool.
fn parse_gpu(output: &str) -> Option<f32> {
    let mut lines = output.lines().filter(|l| l.starts_with('"'));
    let _header = lines.next()?;
    let values = lines.next()?;
    let mut sum = 0.0f32;
    let mut any = false;
    for (i, cell) in values.split("\",\"").enumerate() {
        if i == 0 {
            continue; // time stamp
        }
        if let Ok(v) = cell.trim_matches('"').trim().parse::<f32>() {
            sum += v;
            any = true;
        }
    }
    if any { Some(sum.min(100.0)) } else { None }
}

fn spawn_gpu() {
    thread::spawn(|| loop {
        if !WANTED.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(500));
            continue;
        }
        let output = Command::new("typeperf")
            .args(["\\GPU Engine(*engtype_3D)\\Utilization Percentage", "-sc", "1"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let value = output
            .ok()
            .and_then(|o| parse_gpu(&String::from_utf8_lossy(&o.stdout)));
        match value {
            Some(pct) => {
                GPU_TENTHS.store((pct * 10.0) as u32, Ordering::Relaxed);
                thread::sleep(GPU_EVERY);
            }
            None => {
                GPU_TENTHS.store(u32::MAX, Ordering::Relaxed);
                thread::sleep(Duration::from_secs(5));
            }
        }
    });
}

pub fn spawn(ui: Weak<MainWindow>) {
    spawn_gpu();
    thread::spawn(move || {
        let mut sys = System::new();
        let mut net_base: Option<(u64, u64, Instant)> = None;
        let mut last_basic: Option<Instant> = None;
        let mut active = false;

        // the numbers on screen: they only change when a new measurement is taken
        let mut cpu = 0.0f32;
        let mut ram = 0.0f32;
        let mut ram_text = String::from("--");
        let mut gpu = -1.0f32;
        let mut batt = 0.0f32;
        let mut charging = false;
        let mut present = false;
        let mut down_text = String::from("--");
        let mut up_text = String::from("--");

        loop {
            if !WANTED.load(Ordering::Relaxed) {
                active = false;
                net_base = None;
                last_basic = None;
                thread::sleep(Duration::from_millis(300));
                continue;
            }

            if !active {
                // just switched on: start the network clock and warm up the CPU counter
                active = true;
                sys.refresh_cpu_usage();
                let (rx, tx) = net_totals();
                net_base = Some((rx, tx, Instant::now()));
                thread::sleep(Duration::from_millis(1500));
            }

            let now = Instant::now();
            let mut changed = false;

            // network speed: every NET_EVERY, averaged over that time
            if let Some((prx, ptx, t)) = net_base {
                if now.duration_since(t) >= NET_EVERY {
                    let (rx, tx) = net_totals();
                    let dt = now.duration_since(t).as_secs_f64().max(0.001);
                    down_text = speed(rx.saturating_sub(prx) as f64 / dt);
                    up_text = speed(tx.saturating_sub(ptx) as f64 / dt);
                    net_base = Some((rx, tx, now));
                    changed = true;
                }
            }

            // cpu, memory, gpu, battery: every BASIC_EVERY
            let basic_due = last_basic.map_or(true, |t| now.duration_since(t) >= BASIC_EVERY);
            if basic_due {
                sys.refresh_cpu_usage();
                sys.refresh_memory();
                cpu = (sys.global_cpu_usage() / 100.0).clamp(0.0, 1.0);
                let total = sys.total_memory() as f64;
                let used = sys.used_memory() as f64;
                ram = if total > 0.0 { (used / total) as f32 } else { 0.0 };
                ram_text = format!("{:.1} / {:.1} GB", used / GB, total / GB);

                let raw = GPU_TENTHS.load(Ordering::Relaxed);
                gpu = if raw == u32::MAX { -1.0 } else { raw as f32 / 1000.0 };

                let (b, c, p) = battery();
                batt = b;
                charging = c;
                present = p;

                last_basic = Some(now);
                changed = true;
            }

            if changed {
                // compact text for the collapsed pill
                let mut text = format!("CPU {:.0}%", cpu * 100.0);
                if gpu >= 0.0 {
                    text.push_str(&format!("  GPU {:.0}%", gpu * 100.0));
                }
                text.push_str(&format!("  RAM {:.0}%", ram * 100.0));

                let ram_text = ram_text.clone();
                let down_text = down_text.clone();
                let up_text = up_text.clone();
                let sent = ui.upgrade_in_event_loop(move |ui| {
                    ui.set_spec_cpu(cpu);
                    ui.set_spec_ram(ram);
                    ui.set_spec_ram_text(ram_text.into());
                    ui.set_spec_gpu(gpu);
                    ui.set_spec_down(down_text.into());
                    ui.set_spec_up(up_text.into());
                    ui.set_spec_batt(batt);
                    ui.set_spec_charging(charging);
                    ui.set_spec_batt_present(present);
                    ui.set_spec_text(text.into());
                });
                if sent.is_err() {
                    break;
                }
            }

            thread::sleep(Duration::from_millis(500));
        }
    });
}