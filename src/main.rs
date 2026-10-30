mod bar;
mod clock;
mod launcher;
mod panel;
mod power;
mod state;
mod theme;

use amane::{App, Service};

// `amane ipc call toggle-bar`, bound to Mod+O
fn toggle_bar(_: &[String]) -> String {
    state::toggle_bar();

    String::from("bar toggled")
}

// `amane ipc call toggle-launcher`, bound to Mod+D
fn toggle_launcher(_: &[String]) -> String {
    state::toggle_launcher();

    String::from("launcher toggled")
}

fn main() {
    /*
     * start the wallpaper palette before the first frame, so the bar already
     * has its colors; the service keeps watching the wallpaper after that
     */
    drop(theme::Wallpaper::read());

    // these read their source once, so doing it now keeps the first frame quick
    drop(clock::Clock::read());
    drop(power::PowerMode::read());
    drop(state::Ui::read());
    drop(state::Idle::read());
    drop(amane::Network::read());
    drop(amane::Battery::read());
    drop(amane::Workspaces::read());

    // the launcher's list is scanned in the background from here on
    drop(amane::Apps::read());

    App::new()
        .window_per_monitor(bar::view)
        .window(panel::wifi)
        .window(panel::power)
        .window(launcher::view)
        .ipc("toggle-bar", toggle_bar)
        .ipc("toggle-launcher", toggle_launcher)
        .run();
}
