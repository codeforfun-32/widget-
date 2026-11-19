use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Weak};
use windows::Devices::Radios::{Radio, RadioKind, RadioState};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

use crate::{MainWindow, QuickState};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub enum Cmd {
    Volume(f32),
    Brightness(f32),
    Wifi(bool),
    Bluetooth(bool),
    Airplane(bool),
    Refresh,
    Poll,
}

static TX: OnceLock<Sender<Cmd>> = OnceLock::new();
static WANTED: AtomicBool = AtomicBool::new(false);

pub fn send(cmd: Cmd) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(cmd);
    }
}

pub fn set_wanted(on: bool) {
    let was = WANTED.swap(on, Ordering::Relaxed);
    if on && !was {
        send(Cmd::Refresh);
    }
}

pub fn open_setting(uri: &str) {
    if !(uri.starts_with("ms-settings:") || uri.starts_with("ms-settings-")) {
        return;
    }
    let _ = Command::new("cmd")
        .args(["/C", "start", "", uri])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

fn radios() -> Vec<Radio> {
    let mut out = Vec::new();
    let Ok(op) = Radio::GetRadiosAsync() else { return out };
    let Ok(list) = op.join() else { return out };
    let size = list.Size().unwrap_or(0);
    for i in 0..size {
        if let Ok(radio) = list.GetAt(i) {
            out.push(radio);
        }
    }
    out
}

fn is_on(radio: &Radio) -> bool {
    radio.State().map(|s| s == RadioState::On).unwrap_or(false)
}

fn set_state(radio: &Radio, on: bool) {
    let state = if on { RadioState::On } else { RadioState::Off };
    if let Ok(op) = radio.SetStateAsync(state) {
        let _ = op.join();
    }
}

fn set_kind(kind: RadioKind, on: bool) {
    for radio in radios() {
        if radio.Kind().ok() == Some(kind) {
            set_state(&radio, on);
        }
    }
}

fn set_airplane(on: bool) {
    for radio in radios() {
        set_state(&radio, !on);
    }
}

fn read_radios() -> (bool, bool, bool) {
    let list = radios();
    let wifi = list.iter().any(|r| r.Kind().ok() == Some(RadioKind::WiFi) && is_on(r));
    let bluetooth = list.iter().any(|r| r.Kind().ok() == Some(RadioKind::Bluetooth) && is_on(r));
    let airplane = !list.is_empty() && !list.iter().any(|r| is_on(r));
    (wifi, bluetooth, airplane)
}

fn endpoint() -> Option<IAudioEndpointVolume> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?;
        device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()
    }
}

fn get_volume() -> Option<f32> {
    unsafe { endpoint()?.GetMasterVolumeLevelScalar().ok() }
}

fn set_volume(level: f32) {
    unsafe {
        if let Some(ep) = endpoint() {
            let _ = ep.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null());
        }
    }
}

fn powershell(script: &str) -> Option<String> {
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn get_brightness() -> Option<f32> {
    let text = powershell(
        "(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness -ErrorAction Stop | Select-Object -First 1).CurrentBrightness",
    )?;
    let value: f32 = text.lines().next()?.trim().parse().ok()?;
    Some((value / 100.0).clamp(0.0, 1.0))
}

fn set_brightness(level: f32) {
    let percent = (level.clamp(0.0, 1.0) * 100.0).round() as u32;
    let script = format!(
        "Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods | Select-Object -First 1 | Invoke-CimMethod -MethodName WmiSetBrightness -Arguments @{{Timeout=1; Brightness={percent}}} | Out-Null"
    );
    let _ = powershell(&script);
}

pub fn spawn(ui: Weak<MainWindow>) {
    let (tx, rx) = mpsc::channel::<Cmd>();
    let _ = TX.set(tx);

    thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let _ = Radio::RequestAccessAsync().and_then(|op| op.join());

        let mut hold_until = Instant::now();

        loop {
            let mut cmds: Vec<Cmd> = Vec::new();
            match rx.recv_timeout(Duration::from_secs(3)) {
                Ok(c) => cmds.push(c),
                Err(RecvTimeoutError::Timeout) => {
                    if WANTED.load(Ordering::Relaxed) {
                        cmds.push(Cmd::Poll);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
            cmds.extend(rx.try_iter());

            let mut volume = None;
            let mut brightness = None;
            let mut refresh = false;
            let mut read_state = false;

            for cmd in cmds {
                match cmd {
                    Cmd::Volume(v) => volume = Some(v),
                    Cmd::Brightness(b) => brightness = Some(b),
                    Cmd::Wifi(on) => {
                        set_kind(RadioKind::WiFi, on);
                        read_state = true;
                    }
                    Cmd::Bluetooth(on) => {
                        set_kind(RadioKind::Bluetooth, on);
                        read_state = true;
                    }
                    Cmd::Airplane(on) => {
                        set_airplane(on);
                        read_state = true;
                    }
                    Cmd::Refresh => {
                        refresh = true;
                        read_state = true;
                    }
                    Cmd::Poll => read_state = true,
                }
            }

            if let Some(v) = volume {
                set_volume(v);
                hold_until = Instant::now() + Duration::from_millis(1500);
            }
            if let Some(b) = brightness {
                set_brightness(b);
                hold_until = Instant::now() + Duration::from_millis(1500);
            }

            if !read_state {
                continue;
            }

            let (wifi, bluetooth, airplane) = read_radios();
            let vol = get_volume();
            let bright = if refresh { Some(get_brightness()) } else { None };
            let holding = Instant::now() < hold_until;

            let sent = ui.upgrade_in_event_loop(move |ui| {
                let q = ui.global::<QuickState>();
                q.set_wifi_on(wifi);
                q.set_bt_on(bluetooth);
                q.set_airplane_on(airplane);
                if !holding {
                    if let Some(v) = vol {
                        q.set_volume(v);
                    }
                }
                if let Some(b) = bright {
                    match b {
                        Some(v) => {
                            q.set_brightness_ok(true);
                            if !holding {
                                q.set_brightness(v);
                            }
                        }
                        None => q.set_brightness_ok(false),
                    }
                }
            });
            if sent.is_err() {
                break;
            }
        }
    });
}