use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood {
    Idle,
    Happy,
    Dizzy,
}

impl Mood {
    pub fn as_int(self) -> i32 {
        match self {
            Mood::Idle => 0,
            Mood::Happy => 1,
            Mood::Dizzy => 2,
        }
    }

    /// Name of the sound effect for this mood ("" = silent).
    pub fn sfx(self) -> &'static str {
        match self {
            Mood::Idle => "",
            Mood::Happy => "happy",
            Mood::Dizzy => "dizzy",
        }
    }
}

const HAPPY_FOR: Duration = Duration::from_millis(1200);
const DIZZY_FOR: Duration = Duration::from_millis(2200);
const POKE_WINDOW: Duration = Duration::from_millis(1800);
const DIZZY_POKES: usize = 5;

/// Mascot brain: poke it once or twice and it is happy, poke it a lot and it gets dizzy.
pub struct Companion {
    mood: Mood,
    mood_until: Option<Instant>,
    pokes: Vec<Instant>,
}

impl Companion {
    pub fn new() -> Self {
        Self { mood: Mood::Idle, mood_until: None, pokes: Vec::new() }
    }

    pub fn poke(&mut self, now: Instant) -> Mood {
        self.pokes.retain(|t| now.duration_since(*t) < POKE_WINDOW);
        self.pokes.push(now);
        if self.pokes.len() >= DIZZY_POKES {
            self.pokes.clear();
            self.set(Mood::Dizzy, DIZZY_FOR, now)
        } else {
            self.set(Mood::Happy, HAPPY_FOR, now)
        }
    }

    /// Celebrate (used when the timer finishes).
    pub fn cheer(&mut self, now: Instant) -> Mood {
        self.set(Mood::Happy, Duration::from_millis(2500), now)
    }

    /// Call often; returns the current mood and ends moods that ran out.
    pub fn tick(&mut self, now: Instant) -> Mood {
        if let Some(until) = self.mood_until {
            if now >= until {
                self.mood = Mood::Idle;
                self.mood_until = None;
            }
        }
        self.mood
    }

    fn set(&mut self, mood: Mood, duration: Duration, now: Instant) -> Mood {
        self.mood = mood;
        self.mood_until = Some(now + duration);
        mood
    }
}

const COLLAPSE_DELAY: Duration = Duration::from_millis(350);

/// Expands right away on hover, collapses only after the cursor has been away for a moment.
pub struct HoverTracker {
    expanded: bool,
    outside_since: Option<Instant>,
}

impl HoverTracker {
    pub fn new() -> Self {
        Self { expanded: false, outside_since: None }
    }

    pub fn update(&mut self, inside: bool, now: Instant) -> bool {
        if inside {
            self.expanded = true;
            self.outside_since = None;
        } else if self.expanded {
            let since = *self.outside_since.get_or_insert(now);
            if now.duration_since(since) >= COLLAPSE_DELAY {
                self.expanded = false;
                self.outside_since = None;
            }
        }
        self.expanded
    }
}