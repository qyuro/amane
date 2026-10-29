use std::time::Duration;

use amane::{Network, Service, TextInput};

/// Which popup is open, and what the Wi-Fi panel is asking a password for.
pub struct Ui {
    pub wifi_open: bool,
    pub power_open: bool,
    pub bar_hidden: bool,
    pub selected: String,
}

impl Service for Ui {
    fn new() -> Self {
        Self {
            wifi_open: false,
            power_open: false,
            bar_hidden: false,
            selected: String::new(),
        }
    }

    // nothing changes on its own, only through clicks
    fn interval() -> Duration {
        Duration::from_secs(3600)
    }
}

pub fn toggle_wifi() {
    let open = {
        let mut ui = Ui::write();

        ui.wifi_open = !ui.wifi_open;
        ui.power_open = false;

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
    ui.wifi_open = false;
    ui.selected.clear();
}

pub fn close_wifi() {
    let mut ui = Ui::write();

    ui.wifi_open = false;
    ui.selected.clear();

    drop(ui);

    forget_password();
}

pub fn close_power() {
    Ui::write().power_open = false;
}

// called from `amane ipc call toggle-bar`, which niri binds to Mod+O
pub fn toggle_bar() {
    let hidden = {
        let mut ui = Ui::write();

        ui.bar_hidden = !ui.bar_hidden;

        if ui.bar_hidden {
            ui.wifi_open = false;
            ui.power_open = false;
            ui.selected.clear();
        }

        ui.bar_hidden
    };

    if hidden {
        forget_password();
    }
}

pub fn forget_password() {
    TextInput::set_text("wifi-password", "");
}
