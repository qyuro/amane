use std::time::{Duration, SystemTime};

use amane::{
    Align, Center, Column, Color, Horizontal, Layer, LayerWindow, Margin, Notification,
    Notifications, Palette, Parent, Padding, Pointer, Rectangle, Row, Service, Text, Urgency,
    Vertical, Weight, Widget, Zone,
};

use crate::history::History;
use crate::theme::Theme;

const WIDTH: f32 = 330.0;
const CARD_HEIGHT: f32 = 70.0;
const GAP: f32 = 8.0;

// the bar itself stays on the top layer, and niri hides that layer behind a
// fullscreen window, so notifications never cover fullscreen video or games
pub fn view() -> LayerWindow {
    let palette = Palette::read();
    let theme = Theme::from(&palette);

    let notifications = Notifications::read();
    let history = History::read();

    // Do Not Disturb keeps collecting the history, but nothing pops up
    let list: Vec<&Notification> = if history.muted() {
        Vec::new()
    } else {
        // newest on top
        notifications.list().iter().rev().take(4).collect()
    };

    let mut cards: Vec<Box<dyn Widget>> = Vec::new();

    for notification in &list {
        cards.push(Box::new(card(notification, &theme)));
    }

    let count = cards.len() as f32;

    let height = if count == 0.0 {
        1.0
    } else {
        count * CARD_HEIGHT + (count - 1.0) * GAP
    };

    LayerWindow::new()
        .width(WIDTH)
        .height(height)
        .anchor_vertical(Vertical::Top)
        .anchor_horizontal(Horizontal::Right)
        .margin(Margin {
            top: 8,
            right: 8,
            bottom: 0,
            left: 0,
        })
        .layer(Layer::Top)
        .space(Zone::Ignore)
        .visible(count > 0.0)
        .namespace("amane-notify")
        .child(Column::new(cards).width(Parent).gap(GAP))
}

fn card(notification: &Notification, theme: &Theme) -> Rectangle {
    let id = notification.id();

    let accent = match notification.urgency() {
        Urgency::Critical => theme.danger,
        Urgency::Low => theme.muted,
        Urgency::Normal => theme.accent,
    };

    // the style bar on the left tells the urgency apart without any sound
    let stripe = Rectangle::new()
        .width(3.0)
        .height(42.0)
        .radius(3.0)
        .fill(accent);

    let app = clean(notification.app_name());
    let summary = clean(notification.summary());
    let body = clean(notification.body());

    let mut labels: Vec<Box<dyn Widget>> = Vec::new();

    if !app.is_empty() {
        labels.push(Box::new(
            Text::new(app)
                .size(9.5)
                .color(theme.muted)
                .weight(Weight::Medium),
        ));
    }

    labels.push(Box::new(
        Text::new(summary)
            .size(12.5)
            .color(theme.fg)
            .weight(Weight::SemiBold)
            .elide(),
    ));

    if !body.is_empty() {
        labels.push(Box::new(
            Text::new(body)
                .size(11.0)
                .color(theme.muted)
                .max_lines(1)
                .elide(),
        ));
    }

    let text = Column::new(labels)
        .width(258.0)
        .gap(1.0)
        .align(Align::Start);

    let close = Rectangle::new()
        .width(18.0)
        .height(18.0)
        .radius(6.0)
        .fill(Color::TRANSPARENT)
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(move |_| Notifications::dismiss(id))
        .child(Text::new("\u{2715}").size(10.0).color(theme.muted));

    let content = Row::new(vec![
        Box::new(stripe) as Box<dyn Widget>,
        Box::new(text),
        Box::new(close),
    ])
    .width(296.0)
    .gap(8.0)
    .align(Center);

    Rectangle::new()
        .width(Parent)
        .height(CARD_HEIGHT)
        .radius(14.0)
        .fill(theme.panel)
        .border(1.0, theme.border)
        .padding(Padding {
            left: 14.0,
            right: 10.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(move |_| Notifications::click(id))
        .child(content)
}

// bodies may carry simple markup like <b>, which the text widget does not parse
pub(crate) fn clean(text: &str) -> String {
    let mut plain = String::new();
    let mut tag = false;

    for character in text.trim().chars() {
        match character {
            '<' => tag = true,
            '>' => tag = false,
            _ if !tag => plain.push(character),
            _ => {}
        }
    }

    plain
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

// amane keeps notifications until someone closes them, so this closes the
// old ones the way a normal popup would disappear
pub struct Expiry;

impl Service for Expiry {
    fn new() -> Self {
        Self
    }

    fn interval() -> Duration {
        Duration::from_secs(1)
    }

    fn update(&mut self) -> bool {
        let now = SystemTime::now();
        let mut expired: Vec<u32> = Vec::new();

        {
            let notifications = Notifications::read();

            for notification in notifications.list() {
                let limit = match notification.urgency() {
                    Urgency::Low => 4,
                    Urgency::Critical => 12,
                    Urgency::Normal => 7,
                };

                let old = now
                    .duration_since(notification.received())
                    .map(|age| age.as_secs() >= limit)
                    .unwrap_or(false);

                if old {
                    expired.push(notification.id());
                }
            }
        }

        if expired.is_empty() {
            return false;
        }

        for id in expired {
            Notifications::dismiss(id);
        }

        true
    }
}
