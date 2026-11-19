use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::thread;
use std::time::Duration;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Opens a link in the default browser (or app for spotify: links).
pub fn open_url(url: &str) {
    let allowed = url.starts_with("https://") || url.starts_with("http://") || url.starts_with("spotify:");
    if !allowed {
        return;
    }
    let _ = Command::new("rundll32")
        .arg("url.dll,FileProtocolHandler")
        .arg(url)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

/// Percent-encodes text for use inside a URL.
pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub fn http_get_string(url: &str) -> Option<String> {
    ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 DynamicIsland")
        .timeout(Duration::from_secs(8))
        .call()
        .ok()?
        .into_string()
        .ok()
}

pub fn http_get_bytes(url: &str, max: u64) -> Option<Vec<u8>> {
    let response = ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 DynamicIsland")
        .timeout(Duration::from_secs(8))
        .call()
        .ok()?;
    let mut buf = Vec::new();
    response.into_reader().take(max).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Finds the first YouTube video that matches a search text.
fn youtube_first_video(query: &str) -> Option<String> {
    let url = format!("https://www.youtube.com/results?search_query={}", url_encode(query));
    let body = ureq::get(&url)
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120 Safari/537.36")
        .set("Accept-Language", "en-US,en;q=0.9")
        .timeout(Duration::from_secs(8))
        .call()
        .ok()?
        .into_string()
        .ok()?;
    let marker = "\"videoId\":\"";
    let start = body.find(marker)? + marker.len();
    let id: String = body[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if id.len() == 11 { Some(id) } else { None }
}

/// Opens what is currently playing (best effort, Windows does not tell us the exact page).
pub fn open_now_playing(source: String, title: String, artist: String) {
    thread::spawn(move || {
        let query = if artist.is_empty() { title.clone() } else { format!("{title} {artist}") };
        if query.trim().is_empty() {
            return;
        }
        let s = source.to_lowercase();

        if s.contains("spotify") {
            open_url(&format!("spotify:search:{}", url_encode(&query)));
            return;
        }

        let browsers = ["chrome", "edge", "brave", "firefox", "opera", "vivaldi"];
        if browsers.iter().any(|b| s.contains(b)) {
            match youtube_first_video(&query) {
                Some(id) => open_url(&format!("https://www.youtube.com/watch?v={id}")),
                None => open_url(&format!(
                    "https://www.youtube.com/results?search_query={}",
                    url_encode(&query)
                )),
            }
        }
    });
}