use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN};

/// Top-center of the primary monitor for a window of the given physical width.
pub fn top_center(window_width_px: i32) -> (i32, i32) {
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    (((screen_w - window_width_px) / 2).max(0), 0)
}