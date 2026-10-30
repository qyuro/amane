use std::time::{Duration, Instant};

use amane::{Animation, Easing, Network, Service, TextInput};

/// How long the full bar waits without input before it folds back to the ovals.
const IDLE_TIMEOUT: Duration = Duration::from_secs(15);

/// Which popup is open, how the bar looks, and what the Wi-Fi panel is asking a password for.
pub struct Ui {
    pub wifi_open: bool,
    pub power_open: bool,

    /// the bar keeps only the workspace ovals
    pub bar_minimal: bool,

    pub launcher_open: bool,
    pub launcher_query: String,
    pub launcher_selected: usize,

    pub selected: String,

    /// 1 at the full bar, 0 when only the ovals are left
    pub bar: Animation<f32>,

    /// 1 while each overlay is open, 0 once it has faded away
    pub launcher: Animation<f32>,
    pub wifi: Animation<f32>,
    pub power: Animation<f32>,

    /// the last click, key or toggle, so the bar can fold itself away
    pub last_activity: Instant,
}

impl Service for Ui {
    fn new() -> Self {
        let overlay = || {
            Animation::new(0.0)
                .duration(Duration::from_millis(190))
                .easing(Easing::Out)
        };

        Self {
            wifi_open: false,
            power_open: false,
            bar_minimal: false,
            launcher_open: false,
            launcher_query: String::new(),
            launcher_selected: 0,
            selected: String::new(),

            bar: Animation::new(1.0)
                .duration(Duration::from_millis(260))
                .easing(Easing::InOut),

            launcher: overlay(),
            wifi: overlay(),
            power: overlay(),

            last_activity: Instant::now(),
        }
    }

    // nothing changes on its own, only through clicks and ipc calls
    fn interval() -> Duration {
        Duration::from_secs(3600)
    }
}

/// Folds the bar back to its ovals after a while without any input.
pub struct Idle;

impl Service for Idle {
    fn new() -> Self {
        Self
    }

    fn interval() -> Duration {
        Duration::from_secs(1)
    }

    fn update(&mut self) -> bool {
        let collapse = {
            let ui = Ui::read();

            !ui.bar_minimal
                && !ui.wifi_open
                && !ui.power_open
                && !ui.launcher_open
                && ui.last_activity.elapsed() >= IDLE_TIMEOUT
        };

        if collapse {
            collapse_bar();
        }

        collapse
    }
}

/// Anything the pointer or a key did to the bar postpones folding it away.
pub fn touch() {
    Ui::write().last_activity = Instant::now();
}

pub fn toggle_wifi() {
    let open = {
        let mut ui = Ui::write();

        ui.wifi_open = !ui.wifi_open;
        let wifi = if ui.wifi_open { 1.0 } else { 0.0 };

        ui.wifi.to(wifi);

        ui.power_open = false;
        ui.power.to(0.0);

        ui.launcher_open = false;
        ui.launcher.to(0.0);

        ui.last_activity = Instant::now();

        if !ui.wifi_open {
            ui.selected.clear();
        }

        ui.wifi_open
    };

    if open {
        Network::scan();
    } else {
        forget_password();
    }
}

pub fn toggle_power() {
    let mut ui = Ui::write();

    ui.power_open = !ui.power_open;
    let power = if ui.power_open { 1.0 } else { 0.0 };

    ui.power.to(power);

    ui.wifi_open = false;
    ui.wifi.to(0.0);

    ui.launcher_open = false;
    ui.launcher.to(0.0);

    ui.selected.clear();
    ui.last_activity = Instant::now();

    drop(ui);

    forget_password();
}

pub fn close_wifi() {
    let mut ui = Ui::write();

    ui.wifi_open = false;
    ui.wifi.to(0.0);
    ui.selected.clear();
    ui.last_activity = Instant::now();

    drop(ui);

    forget_password();
}

pub fn close_power() {
    let mut ui = Ui::write();

    ui.power_open = false;
    ui.power.to(0.0);
    ui.last_activity = Instant::now();
}

// called from `amane ipc call toggle-bar`, which niri binds to Mod+O
pub fn toggle_bar() {
    let minimal = {
        let mut ui = Ui::write();

        ui.bar_minimal = !ui.bar_minimal;
        let bar = if ui.bar_minimal { 0.0 } else { 1.0 };

        ui.bar.to(bar);
        ui.last_activity = Instant::now();

        if ui.bar_minimal {
            ui.wifi_open = false;
            ui.wifi.to(0.0);

            ui.power_open = false;
            ui.power.to(0.0);

            ui.launcher_open = false;
            ui.launcher.to(0.0);

            ui.selected.clear();
        }

        ui.bar_minimal
    };

    if minimal {
        forget_password();
    }
}

// the timer's way of going minimal, without counting as activity
fn collapse_bar() {
    let mut ui = Ui::write();

    ui.bar_minimal = true;
    ui.bar.to(0.0);

    ui.wifi_open = false;
    ui.wifi.to(0.0);

    ui.power_open = false;
    ui.power.to(0.0);

    ui.launcher_open = false;
    ui.launcher.to(0.0);

    ui.selected.clear();
    ui.launcher_selected = 0;

    drop(ui);

    forget_password();
}

// called from `amane ipc call toggle-launcher`, which niri binds to Mod+D
pub fn toggle_launcher() {
    let opening = {
        let mut ui = Ui::write();

        let opening = !ui.launcher_open;
        ui.launcher_open = opening;
        ui.launcher.to(if opening { 1.0 } else { 0.0 });
        ui.last_activity = Instant::now();

        if opening {
            ui.launcher_query.clear();
            ui.launcher_selected = 0;

            ui.wifi_open = false;
            ui.wifi.to(0.0);

            ui.power_open = false;
            ui.power.to(0.0);

            ui.selected.clear();
        }

        opening
    };

    if opening {
        forget_password();
        TextInput::set_text("launcher", "");
    }
}

/// Closes the launcher; the query stays on screen while it fades out and is
/// cleared the next time it opens.
pub fn close_launcher() {
    let mut ui = Ui::write();

    ui.launcher_open = false;
    ui.launcher.to(0.0);
    ui.last_activity = Instant::now();
}

pub fn forget_password() {
    TextInput::set_text("wifi-password", "");
}
