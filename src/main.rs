mod bar;
mod clock;
mod panel;
mod power;
mod state;
mod theme;

use amane::{App, Service};

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
    drop(amane::Network::read());
    drop(amane::Battery::read());
    drop(amane::Workspaces::read());

    App::new()
        .window_per_monitor(bar::view)
        .window(panel::wifi)
        .window(panel::power)
        .run();
}
