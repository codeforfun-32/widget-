use std::time::{Duration, Instant};

pub enum TimerEvent {
    None,
    /// New text to show, like "04:59".
    Tick(String),
    Finished,
}

/// The mascot's countdown task.
#[derive(Default)]
pub struct Countdown {
    end: Option<Instant>,
    last_shown: Option<u64>,
}

impl Countdown {
    pub fn start(&mut self, minutes: u64) {
        self.end = Some(Instant::now() + Duration::from_secs(minutes * 60));
        self.last_shown = None;
    }

    pub fn stop(&mut self) {
        self.end = None;
        self.last_shown = None;
    }

    pub fn poll(&mut self, now: Instant) -> TimerEvent {
        let Some(end) = self.end else { return TimerEvent::None };
        if now >= end {
            self.end = None;
            self.last_shown = None;
            return TimerEvent::Finished;
        }
        let secs = (end - now).as_secs() + 1;
        if self.last_shown == Some(secs) {
            return TimerEvent::None;
        }
        self.last_shown = Some(secs);
        TimerEvent::Tick(format!("{:02}:{:02}", secs / 60, secs % 60))
    }
}