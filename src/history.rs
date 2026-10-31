use std::thread;
use std::time::{Duration, SystemTime};

use amane::{
    Align, Button, Center, Color, Column, Horizontal, Key, Keyboard, Layer, LayerWindow, Margin,
    Notification, Notifications, Palette, Parent, Padding, Pointer, Rectangle, Row, ScrollArea,
    Service, Text, Urgency, Vertical, Weight, Widget, Zone,
};

use crate::notify::clean;
use crate::state::{close_history, Ui};
use crate::theme::{with_alpha, Theme};

const ICON_FONT: &str = "JetBrainsMono Nerd Font";

// the panel keeps the last notifications, so a restart does not lose them all
const MAX_ENTRIES: usize = 50;

const WIDTH: f32 = 360.0;
const HEIGHT: f32 = 420.0;

// the list scrolls instead of growing the panel
const LIST_HEIGHT: f32 = 300.0;
const ROW_HEIGHT: f32 = 64.0;

const BELL: &str = "\u{f0f3}";
const BELL_OFF: &str = "\u{f1f6}";
const CLOSE: &str = "\u{f0156}";
const DISMISS: &str = "\u{2715}";

/// One archived notification, kept after the live popup has gone away.
pub struct Entry {
    notification: Notification,

    // the bell only lights up for notifications that arrived while the history
    // was closed; opening the panel marks them all read
    unread: bool,
}

impl Entry {
    pub fn id(&self) -> u32 {
        self.notification.id()
    }

    pub fn notification(&self) -> &Notification {
        &self.notification
    }

    /// How long ago it arrived, as a short label for the row.
    pub fn age(&self) -> String {
        let seconds = SystemTime::now()
            .duration_since(self.notification.received())
            .map(|age| age.as_secs())
            .unwrap_or(0);

        match seconds {
            0..=59 => String::from("сейчас"),
            60..=3599 => format!("{} мин", seconds / 60),
            3600..=86_399 => format!("{} ч", seconds / 3600),
            _ => format!("{} д", seconds / 86_400),
        }
    }
}

/// Keeps a copy of every notification, including the ones the popup closed.
///
/// `Notifications` drops a notification from its live list as soon as it
/// expires or is dismissed, and the popup reads that list; the archive is what
/// the history panel shows afterwards.
pub struct History {
    entries: Vec<Entry>,

    /// Do Not Disturb: popups are hidden, but the archive still collects them
    muted: bool,
}

impl Service for History {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            muted: false,
        }
    }

    /*
     * The default loop holds the service write lock while calling update(),
     * and update() would need `Notifications`. The popup view reads
     * `Notifications` first and this service second, so that order could
     * deadlock. Reading the live list before taking the write lock keeps the
     * two locks apart.
     */
    fn listen() {
        loop {
            thread::sleep(Duration::from_millis(400));

            let live: Vec<Notification> = {
                let notifications = Notifications::read();

                notifications.list().to_vec()
            };

            // a poll that found nothing new must not wake every window
            if !History::read().fresh(&live) {
                continue;
            }

            History::write().archive(live);
        }
    }
}

impl History {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    pub fn unread(&self) -> usize {
        self.entries.iter().filter(|entry| entry.unread).count()
    }

    // true when the live list has something the archive has not seen or has
    // seen in an older version
    fn fresh(&self, live: &[Notification]) -> bool {
        live.iter().any(|notification| {
            !self.entries.iter().any(|entry| {
                entry.id() == notification.id()
                    && entry.notification.received() == notification.received()
            })
        })
    }

    fn archive(&mut self, live: Vec<Notification>) {
        let muted = self.muted;

        for notification in live {
            let id = notification.id();
            let received = notification.received();

            if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id() == id) {
                // a sender may replace its earlier notification, keeping its id
                if entry.notification.received() == received {
                    continue;
                }

                entry.notification = notification;
                entry.unread = !muted;

                continue;
            }

            self.entries.insert(
                0,
                Entry {
                    notification,
                    unread: !muted,
                },
            );
        }

        self.entries.truncate(MAX_ENTRIES);
    }

    /// Opening the history clears the bell's unread state.
    pub fn mark_read() {
        let mut history = Self::write();

        for entry in &mut history.entries {
            entry.unread = false;
        }
    }

    /// Turns popups off or back on; the archive keeps collecting either way.
    pub fn toggle_mute() {
        let muted = {
            let mut history = Self::write();

            history.muted = !history.muted;

            // no popup means nothing new to point the bell at
            if history.muted {
                for entry in &mut history.entries {
                    entry.unread = false;
                }
            }

            history.muted
        };

        // clear the live list while muted, so the old popups do not come back
        // when notifications are turned on again
        if muted {
            Notifications::clear();
        }
    }

    /// Removes one notification from both the archive and the live popups.
    pub fn dismiss(id: u32) {
        // the live list goes first: the archive poll would otherwise put the
        // entry back before the dismissal is seen
        Notifications::dismiss(id);

        let mut history = Self::write();

        if let Some(index) = history.entries.iter().position(|entry| entry.id() == id) {
            history.entries.remove(index);
        }
    }

    pub fn clear() {
        Notifications::clear();

        Self::write().entries.clear();
    }
}

pub fn view() -> LayerWindow {
    let palette = Palette::read();
    let theme = Theme::from(&palette);

    let ui = Ui::read();
    let history = History::read();

    let fade = ui.history.value();

    let mut content: Vec<Box<dyn Widget>> = Vec::new();

    content.push(Box::new(header(&history, &theme)));

    if history.entries().is_empty() {
        content.push(Box::new(empty_state(&history, &theme)));
    } else {
        content.push(Box::new(list(&history, &theme)));
    }

    content.push(Box::new(footer(&history, &theme)));

    let body = Column::new(content).width(Parent).gap(10.0);

    let root = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .radius(18.0)
        .fill(theme.panel)
        .border(1.0, theme.border)
        .padding(16.0)
        .opacity(fade)
        .scale(0.98 + 0.02 * fade)
        .translate(0.0, (1.0 - fade) * -10.0)
        .child(body);

    LayerWindow::new()
        .width(WIDTH)
        .height(HEIGHT)
        .anchor_vertical(Vertical::Top)
        .anchor_horizontal(Horizontal::Right)
        .margin(Margin {
            top: 46,
            right: 12,
            bottom: 0,
            left: 0,
        })
        .layer(Layer::Overlay)
        .space(Zone::Ignore)
        .keyboard(Keyboard::OnDemand)
        .visible(ui.history_open || fade > 0.0)
        .namespace("amane-history")
        .on_key(|key| {
            if key == Key::Escape {
                close_history();
            }
        })
        .child(root)
}

fn header(history: &History, theme: &Theme) -> Row {
    let mut items: Vec<Box<dyn Widget>> = Vec::new();

    items.push(Box::new(
        Text::new("Уведомления")
            .size(16.0)
            .color(theme.fg)
            .weight(Weight::SemiBold),
    ));

    let count = history.entries().len();

    if count > 0 {
        items.push(Box::new(
            Text::new(format!("{count}"))
                .size(10.5)
                .color(theme.muted),
        ));
    }

    // pushes the buttons to the right edge
    items.push(Box::new(
        Rectangle::new()
            .width(Parent)
            .height(1.0)
            .fill(Color::TRANSPARENT),
    ));

    let (icon, color) = if history.muted() {
        (BELL_OFF, theme.muted)
    } else {
        (BELL, theme.fg)
    };

    items.push(Box::new(icon_button(
        icon,
        color,
        theme,
        |_| History::toggle_mute(),
    )));

    items.push(Box::new(icon_button(CLOSE, theme.fg, theme, |_| {
        close_history()
    })));

    Row::new(items).width(Parent).gap(6.0).align(Center)
}

fn empty_state(history: &History, theme: &Theme) -> Rectangle {
    let message = if history.muted() {
        "Уведомления выключены"
    } else {
        "Здесь пока пусто"
    };

    Rectangle::new()
        .width(Parent)
        .height(LIST_HEIGHT)
        .radius(12.0)
        .fill(theme.surface)
        .align_child(Center, Center)
        .child(
            Column::new(vec![
                Box::new(
                    Text::new(if history.muted() { BELL_OFF } else { BELL })
                        .size(26.0)
                        .color(theme.muted)
                        .font(ICON_FONT)
                        .tight(),
                ) as Box<dyn Widget>,
                Box::new(Text::new(message).size(12.0).color(theme.muted)),
            ])
            .gap(6.0)
            .align(Center),
        )
}

fn list(history: &History, theme: &Theme) -> ScrollArea {
    let mut rows: Vec<Box<dyn Widget>> = Vec::new();

    for entry in history.entries() {
        rows.push(Box::new(entry_row(entry, theme)));
    }

    ScrollArea::new("history-list", Column::new(rows).width(Parent).gap(4.0)).height(LIST_HEIGHT)
}

fn entry_row(entry: &Entry, theme: &Theme) -> Rectangle {
    let notification = entry.notification();
    let id = entry.id();

    let accent = match notification.urgency() {
        Urgency::Critical => theme.danger,
        Urgency::Low => theme.muted,
        Urgency::Normal => theme.accent,
    };

    // the same urgency stripe the popup card uses
    let stripe = Rectangle::new()
        .width(3.0)
        .height(38.0)
        .radius(3.0)
        .fill(accent);

    let app = clean(notification.app_name());
    let summary = clean(notification.summary());
    let body = clean(notification.body());

    let mut meta: Vec<Box<dyn Widget>> = Vec::new();

    if !app.is_empty() {
        meta.push(Box::new(
            Text::new(app)
                .size(9.5)
                .color(theme.muted)
                .weight(Weight::Medium)
                .elide(),
        ));
    }

    meta.push(Box::new(
        Rectangle::new()
            .width(Parent)
            .height(1.0)
            .fill(Color::TRANSPARENT),
    ));

    meta.push(Box::new(
        Text::new(entry.age()).size(9.5).color(theme.muted),
    ));

    let meta = Row::new(meta).width(Parent).gap(6.0).align(Center);

    let mut labels: Vec<Box<dyn Widget>> = vec![
        Box::new(meta) as Box<dyn Widget>,
        Box::new(
            Text::new(summary)
                .size(12.5)
                .color(theme.fg)
                .weight(Weight::SemiBold)
                .max_lines(1)
                .elide(),
        ),
    ];

    if !body.is_empty() {
        labels.push(Box::new(
            Text::new(body)
                .size(10.5)
                .color(theme.muted)
                .max_lines(1)
                .elide(),
        ));
    }

    let labels = Column::new(labels)
        .width(Parent)
        .gap(1.0)
        .align(Align::Start);

    let close = Rectangle::new()
        .width(18.0)
        .height(18.0)
        .radius(6.0)
        .fill(Color::TRANSPARENT)
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(move |_| History::dismiss(id))
        .child(Text::new(DISMISS).size(10.0).color(theme.muted));

    let content = Row::new(vec![
        Box::new(stripe) as Box<dyn Widget>,
        Box::new(labels),
        Box::new(close),
    ])
    .width(Parent)
    .gap(8.0)
    .align(Center);

    Rectangle::new()
        .width(Parent)
        .height(ROW_HEIGHT)
        .radius(11.0)
        .fill(with_alpha(theme.fg, 14))
        .padding(Padding {
            left: 10.0,
            right: 8.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(move |_| Notifications::click(id))
        .child(content)
}

fn footer(history: &History, theme: &Theme) -> Row {
    let status = if history.muted() {
        String::from("Не беспокоить")
    } else {
        format!("Всего: {}", history.entries().len())
    };

    let mut items: Vec<Box<dyn Widget>> = vec![
        Box::new(Text::new(status).size(10.5).color(theme.muted)) as Box<dyn Widget>,
        Box::new(
            Rectangle::new()
                .width(Parent)
                .height(1.0)
                .fill(Color::TRANSPARENT),
        ),
    ];

    if !history.entries().is_empty() {
        items.push(Box::new(
            Rectangle::new()
                .width(104.0)
                .height(28.0)
                .radius(8.0)
                .fill(theme.surface)
                .align_child(Center, Center)
                .cursor(Pointer)
                .on_click(|_| History::clear())
                .child(Text::new("Очистить всё").size(11.5).color(theme.fg)),
        ));
    }

    Row::new(items)
        .width(Parent)
        .height(28.0)
        .gap(8.0)
        .align(Center)
}

fn icon_button(
    icon: &'static str,
    color: Color,
    theme: &Theme,
    clicked: impl Fn(Button) + 'static,
) -> Rectangle {
    Rectangle::new()
        .width(26.0)
        .height(26.0)
        .radius(8.0)
        .fill(theme.surface)
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(clicked)
        .child(
            Text::new(icon)
                .size(13.0)
                .color(color)
                .font(ICON_FONT)
                .tight(),
        )
}
