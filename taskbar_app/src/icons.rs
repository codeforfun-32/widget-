//! App icons and display names. Pulled through PowerShell once per app, then cached on disk.
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

const SCRIPT: &str = r##"
Add-Type -AssemblyName System.Drawing
$p = $args[0]
$out = $args[1]

function Save-Scaled($img) {
  $bmp = New-Object System.Drawing.Bitmap 96,96
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.DrawImage($img, 0, 0, 96, 96)
  $bmp.Save($out + ".png", [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose()
  $bmp.Dispose()
}

$saved = $false

# Microsoft Store apps keep their logo next to the exe, described in AppxManifest.xml
try {
  if ($p -like '*\WindowsApps\*') {
    $root = Split-Path $p
    while ($root -and -not (Test-Path -LiteralPath (Join-Path $root 'AppxManifest.xml'))) { $root = Split-Path $root }
    if ($root) {
      [xml]$m = Get-Content -LiteralPath (Join-Path $root 'AppxManifest.xml')
      $ve = $m.SelectSingleNode("//*[local-name()='VisualElements']")
      $logo = $null
      if ($ve) {
        foreach ($a in 'Square44x44Logo','Square150x150Logo') {
          $v = $ve.GetAttribute($a)
          if ($v) { $logo = $v; break }
        }
      }
      if ($logo) {
        $lp = Join-Path $root $logo
        $dir = Split-Path $lp
        $base = [System.IO.Path]::GetFileNameWithoutExtension($lp)
        $c = Get-ChildItem -LiteralPath $dir -Filter ($base + '*.png') -ErrorAction SilentlyContinue |
             Where-Object { $_.Name -notmatch 'contrast|unplated' } |
             Sort-Object Length -Descending | Select-Object -First 1
        if ($c) {
          $img = [System.Drawing.Image]::FromFile($c.FullName)
          Save-Scaled $img
          $img.Dispose()
          $saved = $true
        }
      }
    }
  }
} catch {}

# normal apps: the icon inside the exe
try {
  if (-not $saved) {
    $ico = [System.Drawing.Icon]::ExtractAssociatedIcon($p)
    Save-Scaled ($ico.ToBitmap())
  }
} catch {}

try {
  $d = (Get-Item -LiteralPath $p).VersionInfo.FileDescription
  if ($d) { Set-Content -Path ($out + ".txt") -Value $d -Encoding UTF8 }
} catch {}
"##;

fn dir() -> PathBuf {
    shared::Config::dir().join("icons")
}

/// The "v2" salt makes old cached icons (made by the earlier, weaker method) get rebuilt once.
fn key(exe: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in format!("v2|{}", exe.to_lowercase()).bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

pub struct Icons {
    map: HashMap<String, Option<slint::Image>>,
    names: HashMap<String, String>,
    asked: HashSet<String>,
    tx: mpsc::Sender<String>,
    pub dirty: Arc<AtomicBool>,
}

impl Icons {
    pub fn new() -> Self {
        let d = dir();
        let _ = std::fs::create_dir_all(&d);

        // forget earlier failures so they get another try
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                if e.path().extension().map(|x| x == "fail").unwrap_or(false) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }

        let script = d.join("extract.ps1");
        let _ = std::fs::write(&script, SCRIPT);

        let (tx, rx) = mpsc::channel::<String>();
        let dirty = Arc::new(AtomicBool::new(false));
        let dirty2 = dirty.clone();
        std::thread::spawn(move || {
            for exe in rx {
                let base = dir().join(key(&exe));
                let mut c = Command::new("powershell");
                c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
                    .arg(&script)
                    .arg(&exe)
                    .arg(&base);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    c.creation_flags(0x0800_0000);
                }
                let _ = c.status();
                if !base.with_extension("png").exists() {
                    let _ = std::fs::write(base.with_extension("fail"), "x");
                }
                dirty2.store(true, Ordering::SeqCst);
            }
        });

        Self { map: HashMap::new(), names: HashMap::new(), asked: HashSet::new(), tx, dirty }
    }

    /// (icon, display name) for an exe; both None until the icon has been extracted.
    pub fn get(&mut self, exe: &str) -> (Option<slint::Image>, Option<String>) {
        if let Some(v) = self.map.get(exe) {
            return (v.clone(), self.names.get(exe).cloned());
        }
        let base = dir().join(key(exe));
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
            if let Ok(name) = std::fs::read_to_string(base.with_extension("txt")) {
                let name = name.trim().to_string();
                if !name.is_empty() { self.names.insert(exe.to_string(), name); }
            }
            self.map.insert(exe.to_string(), img.clone());
            return (img, self.names.get(exe).cloned());
        }
        if base.with_extension("fail").exists() {
            self.map.insert(exe.to_string(), None);
        } else if self.asked.insert(exe.to_string()) {
            let _ = self.tx.send(exe.to_string());
        }
        (None, None)
    }

    /// Forget "not loaded yet" answers so new files get picked up.
    pub fn reload_missing(&mut self) {
        self.map.retain(|_, v| v.is_some());
    }
}