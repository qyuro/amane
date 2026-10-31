mod bar;
mod clock;
mod history;
mod launcher;
mod layout;
mod lock;
mod notify;
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

// `amane ipc call lock`, bound to Mod+Shift+P
fn lock_session(_: &[String]) -> String {
    lock::lock_now();

    String::from("locking")
}

// the bell in the bar does the same; handy for a hotkey
fn toggle_history(_: &[String]) -> String {
    state::toggle_history();

    String::from("history toggled")
}

// turns the popups off or back on without opening the panel
fn toggle_notifications(_: &[String]) -> String {
    history::History::toggle_mute();

    let muted = history::History::read().muted();

    if muted {
        String::from("notifications off")
    } else {
        String::from("notifications on")
    }
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
    drop(layout::Layout::read());
    drop(lock::Locker::read());

    // owning org.freedesktop.Notifications from the start means no early popup is lost
    drop(amane::Notifications::read());
    drop(history::History::read());
    drop(notify::Expiry::read());

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
        .window(notify::view)
        .window(history::view)
        .lock(lock::view)
        .ipc("toggle-bar", toggle_bar)
        .ipc("toggle-launcher", toggle_launcher)
        .ipc("lock", lock_session)
        .ipc("toggle-history", toggle_history)
        .ipc("toggle-notifications", toggle_notifications)
        .run();
}
