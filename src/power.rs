use std::thread;
use std::time::Duration;

use amane::{Argument, Bus, Service, Value};

const TUNED: &str = "com.redhat.tuned";
const TUNED_PATH: &str = "/Tuned";
const TUNED_CONTROL: &str = "com.redhat.tuned.control";

const POWER_PROFILES: &str = "net.hadess.PowerProfiles";
const POWER_PROFILES_PATH: &str = "/net/hadess/PowerProfiles";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    Saver,
    Balanced,
    Performance,
}

impl Power {
    pub const ALL: [Power; 3] = [Power::Saver, Power::Balanced, Power::Performance];

    pub fn label(self) -> &'static str {
        match self {
            Power::Saver => "Экономный",
            Power::Balanced => "Обычный",
            Power::Performance => "Производительный",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Power::Saver => "меньше расход батареи",
            Power::Balanced => "баланс скорости и энергии",
            Power::Performance => "максимальная мощность",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Power::Saver => "\u{f032a}",       // leaf
            Power::Balanced => "\u{f04a2}",    // signal
            Power::Performance => "\u{f04c5}", // speedometer
        }
    }

    // tuned's own profile names
    fn tuned(self) -> &'static str {
        match self {
            Power::Saver => "powersave",
            Power::Balanced => "balanced",
            Power::Performance => "throughput-performance",
        }
    }

    // the names tuned-ppd uses for the same three modes
    fn power_profiles(self) -> &'static str {
        match self {
            Power::Saver => "power-saver",
            Power::Balanced => "balanced",
            Power::Performance => "performance",
        }
    }

    fn from_profile(profile: &str) -> Self {
        let profile = profile.to_ascii_lowercase();

        if profile.contains("power") {
            Power::Saver
        } else if profile.contains("performance")
            || profile.contains("throughput")
            || profile.contains("latency")
            || profile.contains("realtime")
        {
            Power::Performance
        } else {
            Power::Balanced
        }
    }
}

pub struct PowerMode {
    // the raw tuned profile, so changes made outside the bar are noticed too
    profile: String,
}

impl Service for PowerMode {
    fn new() -> Self {
        Self {
            profile: read_profile(),
        }
    }

    fn interval() -> Duration {
        Duration::from_secs(3)
    }

    fn update(&mut self) -> bool {
        let profile = read_profile();

        if profile == self.profile {
            return false;
        }

        self.profile = profile;

        true
    }
}

impl PowerMode {
    pub fn mode(&self) -> Power {
        Power::from_profile(&self.profile)
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub fn set(mode: Power) {
        // D-Bus waits for an answer, so it runs off the drawing thread
        thread::spawn(move || {
            let bus = Bus::system();

            let reply = bus.call(
                TUNED,
                TUNED_PATH,
                TUNED_CONTROL,
                "switch_profile",
                &[Argument::from(mode.tuned())],
            );

            // tuned is not there, so try the power-profiles front end instead
            if reply == Value::Nothing {
                bus.set_property(
                    POWER_PROFILES,
                    POWER_PROFILES_PATH,
                    POWER_PROFILES,
                    "ActiveProfile",
                    Argument::from(mode.power_profiles()),
                );
            }
        });
    }
}

fn read_profile() -> String {
    let bus = Bus::system();

    let active = bus.call(TUNED, TUNED_PATH, TUNED_CONTROL, "active_profile", &[]);
    let active = active.text().trim();

    if !active.is_empty() {
        return String::from(active);
    }

    String::from(
        bus.property(
            POWER_PROFILES,
            POWER_PROFILES_PATH,
            POWER_PROFILES,
            "ActiveProfile",
        )
        .text(),
    )
}
