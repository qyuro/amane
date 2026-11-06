use std::sync::{LazyLock, Mutex, Once};
use std::thread;
use std::time::{Duration, Instant};

use amane::{
    Audio, Button, Center, Color, Column, Horizontal, Key, Keyboard, Layer, LayerWindow, Margin,
    Monitor, Palette, Parent, Padding, Pointer, Rectangle, Row, Service, Start, Text, Vertical,
    Weight, Widget, Zone,
};

use crate::state::{close_volume, Ui};
use crate::theme::{with_alpha, Theme};

const ICON_FONT: &str = "JetBrainsMono Nerd Font";

// how long the mini notice stays up, with a short fade at each end
const SHOW: f32 = 1500.0;
const FADE_IN: f32 = 140.0;
const FADE_OUT: f32 = 260.0;

// a small pill under the bar: icon, level line, percentage
const OSD_WIDTH: f32 = 210.0;
const OSD_HEIGHT: f32 = 38.0;
const OSD_PADDING: f32 = 12.0;
const OSD_ICON_WIDTH: f32 = 18.0;
const OSD_PERCENT_WIDTH: f32 = 38.0;
const OSD_GAP: f32 = 8.0;
const OSD_LINE: f32 = 5.0;

const OSD_TRACK: f32 = OSD_WIDTH
    - OSD_PADDING * 2.0
    - OSD_ICON_WIDTH
    - OSD_PERCENT_WIDTH
    - OSD_GAP * 2.0;

// how quickly the drawn level catches up with a key press, per poll
const LEVEL_EASE: f32 = 0.3;

const PANEL_WIDTH: f32 = 330.0;
const PANEL_HEIGHT: f32 = 352.0;

const SLIDER_WIDTH: f32 = PANEL_WIDTH - 32.0;
const SLIDER_HEIGHT: f32 = 28.0;
const LINE_HEIGHT: f32 = 8.0;

// the sound server takes a little time for each level, so a drag sends one
// change every interval instead of one for every pixel
const SEND_INTERVAL: Duration = Duration::from_millis(55);

/// Remembers the last levels shown and when the volume line began.
pub struct Notice {
    volume: u8,
    muted: bool,
    last: (u8, bool),

    // the level actually drawn, which glides toward the sound server's level
    level: f32,

    // when the line first appeared and when it may start fading out; another
    // key press while it is up extends the hold instead of blinking it
    shown_at: Option<Instant>,
    hold_until: Option<Instant>,
}

impl Service for Notice {
    fn new() -> Self {
        let audio = Audio::read();
        let level = (audio.volume(), audio.muted());

        Self {
            volume: level.0,
            muted: level.1,
            last: level,
            level: f32::from(level.0),
            shown_at: None,
            hold_until: None,
        }
    }

    fn interval() -> Duration {
        Duration::from_millis(16)
    }

    // a change made by a niri hotkey goes through wpctl, and the sound server
    // reports it to Audio; comparing the levels is enough to notice it
    fn update(&mut self) -> bool {
        let audio = Audio::read();
        let target = (audio.volume(), audio.muted());

        let changed = target != self.last;

        if changed {
            self.last = target;
            self.volume = target.0;
            self.muted = target.1;

            let now = Instant::now();

            if !self.hold_until.is_some_and(|until| until > now) {
                self.shown_at = Some(now);
            }

            self.hold_until = Some(now + Duration::from_secs_f32(SHOW / 1000.0));
        }

        let before = self.level;
        self.level = ease(self.level, f32::from(self.volume));

        changed || (self.level - before).abs() > 0.01
    }
}

impl Notice {
    pub fn volume(&self) -> u8 {
        self.volume
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    /// The level to draw, eased so a key press moves the line instead of jumping.
    pub fn level(&self) -> f32 {
        self.level
    }

    pub fn active(&self) -> bool {
        self.hold_until
            .is_some_and(|until| until > Instant::now())
    }

    // 0 to 1, fading in and back out at the ends of the notice
    pub fn value(&self) -> f32 {
        let Some(start) = self.shown_at else {
            return 0.0;
        };

        let Some(until) = self.hold_until else {
            return 0.0;
        };

        let now = Instant::now();

        if now >= until {
            return 0.0;
        }

        let elapsed = now.duration_since(start).as_secs_f32() * 1000.0;
        let remaining = until.duration_since(now).as_secs_f32() * 1000.0;

        let fade_in = (elapsed / FADE_IN).min(1.0);
        let fade_out = (remaining / FADE_OUT).min(1.0);

        fade_in.min(fade_out).clamp(0.0, 1.0)
    }
}

/// Hides the notice at once, for opening or closing the volume panel.
pub fn hide() {
    let mut notice = Notice::write();

    notice.shown_at = None;
    notice.hold_until = None;
}

pub fn icon(volume: u8, muted: bool) -> &'static str {
    if muted || volume == 0 {
        "\u{f026}" // crossed speaker
    } else if volume < 40 {
        "\u{f027}" // one wave
    } else {
        "\u{f028}" // two waves
    }
}

pub fn microphone_icon(volume: u8, muted: bool) -> &'static str {
    if muted || volume == 0 {
        "\u{f131}" // crossed microphone
    } else {
        "\u{f130}" // microphone
    }
}

/// Moves the volume by a few percent, unmuting on the way up.
pub fn nudge(delta: i16, volume: u8, muted: bool) {
    let volume = (i16::from(volume) + delta).clamp(0, 100) as u8;

    if muted && delta > 0 {
        Audio::toggle_mute();
    }

    Audio::set_volume(volume);
}

// the level waiting for the worker, and when the last one was handed over
struct Sending {
    pending: Option<u8>,
    last: Option<Instant>,
}

static OUTPUT: LazyLock<Mutex<Sending>> = LazyLock::new(|| {
    Mutex::new(Sending {
        pending: None,
        last: None,
    })
});

static MICROPHONE: LazyLock<Mutex<Sending>> = LazyLock::new(|| {
    Mutex::new(Sending {
        pending: None,
        last: None,
    })
});

fn due(sending: &Mutex<Sending>) -> bool {
    let sending = sending.lock().unwrap_or_else(|error| error.into_inner());

    sending.pending.is_some()
        && sending
            .last
            .is_none_or(|last| last.elapsed() >= SEND_INTERVAL)
}

/// Changes the output volume, keeping drags from flooding the sound server.
pub fn set(volume: u8) {
    send(&OUTPUT, volume, false);
}

/// The same for the microphone.
pub fn set_microphone(volume: u8) {
    send(&MICROPHONE, volume, true);
}

fn send(sending: &Mutex<Sending>, volume: u8, microphone: bool) {
    let volume = volume.min(100);

    let send_now = {
        let mut sending = sending.lock().unwrap_or_else(|error| error.into_inner());

        sending.pending = Some(volume);

        sending
            .last
            .is_none_or(|last| last.elapsed() >= SEND_INTERVAL)
    };

    if send_now {
        flush_slot(sending, microphone);
    } else {
        start_sender();
    }
}

/// Hands the last level of a drag to the worker right away.
pub fn flush() {
    flush_slot(&OUTPUT, false);
}

pub fn flush_microphone() {
    flush_slot(&MICROPHONE, true);
}

fn flush_slot(sending: &Mutex<Sending>, microphone: bool) {
    let volume = {
        let mut sending = sending.lock().unwrap_or_else(|error| error.into_inner());

        let volume = sending.pending.take();

        if volume.is_some() {
            sending.last = Some(Instant::now());
        }

        volume
    };

    if let Some(volume) = volume {
        if microphone {
            Audio::set_microphone_volume(volume);
        } else {
            Audio::set_volume(volume);
        }
    }
}

// a drag often ends between two sends, so the last level goes out shortly after
fn start_sender() {
    static STARTED: Once = Once::new();

    STARTED.call_once(|| {
        thread::spawn(|| loop {
            thread::sleep(Duration::from_millis(25));

            if due(&OUTPUT) {
                flush_slot(&OUTPUT, false);
            }

            if due(&MICROPHONE) {
                flush_slot(&MICROPHONE, true);
            }
        });
    });
}

/// The small volume pill under the bar, shown for key presses and the wheel.
pub fn osd(_monitor: &Monitor) -> LayerWindow {
    let palette = Palette::read();
    let theme = Theme::from(&palette);

    let ui = Ui::read();
    let notice = Notice::read();

    let reveal = notice.value();
    let active = notice.active() && !ui.volume_open;

    if active {
        amane::request_frame();
    }

    let volume = notice.volume();
    let level = notice.level();
    let muted = notice.muted();
    let color = if muted { theme.muted } else { theme.accent };

    let track = Rectangle::new()
        .width(OSD_TRACK)
        .height(OSD_LINE)
        .radius(amane::Full)
        .fill(with_alpha(theme.fg, 28))
        .align_child(Start, Center)
        .clip()
        .child(
            Rectangle::new()
                .width(OSD_TRACK * (level / 100.0))
                .height(OSD_LINE)
                .radius(amane::Full)
                .fill(color),
        );

    let contents = Row::new(vec![
        Box::new(
            Rectangle::new()
                .width(OSD_ICON_WIDTH)
                .height(20.0)
                .fill(Color::TRANSPARENT)
                .align_child(Center, Center)
                .child(
                    Text::new(icon(volume, muted))
                        .size(14.0)
                        .color(color)
                        .font(ICON_FONT)
                        .tight(),
                ),
        ) as Box<dyn Widget>,
        Box::new(track),
        Box::new(
            Rectangle::new()
                .width(OSD_PERCENT_WIDTH)
                .height(20.0)
                .fill(Color::TRANSPARENT)
                .align_child(amane::End, Center)
                .child(
                    Text::new(format!("{volume}%"))
                        .size(11.0)
                        .color(theme.fg)
                        .weight(Weight::Medium),
                ),
        ),
    ])
    .width(Parent)
    .gap(OSD_GAP)
    .align(Center);

    let root = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .radius(12.0)
        .fill(Color::BLACK)
        .border(1.0, theme.border)
        .padding(Padding {
            left: OSD_PADDING,
            right: OSD_PADDING,
            top: 0.0,
            bottom: 0.0,
        })
        .opacity(reveal)
        .scale(0.97 + 0.03 * reveal)
        .translate(0.0, (1.0 - reveal) * -6.0)
        .child(contents);

    LayerWindow::new()
        .width(OSD_WIDTH)
        .height(OSD_HEIGHT)
        .anchor_vertical(Vertical::Top)
        .anchor_horizontal(Horizontal::Middle)
        .margin(Margin {
            top: 42,
            right: 0,
            bottom: 0,
            left: 0,
        })
        .layer(Layer::Overlay)
        .space(Zone::Ignore)
        .visible(active)
        .click_through()
        .namespace("amane-volume-notice")
        .child(root)
}

/// The volume slider opened by the bar icon.
pub fn panel() -> LayerWindow {
    let palette = Palette::read();
    let theme = Theme::from(&palette);

    let ui = Ui::read();
    let audio = Audio::read();

    let fade = ui.volume.value();
    let volume = audio.volume();
    let muted = audio.muted();
    let microphone = audio.microphone_volume();
    let microphone_muted = audio.microphone_muted();

    let body = Column::new(vec![
        Box::new(header(&theme)) as Box<dyn Widget>,
        Box::new(level(
            icon(volume, muted),
            volume,
            muted,
            if muted { "выключен" } else { "звук" },
            30.0,
            &theme,
        )),
        Box::new(slider(volume, muted, false, &theme)),
        Box::new(mute_button(muted, false, &theme)),
        Box::new(divider(&theme)),
        Box::new(section("Микрофон", &theme)),
        Box::new(level(
            microphone_icon(microphone, microphone_muted),
            microphone,
            microphone_muted,
            if microphone_muted { "выключен" } else { "запись" },
            22.0,
            &theme,
        )),
        Box::new(slider(microphone, microphone_muted, true, &theme)),
        Box::new(mute_button(microphone_muted, true, &theme)),
    ])
    .width(Parent)
    .gap(9.0);

    let root = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .radius(18.0)
        .fill(Color::BLACK)
        .border(1.0, theme.border)
        .padding(Padding {
            left: 16.0,
            right: 16.0,
            top: 16.0,
            bottom: 16.0,
        })
        .opacity(fade)
        .scale(0.98 + 0.02 * fade)
        .translate(0.0, (1.0 - fade) * -10.0)
        .child(body);

    LayerWindow::new()
        .width(PANEL_WIDTH)
        .height(PANEL_HEIGHT)
        .anchor_vertical(Vertical::Top)
        .anchor_horizontal(Horizontal::Middle)
        .margin(Margin {
            top: 46,
            right: 0,
            bottom: 0,
            left: 0,
        })
        .layer(Layer::Overlay)
        .space(Zone::Ignore)
        .keyboard(Keyboard::OnDemand)
        .visible(ui.volume_open || fade > 0.0)
        .namespace("amane-volume")
        .on_key(|key| {
            if key == Key::Escape {
                close_volume();
            }
        })
        .child(root)
}

fn slider(volume: u8, muted: bool, microphone: bool, theme: &Theme) -> Rectangle {
    let fill_width = SLIDER_WIDTH * (volume as f32 / 100.0);

    let set_level: fn(u8) = if microphone { set_microphone } else { set };
    let flush_level: fn() = if microphone {
        flush_microphone
    } else {
        flush
    };

    let line = Rectangle::new()
        .width(SLIDER_WIDTH)
        .height(LINE_HEIGHT)
        .radius(amane::Full)
        .fill(with_alpha(theme.fg, 28))
        .align_child(Start, Center)
        .clip()
        .child(
            Rectangle::new()
                .width(fill_width)
                .height(LINE_HEIGHT)
                .radius(amane::Full)
                .fill(if muted { theme.muted } else { theme.accent }),
        );

    // the line is thin, so the whole slider area takes the drag
    Rectangle::new()
        .width(SLIDER_WIDTH)
        .height(SLIDER_HEIGHT)
        .fill(Color::TRANSPARENT)
        .align_child(Start, Center)
        .cursor(Pointer)
        .on_drag(move |point| {
            let percent = (point.x / SLIDER_WIDTH * 100.0).clamp(0.0, 100.0).round() as u8;

            set_level(percent);
        })
        .on_click(move |_| flush_level())
        .child(line)
}

/// The big icon, percentage and state label at the top of a section.
fn level(
    icon: &'static str,
    volume: u8,
    muted: bool,
    label: &'static str,
    size: f32,
    theme: &Theme,
) -> Row {
    let color = if muted { theme.muted } else { theme.accent };

    Row::new(vec![
        Box::new(
            Text::new(icon)
                .size(size)
                .color(color)
                .font(ICON_FONT)
                .tight(),
        ) as Box<dyn Widget>,
        Box::new(
            Text::new(format!("{volume}%"))
                .size(size)
                .color(theme.fg)
                .weight(Weight::Medium),
        ),
        Box::new(
            Rectangle::new()
                .width(Parent)
                .height(1.0)
                .fill(Color::TRANSPARENT),
        ),
        Box::new(Text::new(label).size(11.0).color(theme.muted)),
    ])
    .width(Parent)
    .gap(10.0)
    .align(Center)
}

fn section(title: &'static str, theme: &Theme) -> Row {
    Row::new(vec![
        Box::new(
            Text::new(title)
                .size(11.5)
                .color(theme.muted)
                .weight(Weight::Medium),
        ),
        Box::new(
            Rectangle::new()
                .width(Parent)
                .height(1.0)
                .fill(Color::TRANSPARENT),
        ),
    ])
    .width(Parent)
    .gap(6.0)
    .align(Center)
}

fn divider(theme: &Theme) -> Rectangle {
    Rectangle::new()
        .width(Parent)
        .height(1.0)
        .fill(with_alpha(theme.fg, 18))
}

fn mute_button(muted: bool, microphone: bool, theme: &Theme) -> Rectangle {
    let label = match (microphone, muted) {
        (false, false) => "Выключить звук",
        (false, true) => "Включить звук",
        (true, false) => "Выключить микрофон",
        (true, true) => "Включить микрофон",
    };

    Rectangle::new()
        .width(Parent)
        .height(34.0)
        .radius(10.0)
        .fill(if muted {
            with_alpha(theme.danger, 55)
        } else {
            theme.surface
        })
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(move |_| {
            if microphone {
                Audio::toggle_microphone_mute();
            } else {
                Audio::toggle_mute();
            }
        })
        .child(
            Text::new(label)
                .size(12.5)
                .color(if muted { theme.danger } else { theme.fg }),
        )
}

fn header(theme: &Theme) -> Row {
    Row::new(vec![
        Box::new(
            Text::new("Громкость")
                .size(16.0)
                .color(theme.fg)
                .weight(Weight::SemiBold),
        ),
        Box::new(
            Rectangle::new()
                .width(Parent)
                .height(1.0)
                .fill(Color::TRANSPARENT),
        ),
        Box::new(
            Rectangle::new()
                .width(26.0)
                .height(26.0)
                .radius(8.0)
                .fill(theme.surface)
                .align_child(Center, Center)
                .cursor(Pointer)
                .on_click(|_: Button| close_volume())
                .child(
                    Text::new("\u{f0156}")
                        .size(13.0)
                        .color(theme.fg)
                        .font(ICON_FONT)
                        .tight(),
                ),
        ),
    ])
    .width(Parent)
    .gap(6.0)
    .align(Center)
}

// glides a drawn value toward its target, snapping once the difference is too
// small to see
fn ease(current: f32, target: f32) -> f32 {
    let next = current + (target - current) * LEVEL_EASE;

    if (target - next).abs() < 0.15 {
        target
    } else {
        next
    }
}
