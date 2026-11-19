//! Everything that talks to Windows directly.
use std::ffi::c_void;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, COLORREF, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP};
use windows::Win32::UI::Shell::{SHAppBarMessage, APPBARDATA};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetClassNameW, GetCursorPos, GetDesktopWindow, GetForegroundWindow, GetSystemMetrics,
    GetWindow, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, PostMessageW, SetForegroundWindow,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE,
    GW_CHILD, GW_HWNDNEXT, GW_OWNER, HWND_TOPMOST, LWA_ALPHA, SM_CXSCREEN, SM_CYSCREEN,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_MINIMIZE, SW_RESTORE,
    SW_SHOW, SW_SHOWNOACTIVATE, WM_CLOSE, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

const SHELL_CLASSES: [&str; 8] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "Windows.UI.Core.CoreWindow",
    "XamlExplorerHostIslandWindow",
];

// ───────── our own window ─────────

pub fn hwnd_of(window: &slint::Window) -> Option<HWND> {
    let handle = window.window_handle();
    let raw = handle.window_handle().ok()?.as_raw();
    match raw {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut c_void)),
        _ => None,
    }
}

pub fn place(hwnd: HWND, x: i32, y: i32) {
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// Keeps the bar on screen and above everything: brings it back if "Show desktop" / Win+D
/// minimised it, and puts it on top of other always-on-top windows.
pub fn keep_on_top(hwnd: HWND) {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// Registers the bar as a real Windows "app bar" so maximised windows stop above it,
/// exactly like the normal taskbar. `px` = height to reserve at the bottom (0 = give the space back).
pub fn reserve(hwnd: HWND, sw: i32, sh: i32, px: i32, registered: &mut bool) {
    unsafe {
        let mut d = APPBARDATA::default();
        d.cbSize = std::mem::size_of::<APPBARDATA>() as u32;
        d.hWnd = hwnd;
        if px <= 0 {
            if *registered {
                SHAppBarMessage(1, &mut d); // ABM_REMOVE
                *registered = false;
            }
            return;
        }
        if !*registered {
            d.uCallbackMessage = 0x0400 + 77;
            if SHAppBarMessage(0, &mut d) == 0 {
                return; // ABM_NEW failed
            }
            *registered = true;
        }
        d.uEdge = 3; // bottom
        d.rc = RECT { left: 0, top: sh - px, right: sw, bottom: sh };
        SHAppBarMessage(2, &mut d); // ABM_QUERYPOS
        d.rc.top = d.rc.bottom - px;
        SHAppBarMessage(3, &mut d); // ABM_SETPOS
    }
}

pub fn sync_styles(hwnd: HWND) {
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let wanted = current | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize;
        if wanted != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}

pub fn set_region(hwnd: HWND, left: i32, top: i32, right: i32, bottom: i32) {
    unsafe {
        let region = CreateRectRgn(left, top, right, bottom);
        let _ = SetWindowRgn(hwnd, Some(region), true);
    }
}

// ───────── screen, cursor, clock ─────────

pub fn screen() -> (i32, i32) {
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

pub fn cursor() -> Option<(i32, i32)> {
    unsafe {
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_ok() { Some((p.x, p.y)) } else { None }
    }
}

pub struct Clock {
    pub h: u16,
    pub m: u16,
    pub day: u16,
    pub month: u16,
    pub year: u16,
}

pub fn now() -> Clock {
    unsafe {
        let t = GetLocalTime();
        Clock { h: t.wHour, m: t.wMinute, day: t.wDay, month: t.wMonth, year: t.wYear }
    }
}

// ───────── windows list ─────────

#[derive(Clone)]
pub struct Win {
    pub hwnd: isize,
    pub exe: String,
}

fn exe_of(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if ok { Some(String::from_utf16_lossy(&buf[..len as usize])) } else { None }
    }
}

/// Windows that belong on a taskbar, top of the stack first.
pub fn list_windows() -> Vec<Win> {
    let mut out = Vec::new();
    let me = std::process::id();
    unsafe {
        let mut cur = GetWindow(GetDesktopWindow(), GW_CHILD).ok();
        let mut guard = 0;
        while let Some(h) = cur {
            guard += 1;
            if guard > 4000 || h.0.is_null() { break; }
            cur = GetWindow(h, GW_HWNDNEXT).ok();

            if !IsWindowVisible(h).as_bool() { continue; }
            if let Ok(owner) = GetWindow(h, GW_OWNER) {
                if !owner.0.is_null() { continue; }
            }
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
            if ex & WS_EX_TOOLWINDOW.0 != 0 && ex & WS_EX_APPWINDOW.0 == 0 { continue; }
            if GetWindowTextLengthW(h) == 0 { continue; }

            let mut cloaked: u32 = 0;
            let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4);
            if cloaked != 0 { continue; }

            let mut pid: u32 = 0;
            GetWindowThreadProcessId(h, Some(&mut pid as *mut u32));
            if pid == me || pid == 0 { continue; }

            let mut cls = [0u16; 128];
            let n = GetClassNameW(h, &mut cls);
            if n > 0 {
                let c = String::from_utf16_lossy(&cls[..n as usize]);
                if SHELL_CLASSES.contains(&c.as_str()) { continue; }
            }

            let Some(exe) = exe_of(pid) else { continue };
            out.push(Win { hwnd: h.0 as isize, exe });
        }
    }
    out
}

pub fn foreground() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

fn key(vk: u8, up: bool) {
    unsafe {
        keybd_event(vk, 0, if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, 0);
    }
}

pub fn activate(hwnd: isize) {
    unsafe {
        let h = HWND(hwnd as *mut c_void);
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        // a tiny Alt tap lets us take the foreground from a no-focus window
        key(0x12, false);
        key(0x12, true);
        let _ = SetForegroundWindow(h);
    }
}

pub fn minimize(hwnd: isize) {
    unsafe {
        let _ = ShowWindow(HWND(hwnd as *mut c_void), SW_MINIMIZE);
    }
}

pub fn close(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut c_void)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

/// Tap the Windows key (opens Start).
pub fn tap_start() {
    key(0x5B, false);
    key(0x5B, true);
}

/// Win + N (notification centre and calendar).
pub fn tap_notifications() {
    key(0x5B, false);
    key(0x4E, false);
    key(0x4E, true);
    key(0x5B, true);
}

/// Win + A (quick settings: Wi-Fi, sound, battery, brightness).
pub fn tap_quick_settings() {
    key(0x5B, false);
    key(0x41, false);
    key(0x41, true);
    key(0x5B, true);
}

pub fn fullscreen_app() -> bool {
    unsafe {
        let h = GetForegroundWindow();
        if h.0.is_null() || !IsWindowVisible(h).as_bool() { return false; }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(h, Some(&mut pid as *mut u32));
        if pid == std::process::id() { return false; }
        let mut cls = [0u16; 128];
        let n = GetClassNameW(h, &mut cls);
        if n > 0 {
            let c = String::from_utf16_lossy(&cls[..n as usize]);
            if SHELL_CLASSES.contains(&c.as_str()) { return false; }
        }
        // normal maximised windows have a title bar; fullscreen games and videos do not
        let style = GetWindowLongPtrW(h, GWL_STYLE) as u32;
        if style & 0x00C0_0000 != 0 { return false; }
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r).is_err() { return false; }
        let (sw, sh) = screen();
        r.left <= 0 && r.top <= 0 && r.right >= sw && r.bottom >= sh
    }
}

// ───────── the real Windows taskbar ─────────

unsafe fn appbar(msg: u32, lparam: isize) -> usize {
    let mut d = APPBARDATA::default();
    d.cbSize = std::mem::size_of::<APPBARDATA>() as u32;
    d.hWnd = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).unwrap_or_default();
    d.lParam = LPARAM(lparam);
    SHAppBarMessage(msg, &mut d)
}

/// Current taskbar state bits (1 = auto-hide, 2 = always on top).
pub fn native_state() -> isize {
    unsafe { appbar(4, 0) as isize }
}

pub fn set_native_state(state: isize) {
    unsafe {
        appbar(10, state);
    }
}

/// Show or hide the real Windows taskbar window(s).
pub fn set_native_visible(show: bool) {
    unsafe {
        for class in [w!("Shell_TrayWnd"), w!("Shell_SecondaryTrayWnd")] {
            if let Ok(h) = FindWindowW(class, PCWSTR::null()) {
                let _ = ShowWindow(h, if show { SW_SHOW } else { SW_HIDE });
            }
        }
    }
}

/// Make the real taskbar see-through (0) or normal (255).
pub fn set_native_alpha(alpha: u8) {
    unsafe {
        for class in [w!("Shell_TrayWnd"), w!("Shell_SecondaryTrayWnd")] {
            if let Ok(h) = FindWindowW(class, PCWSTR::null()) {
                let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
                if ex & (WS_EX_LAYERED.0 as isize) == 0 {
                    SetWindowLongPtrW(h, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as isize);
                }
                let _ = SetLayeredWindowAttributes(h, COLORREF(0), alpha, LWA_ALPHA);
            }
        }
    }
}

// ───────── Windows' hidden-icons box ─────────

/// Press Enter.
pub fn tap_enter() {
    key(0x0D, false);
    key(0x0D, true);
}

/// Press Escape.
pub fn tap_escape() {
    key(0x1B, false);
    key(0x1B, true);
}

/// Win + B (focus the notification area / hidden-icons arrow).
pub fn tap_win_b() {
    key(0x5B, false);
    key(0x42, false);
    key(0x42, true);
    key(0x5B, true);
}

/// True while Windows' hidden-icons box is really open on screen.
pub fn overflow_open() -> bool {
    unsafe {
        for class in [w!("TopLevelWindowForOverflowXamlIsland"), w!("NotifyIconOverflowWindow")] {
            if let Ok(h) = FindWindowW(class, PCWSTR::null()) {
                if !IsWindowVisible(h).as_bool() { continue; }
                let mut cloaked: u32 = 0;
                let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4);
                if cloaked != 0 { continue; }
                let mut r = RECT::default();
                if GetWindowRect(h, &mut r).is_ok() && (r.right - r.left) > 20 && (r.bottom - r.top) > 20 {
                    return true;
                }
            }
        }
        false
    }
}