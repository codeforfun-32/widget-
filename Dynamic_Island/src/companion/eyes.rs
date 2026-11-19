/// Smooth cursor tracking for the mascot's eyes.
pub struct Eyes {
    x: f32,
    y: f32,
}

impl Eyes {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    /// dx, dy = cursor offset from the mascot center in logical px.
    /// Returns Some((x, y)) in the range -1..1 only when the eyes actually moved.
    pub fn update(&mut self, dx: f32, dy: f32) -> Option<(f32, f32)> {
        let tx = (dx / 220.0).clamp(-1.0, 1.0);
        let ty = (dy / 220.0).clamp(-1.0, 1.0);

        let nx = if (tx - self.x).abs() < 0.004 { tx } else { self.x + (tx - self.x) * 0.2 };
        let ny = if (ty - self.y).abs() < 0.004 { ty } else { self.y + (ty - self.y) * 0.2 };

        let changed = nx != self.x || ny != self.y;
        self.x = nx;
        self.y = ny;
        if changed { Some((nx, ny)) } else { None }
    }
}