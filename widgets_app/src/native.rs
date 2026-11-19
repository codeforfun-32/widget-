//! Everything that talks to Windows directly.
use std::ffi::c_void;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetClassNameW, GetCursorPos, GetDesktopWindow, GetForegroundWindow,
    GetAncestor, GetSystemMetrics, GetWindow, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetWindowLongPtrW, SetWindowPos,
    GWL_EXSTYLE, GWL_STYLE, GW_CHILD, GW_HWNDNEXT, GW_OWNER, SM_CXSCREEN, SM_CYSCREEN,
    WindowFromPoint, GA_ROOT, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
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

// ───────── our own windows ─────────

pub fn hwnd_of(window: &slint::Window) -> Option<HWND> {
    let handle = window.window_handle();
    let raw = handle.window_handle().ok()?.as_raw();
    match raw {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut c_void)),
        _ => None,
    }
}

/// No taskbar button, and not part of Alt+Tab.
pub fn sync_styles(hwnd: HWND) {
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let wanted = (current | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        if wanted != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}

/// The window just under the normal app windows: the desktop (wallpaper and icons).
fn desktop_anchor() -> Option<HWND> {
    unsafe {
        let mut cur = GetWindow(GetDesktopWindow(), GW_CHILD).ok();
        let mut guard = 0;
        while let Some(h) = cur {
            guard += 1;
            if guard > 4000 || h.0.is_null() {
                break;
            }
            cur = GetWindow(h, GW_HWNDNEXT).ok();
            let mut cls = [0u16; 64];
            let n = GetClassNameW(h, &mut cls);
            if n > 0 && String::from_utf16_lossy(&cls[..n as usize]) == "WorkerW" {
                if FindWindowExW(Some(h), None, w!("SHELLDLL_DefView"), PCWSTR::null()).is_ok() {
                    return Some(h);
                }
            }
        }
        FindWindowW(w!("Progman"), PCWSTR::null()).ok()
    }
}

/// Put the widget right above the desktop and below every app.
pub fn keep_behind(hwnd: HWND) {
    if let Some(anchor) = desktop_anchor() {
        unsafe {
            let _ = SetWindowPos(hwnd, Some(anchor), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

// ───────── screen, cursor ─────────

pub fn screen() -> (i32, i32) {
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

pub fn cursor() -> Option<(i32, i32)> {
    unsafe {
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_ok() { Some((p.x, p.y)) } else { None }
    }
}

/// The top-level window the mouse is over right now (0 = none).
pub fn root_under_cursor() -> isize {
    unsafe {
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_err() {
            return 0;
        }
        let h = WindowFromPoint(p);
        if h.0.is_null() {
            return 0;
        }
        GetAncestor(h, GA_ROOT).0 as isize
    }
}

pub fn foreground() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

// ───────── other windows ─────────

fn is_shell_or_me(h: HWND) -> bool {
    unsafe {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(h, Some(&mut pid as *mut u32));
        if pid == std::process::id() || pid == 0 {
            return true;
        }
        let mut cls = [0u16; 128];
        let n = GetClassNameW(h, &mut cls);
        if n > 0 {
            let c = String::from_utf16_lossy(&cls[..n as usize]);
            if SHELL_CLASSES.contains(&c.as_str()) {
                return true;
            }
        }
        false
    }
}

/// Rectangles (left, top, right, bottom) of the app windows that are open and not minimized.
pub fn app_windows() -> Vec<(i32, i32, i32, i32)> {
    let mut out = Vec::new();
    unsafe {
        let mut cur = GetWindow(GetDesktopWindow(), GW_CHILD).ok();
        let mut guard = 0;
        while let Some(h) = cur {
            guard += 1;
            if guard > 4000 || h.0.is_null() {
                break;
            }
            cur = GetWindow(h, GW_HWNDNEXT).ok();

            if !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() {
                continue;
            }
            if let Ok(owner) = GetWindow(h, GW_OWNER) {
                if !owner.0.is_null() {
                    continue;
                }
            }
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
            if ex & WS_EX_TOOLWINDOW.0 != 0 && ex & WS_EX_APPWINDOW.0 == 0 {
                continue;
            }
            if GetWindowTextLengthW(h) == 0 {
                continue;
            }
            let mut cloaked: u32 = 0;
            let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4);
            if cloaked != 0 {
                continue;
            }
            if is_shell_or_me(h) {
                continue;
            }
            let mut r = RECT::default();
            if GetWindowRect(h, &mut r).is_err() {
                continue;
            }
            if r.right - r.left < 40 || r.bottom - r.top < 40 {
                continue;
            }
            out.push((r.left, r.top, r.right, r.bottom));
        }
    }
    out
}

/// A fullscreen game or video in front (normal maximised windows keep their title bar).
pub fn fullscreen_app() -> bool {
    unsafe {
        let h = GetForegroundWindow();
        if h.0.is_null() || !IsWindowVisible(h).as_bool() {
            return false;
        }
        if is_shell_or_me(h) {
            return false;
        }
        let style = GetWindowLongPtrW(h, GWL_STYLE) as u32;
        if style & 0x00C0_0000 != 0 {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r).is_err() {
            return false;
        }
        let (sw, sh) = screen();
        r.left <= 0 && r.top <= 0 && r.right >= sw && r.bottom >= sh
    }
}

#[allow(dead_code)]
pub fn exe_of(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if ok { Some(String::from_utf16_lossy(&buf[..len as usize])) } else { None }
    }
}
