use std::time::{Duration, Instant};

use amane::{output, Service};

pub struct Clock {
    // minutes since midnight at the last sync
    base: u32,

    started: Instant,
    synced: Instant,

    hours: String,
    minutes: String,
}

impl Service for Clock {
    fn new() -> Self {
        let (base, seconds) = read_time();
        let now = Instant::now();

        let (hours, minutes) = split(base);

        Self {
            base,
            // shift the anchor back by the seconds already elapsed in this
            // minute, so the display flips exactly on the minute boundary
            started: now - Duration::from_secs(seconds as u64),
            synced: now,
            hours,
            minutes,
        }
    }

    fn interval() -> Duration {
        Duration::from_secs(1)
    }

    fn update(&mut self) -> bool {
        /*
         * counting minutes from the last sync avoids running `date` every
         * second; a resync every 15 minutes catches timezone and clock changes
         */
        if self.synced.elapsed() >= Duration::from_secs(15 * 60) {
            let (base, seconds) = read_time();
            let now = Instant::now();

            self.base = base;
            self.started = now - Duration::from_secs(seconds as u64);
            self.synced = now;
        }

        let passed = self.started.elapsed().as_secs() / 60;
        let minute = (self.base + passed as u32) % (24 * 60);
        let (hours, minutes) = split(minute);

        if hours == self.hours && minutes == self.minutes {
            return false;
        }

        self.hours = hours;
        self.minutes = minutes;

        true
    }
}

impl Clock {
    pub fn hours(&self) -> &str {
        &self.hours
    }

    pub fn minutes(&self) -> &str {
        &self.minutes
    }
}

// the bar shows the two halves stacked, like a little flip clock
fn split(minute_of_day: u32) -> (String, String) {
    (
        format!("{:02}", minute_of_day / 60),
        format!("{:02}", minute_of_day % 60),
    )
}

fn read_time() -> (u32, u32) {
    let text = output("date +%H:%M:%S");

    let mut parts = text.trim().split(':');

    let hours = parts.next().and_then(|part| part.parse::<u32>().ok());
    let minutes = parts.next().and_then(|part| part.parse::<u32>().ok());
    let seconds = parts.next().and_then(|part| part.parse::<u32>().ok());

    let (hours, minutes, seconds) = match (hours, minutes, seconds) {
        (Some(hours), Some(minutes), Some(seconds))
            if hours < 24 && minutes < 60 && seconds < 60 =>
        {
            (hours, minutes, seconds)
        }
        _ => (0, 0, 0),
    };

    ((hours * 60 + minutes) % (24 * 60), seconds)
}
