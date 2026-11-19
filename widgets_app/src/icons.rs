//! Little pictures for the link widgets: website logos and program icons.
//! They are fetched once on a background thread and kept on disk.
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

const SCRIPT: &str = r##"
Add-Type -AssemblyName System.Drawing
$p = $args[0]
$out = $args[1]
try {
  $ico = [System.Drawing.Icon]::ExtractAssociatedIcon($p)
  $img = $ico.ToBitmap()
  $bmp = New-Object System.Drawing.Bitmap 96,96
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.DrawImage($img, 0, 0, 96, 96)
  $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose()
  $bmp.Dispose()
} catch {}
"##;

enum Job {
    Favicon(String),
    File(String),
}

fn dir() -> PathBuf {
    shared::Config::dir().join("widget_icons")
}

fn key(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.to_lowercase().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

pub struct Icons {
    map: HashMap<String, Option<slint::Image>>,
    asked: HashSet<String>,
    tx: mpsc::Sender<(String, Job)>,
    pub dirty: Arc<AtomicBool>,
}

impl Icons {
    pub fn new() -> Self {
        let d = dir();
        let _ = std::fs::create_dir_all(&d);
        let script = d.join("extract.ps1");
        let _ = std::fs::write(&script, SCRIPT);

        // forget earlier failures
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                if e.path().extension().map(|x| x == "fail").unwrap_or(false) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }

        let (tx, rx) = mpsc::channel::<(String, Job)>();
        let dirty = Arc::new(AtomicBool::new(false));
        let dirty2 = dirty.clone();
        std::thread::spawn(move || {
            for (name, job) in rx {
                let base = dir().join(&name);
                let png = base.with_extension("png");
                match job {
                    Job::Favicon(domain) => {
                        let url = format!("https://www.google.com/s2/favicons?domain={}&sz=128", domain);
                        let got = ureq::get(&url)
                            .set("User-Agent", "Mozilla/5.0 DynamicIsland")
                            .timeout(Duration::from_secs(8))
                            .call()
                            .ok()
                            .and_then(|r| {
                                let mut buf = Vec::new();
                                r.into_reader().take(2_000_000).read_to_end(&mut buf).ok()?;
                                image::load_from_memory(&buf).ok()
                            });
                        if let Some(img) = got {
                            let _ = img.to_rgba8().save(&png);
                        }
                    }
                    Job::File(path) => {
                        let mut c = Command::new("powershell");
                        c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
                            .arg(&script)
                            .arg(&path)
                            .arg(&png);
                        #[cfg(windows)]
                        {
                            use std::os::windows::process::CommandExt;
                            c.creation_flags(0x0800_0000);
                        }
                        let _ = c.status();
                    }
                }
                if !png.exists() {
                    let _ = std::fs::write(base.with_extension("fail"), "x");
                }
                dirty2.store(true, Ordering::SeqCst);
            }
        });

        Self { map: HashMap::new(), asked: HashSet::new(), tx, dirty }
    }

    fn lookup(&mut self, id: &str, make: impl FnOnce() -> Job) -> Option<slint::Image> {
        if let Some(v) = self.map.get(id) {
            return v.clone();
        }
        let name = key(id);
        let base = dir().join(&name);
        let png = base.with_extension("png");
        if png.exists() {
            let img = image::open(&png).ok().map(|i| {
                let rgba = i.to_rgba8();
                let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                    rgba.as_raw(),
                    rgba.width(),
                    rgba.height(),
                );
                slint::Image::from_rgba8(buf)
            });
            self.map.insert(id.to_string(), img.clone());
            return img;
        }
        if base.with_extension("fail").exists() {
            self.map.insert(id.to_string(), None);
        } else if self.asked.insert(id.to_string()) {
            let _ = self.tx.send((name, make()));
        }
        None
    }

    pub fn favicon(&mut self, domain: &str) -> Option<slint::Image> {
        let d = domain.to_string();
        self.lookup(&format!("fav|{}", domain), move || Job::Favicon(d))
    }

    pub fn file(&mut self, path: &str) -> Option<slint::Image> {
        let p = path.to_string();
        self.lookup(&format!("file|{}", path), move || Job::File(p))
    }

    /// Forget "not loaded yet" answers so new files get picked up.
    pub fn reload_missing(&mut self) {
        self.map.retain(|_, v| v.is_some());
    }
}
