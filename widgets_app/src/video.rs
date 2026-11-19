//! MP4 / video playback through Windows Media Foundation (built in to Windows, nothing to install).
//! Frames are decoded on a background thread and the newest one is picked up by the widget.
//! Videos are silent. They loop forever.
//!
//! If this file ever fails to build, open widgets_app/Cargo.toml and change
//!     default = ["video"]   to   default = []
//! Everything else keeps working; videos will just show a message.

pub struct Frame {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

#[cfg(feature = "video")]
pub use real::Video;

#[cfg(not(feature = "video"))]
pub use stub::Video;

#[cfg(not(feature = "video"))]
mod stub {
    use super::Frame;
    pub struct Video;
    impl Video {
        pub fn open(_path: &str, _flip: bool) -> Result<Video, String> {
            Err("Video support is switched off in this build. Use a GIF or an image".to_string())
        }
        pub fn set_paused(&self, _p: bool) {}
        pub fn take_frame(&self) -> Option<Frame> { None }
        pub fn error(&self) -> Option<String> { None }
    }
}

#[cfg(feature = "video")]
mod real {
    use super::Frame;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use windows::core::PCWSTR;
    use windows::Win32::Media::MediaFoundation::{
        IMFAttributes, IMFSample, MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromURL,
        MFMediaType_Video, MFStartup, MFVideoFormat_RGB32, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE,
        MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
    };
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    const FIRST_VIDEO_STREAM: u32 = 0xFFFF_FFFC;
    const MF_VERSION: u32 = 0x0002_0070;
    const FLAG_END_OF_STREAM: u32 = 0x2;

    struct Shared {
        frame: Mutex<Option<Frame>>,
        error: Mutex<Option<String>>,
        paused: AtomicBool,
        stop: AtomicBool,
    }

    pub struct Video {
        shared: Arc<Shared>,
    }

    impl Video {
        pub fn open(path: &str, flip: bool) -> Result<Video, String> {
            let shared = Arc::new(Shared {
                frame: Mutex::new(None),
                error: Mutex::new(None),
                paused: AtomicBool::new(false),
                stop: AtomicBool::new(false),
            });
            let s2 = shared.clone();
            let path = path.to_string();
            std::thread::spawn(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                }
                loop {
                    if s2.stop.load(Ordering::Relaxed) {
                        break;
                    }
                    match play_once(&path, flip, &s2) {
                        Ok(()) => continue, // reached the end: start again
                        Err(e) => {
                            if let Ok(mut g) = s2.error.lock() {
                                *g = Some(format!("Could not play this video: {e}. Try an H.264 MP4"));
                            }
                            break;
                        }
                    }
                }
            });
            Ok(Video { shared })
        }

        pub fn set_paused(&self, p: bool) {
            self.shared.paused.store(p, Ordering::Relaxed);
        }

        pub fn take_frame(&self) -> Option<Frame> {
            self.shared.frame.lock().ok().and_then(|mut g| g.take())
        }

        pub fn error(&self) -> Option<String> {
            self.shared.error.lock().ok().and_then(|g| g.clone())
        }
    }

    impl Drop for Video {
        fn drop(&mut self) {
            self.shared.stop.store(true, Ordering::Relaxed);
        }
    }

    fn play_once(path: &str, flip: bool, shared: &Shared) -> Result<(), String> {
        unsafe {
            MFStartup(MF_VERSION, 0).map_err(|e| format!("{e}"))?;

            let mut attrs: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attrs, 2).map_err(|e| format!("{e}"))?;
            let attrs = attrs.ok_or_else(|| "no attributes".to_string())?;
            attrs
                .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
                .map_err(|e| format!("{e}"))?;

            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let reader = MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), Some(&attrs))
                .map_err(|e| format!("{e}"))?;

            // ask for plain 32-bit pixels
            let mt = MFCreateMediaType().map_err(|e| format!("{e}"))?;
            mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| format!("{e}"))?;
            mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).map_err(|e| format!("{e}"))?;
            reader
                .SetCurrentMediaType(FIRST_VIDEO_STREAM, None, &mt)
                .map_err(|e| format!("{e}"))?;

            let cur = reader.GetCurrentMediaType(FIRST_VIDEO_STREAM).map_err(|e| format!("{e}"))?;
            let packed = cur.GetUINT64(&MF_MT_FRAME_SIZE).map_err(|e| format!("{e}"))?;
            let w = (packed >> 32) as usize;
            let h = (packed & 0xFFFF_FFFF) as usize;
            if w == 0 || h == 0 {
                return Err("empty video".to_string());
            }
            // a positive stride means the picture is stored top to bottom; otherwise bottom to top
            let top_down = match cur.GetUINT32(&MF_MT_DEFAULT_STRIDE) {
                Ok(v) => (v as i32) > 0,
                Err(_) => false,
            };
            let flipped = top_down == flip; // flip = user asked to turn it upside down

            // shrink big videos while copying (a widget does not need 4K)
            let step = (w.max(h) + 959) / 960;
            let step = step.max(1);
            let (ow, oh) = (w / step, h / step);

            let mut base: Option<(Instant, i64)> = None;
            let mut was_paused = false;

            loop {
                if shared.stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                if shared.paused.load(Ordering::Relaxed) {
                    was_paused = true;
                    std::thread::sleep(Duration::from_millis(120));
                    continue;
                }

                let mut flags: u32 = 0;
                let mut ts: i64 = 0;
                let mut sample: Option<IMFSample> = None;
                reader
                    .ReadSample(FIRST_VIDEO_STREAM, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample))
                    .map_err(|e| format!("{e}"))?;
                if flags & FLAG_END_OF_STREAM != 0 {
                    return Ok(());
                }
                let Some(sample) = sample else { continue };

                // play at the video's own speed
                if base.is_none() || was_paused {
                    base = Some((Instant::now(), ts));
                    was_paused = false;
                }
                if let Some((t0, ts0)) = base {
                    let due = t0 + Duration::from_nanos(((ts - ts0).max(0) as u64) * 100);
                    let now = Instant::now();
                    if due > now {
                        std::thread::sleep(due - now);
                    }
                }

                let buffer = sample.ConvertToContiguousBuffer().map_err(|e| format!("{e}"))?;
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut len: u32 = 0;
                buffer.Lock(&mut data, None, Some(&mut len)).map_err(|e| format!("{e}"))?;
                if !data.is_null() && (len as usize) >= w * h * 4 {
                    let src = std::slice::from_raw_parts(data, len as usize);
                    let mut out = vec![0u8; ow * oh * 4];
                    for oy in 0..oh {
                        let sy = oy * step;
                        let row = if flipped { h - 1 - sy } else { sy };
                        let src_row = row * w * 4;
                        let dst_row = oy * ow * 4;
                        for ox in 0..ow {
                            let s = src_row + ox * step * 4;
                            let d = dst_row + ox * 4;
                            // Windows gives B G R x, we need R G B A
                            out[d] = src[s + 2];
                            out[d + 1] = src[s + 1];
                            out[d + 2] = src[s];
                            out[d + 3] = 255;
                        }
                    }
                    if let Ok(mut g) = shared.frame.lock() {
                        *g = Some(Frame { w: ow as u32, h: oh as u32, rgba: out });
                    }
                }
                let _ = buffer.Unlock();
            }
        }
    }
}
