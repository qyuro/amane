use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use amane::{
    output, Center, Color, Column, Full, Image, Key, Keyboard, Layer, LayerWindow, Lock, Monitor,
    Palette, Parent, Padding, Rectangle, Service, Stack, Text, TextInput, Weight, Widget, Zone,
};

use crate::clock::Clock;
use crate::theme::{with_alpha, Theme};

// the screen locks after this long without a key or a pointer
const LOCK_AFTER: Duration = Duration::from_secs(10 * 60);

// one text input, kept by this name across redraws
const PASSWORD: &str = "lock-password";

// the event struct from <linux/input.h>: two i64 timestamps, u16 type, u16 code, i32 value
const INPUT_EVENT: usize = 24;

/*
 * Tracks real input from /dev/input, since niri does not report idle time over
 * its socket, and locks the session once nothing has happened for LOCK_AFTER.
 */
pub struct Locker {
    last: Arc<Mutex<Instant>>,
    seen: Arc<Mutex<HashSet<PathBuf>>>,
    date: String,
}

impl Service for Locker {
    fn new() -> Self {
        let last = Arc::new(Mutex::new(Instant::now()));
        let seen = Arc::new(Mutex::new(HashSet::new()));

        spawn_readers(&last, &seen);

        Self {
            last,
            seen,
            date: read_date(),
        }
    }

    // event-driven services get their own loop, and this one also watches the clock
    fn listen() {
        let mut ticks: u64 = 0;

        loop {
            thread::sleep(Duration::from_secs(1));

            ticks += 1;

            // keyboards and mice plugged in later start counting too
            if ticks % 10 == 0 {
                let locker = Locker::read();

                spawn_readers(&locker.last, &locker.seen);
            }

            // the date only changes at midnight, but the lock screen may stay up
            if ticks % 60 == 0 {
                let date = read_date();

                if Locker::read().date != date {
                    Locker::write().date = date;
                }
            }

            let idle = {
                let locker = Locker::read();

                last_input(&locker.last).elapsed() >= LOCK_AFTER
            };

            if idle {
                lock_now();
            }
        }
    }
}

impl Locker {
    pub fn date(&self) -> &str {
        &self.date
    }
}

/// Locks the session now, from idle, the ipc call or a hotkey.
pub fn lock_now() {
    let date = read_date();

    {
        let mut locker = Locker::write();

        *locker.last.lock().unwrap_or_else(PoisonError::into_inner) = Instant::now();
        locker.date = date;
    }

    Lock::start();
}

// the lock screen covers every monitor; the compositor keeps everything else hidden
pub fn view(_monitor: &Monitor) -> LayerWindow {
    let palette = Palette::read();
    let theme = Theme::from(&palette);

    let clock = Clock::read();
    let locker = Locker::read();
    let lock = Lock::read();

    let mut layers: Vec<Box<dyn Widget>> = Vec::new();

    let background = match crate::theme::current_wallpaper() {
        // a small copy, blurred once, so the backdrop costs nothing per frame
        Some(path) => Rectangle::new()
            .width(Parent)
            .height(Parent)
            .fill(Image::cover(path).thumbnail(480, 300).blurred(18)),

        // the palette itself when there is no wallpaper to show
        None => {
            let background = palette.background();

            Rectangle::new()
                .width(Parent)
                .height(Parent)
                .fill(Color::rgb(
                    background.red(),
                    background.green(),
                    background.blue(),
                ))
        }
    };

    layers.push(Box::new(background));

    // darkens the wallpaper enough for the clock and the field to stay readable
    layers.push(Box::new(
        Rectangle::new()
            .width(Parent)
            .height(Parent)
            .fill(with_alpha(palette.background(), 135)),
    ));

    let date = Text::new(String::from(locker.date()))
        .size(14.0)
        .color(theme.muted);

    let time = Text::new(format!("{}:{}", clock.hours(), clock.minutes()))
        .size(84.0)
        .color(theme.fg)
        .weight(Weight::Light);

    let field = Rectangle::new()
        .width(320.0)
        .height(48.0)
        .radius(14.0)
        .fill(with_alpha(theme.fg, 16))
        .border(1.0, with_alpha(theme.fg, 30))
        .padding(Padding {
            left: 16.0,
            right: 16.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_child(Center, Center)
        .child(
            TextInput::new(PASSWORD)
                .width(Parent)
                .placeholder("Пароль")
                .password()
                .focused()
                .size(14.0)
                .color(theme.fg)
                .on_submit(|password| {
                    if !password.is_empty() {
                        Lock::unlock(&password);
                    }
                }),
        );

    let (hint, hint_color) = if lock.checking() {
        ("Проверка пароля…", theme.muted)
    } else if lock.failed() {
        ("Неверный пароль", theme.danger)
    } else {
        ("Введите пароль", theme.muted)
    };

    // a little air between the time and the field, the way a lock screen has it
    let spacer = Rectangle::new()
        .width(1.0)
        .height(18.0)
        .fill(Color::TRANSPARENT);

    let content = Column::new(vec![
        Box::new(date),
        Box::new(time),
        Box::new(spacer),
        Box::new(field),
        Box::new(Text::new(hint).size(11.5).color(hint_color)),
    ])
    .gap(10.0)
    .align(Center);

    layers.push(Box::new(
        Rectangle::new()
            .width(Parent)
            .height(Parent)
            .fill(Color::TRANSPARENT)
            .align_child(Center, Center)
            .child(content),
    ));

    LayerWindow::new()
        .width(Full)
        .height(Full)
        .layer(Layer::Overlay)
        .space(Zone::Ignore)
        .keyboard(Keyboard::Exclusive)
        .namespace("amane-lock")
        .on_key(|key| {
            // escape clears what was typed and the focused input takes keys again
            if key == Key::Escape {
                TextInput::set_text(PASSWORD, "");
            }
        })
        .child(Stack::new(layers).width(Parent).height(Parent))
}

fn read_date() -> String {
    output("date +'%A, %d %B'").trim().to_string()
}

fn last_input(last: &Mutex<Instant>) -> Instant {
    *last.lock().unwrap_or_else(PoisonError::into_inner)
}

/*
 * every readable event device gets a blocking reader; one thread per device is
 * cheaper than polling them, and events from several keyboards do not race
 */
fn spawn_readers(last: &Arc<Mutex<Instant>>, seen: &Arc<Mutex<HashSet<PathBuf>>>) {
    let Ok(entries) = fs::read_dir("/dev/input") else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        let is_event = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("event"));

        if !is_event {
            continue;
        }

        {
            let mut seen = seen.lock().unwrap_or_else(PoisonError::into_inner);

            // a reader is already on this one, or the device was unplugged
            if !seen.insert(path.clone()) {
                continue;
            }
        }

        let last = Arc::clone(last);
        let seen = Arc::clone(seen);

        thread::spawn(move || {
            read_device(&path, &last);

            seen.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&path);
        });
    }
}

fn read_device(path: &Path, last: &Arc<Mutex<Instant>>) {
    let Ok(mut file) = File::open(path) else {
        return;
    };

    let mut event = [0u8; INPUT_EVENT];

    // a device that went away ends the loop, and the next scan may find it again
    while file.read_exact(&mut event).is_ok() {
        if !activity(&event) {
            continue;
        }

        *last.lock().unwrap_or_else(PoisonError::into_inner) = Instant::now();
    }
}

// keys, relative movement and absolute movement cover keyboards, mice and touchpads
fn activity(event: &[u8; INPUT_EVENT]) -> bool {
    let kind = u16::from_ne_bytes([event[16], event[17]]);

    matches!(kind, 0x01 | 0x02 | 0x03)
}
