//! Pictures, GIFs and videos for the media widget.
use std::time::{Duration, Instant};

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

pub enum Player {
    Still,
    Gif(Gif),
    Video(crate::video::Video),
}

pub struct Gif {
    frames: Vec<(SharedPixelBuffer<Rgba8Pixel>, u64)>,
    idx: usize,
    next: Instant,
}

fn to_image(img: &image::RgbaImage) -> Image {
    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(img.as_raw(), img.width(), img.height());
    Image::from_rgba8(buf)
}

fn ext_of(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn is_video(path: &str) -> bool {
    matches!(ext_of(path).as_str(), "mp4" | "m4v" | "mov" | "wmv" | "avi" | "mkv" | "webm")
}

/// Opens a file. Gives back the player and the first picture to show.
pub fn load(path: &str, flip: bool) -> Result<(Player, Option<Image>), String> {
    if path.trim().is_empty() {
        return Err("Choose a picture, GIF or MP4 in settings".to_string());
    }
    if !std::path::Path::new(path).exists() {
        return Err("File not found. Check the path in settings".to_string());
    }
    if is_video(path) {
        let v = crate::video::Video::open(path, flip)?;
        return Ok((Player::Video(v), None));
    }
    if ext_of(path) == "gif" {
        if let Ok(g) = load_gif(path) {
            let first = Image::from_rgba8(g.frames[0].0.clone());
            return Ok((Player::Gif(g), Some(first)));
        }
    }
    let img = image::open(path).map_err(|e| format!("Could not open this file: {e}"))?;
    let img = if img.width() > 1920 || img.height() > 1920 { img.resize(1920, 1920, image::imageops::FilterType::Triangle) } else { img };
    Ok((Player::Still, Some(to_image(&img.to_rgba8()))))
}

fn load_gif(path: &str) -> Result<Gif, String> {
    use image::AnimationDecoder;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let dec = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;
    let mut frames = Vec::new();
    for frame in dec.into_frames() {
        let frame = frame.map_err(|e| e.to_string())?;
        let (n, d) = frame.delay().numer_denom_ms();
        let ms = if d == 0 { 100 } else { (n / d) as u64 };
        let ms = if ms < 20 { 100 } else { ms };
        let mut buf = frame.into_buffer();
        let (w, h) = buf.dimensions();
        if w.max(h) > 720 {
            let k = 720.0 / w.max(h) as f32;
            let nw = ((w as f32 * k) as u32).max(1);
            let nh = ((h as f32 * k) as u32).max(1);
            buf = image::imageops::resize(&buf, nw, nh, image::imageops::FilterType::Triangle);
        }
        frames.push((
            SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(buf.as_raw(), buf.width(), buf.height()),
            ms,
        ));
        if frames.len() >= 400 {
            break;
        }
    }
    if frames.is_empty() {
        return Err("empty gif".to_string());
    }
    Ok(Gif { frames, idx: 0, next: Instant::now() })
}

impl Player {
    /// Called often. Returns a new picture when the frame changed.
    pub fn tick(&mut self, paused: bool) -> Option<Image> {
        match self {
            Player::Still => None,
            Player::Gif(g) => {
                let now = Instant::now();
                if paused {
                    g.next = now + Duration::from_millis(g.frames[g.idx].1);
                    return None;
                }
                if now >= g.next {
                    g.idx = (g.idx + 1) % g.frames.len();
                    g.next = now + Duration::from_millis(g.frames[g.idx].1);
                    return Some(Image::from_rgba8(g.frames[g.idx].0.clone()));
                }
                None
            }
            Player::Video(v) => {
                v.set_paused(paused);
                v.take_frame().map(|f| {
                    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&f.rgba, f.w, f.h);
                    Image::from_rgba8(buf)
                })
            }
        }
    }

    pub fn error(&self) -> Option<String> {
        match self {
            Player::Video(v) => v.error(),
            _ => None,
        }
    }
}
