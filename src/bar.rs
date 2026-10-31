use amane::{
    Battery, Canvas, Center, Color, Column, Full, Horizontal, InputArea, Layer, LayerWindow, Link,
    Margin, Monitor, Network, Palette, Path, Pointer, Rectangle, Row, Service, Shape, Size, Stack,
    Text, Vertical, Weight, Widget, Workspaces, Zone, request_frame,
};

use crate::clock::Clock;
use crate::history::History;
use crate::layout::Layout;
use crate::state::{toggle_history, toggle_power, toggle_wifi, touch, Ui};
use crate::theme::{with_alpha, Theme};

const ICON_FONT: &str = "JetBrainsMono Nerd Font";

const BELL: &str = "\u{f0f3}";
const BELL_OFF: &str = "\u{f1f6}";

// the bar has two sizes: the full one and the minimal one with only the ovals
const HEIGHT: f32 = 34.0;
const PADDING: f32 = 14.0;

// the clock keeps a little more room around it than the modules keep between them
const GAP: f32 = 9.0;
const CLOCK_GAP: f32 = 24.0;

// the clock is wider than two digits, so a minute change never moves the bar
const CLOCK_WIDTH: f32 = 28.0;

// the notification bell sits just left of the clock
const BELL_WIDTH: f32 = 26.0;
const BELL_GAP: f32 = 8.0;

// the language chip takes the ovals' place for a moment after a switch
const LANGUAGE_WIDTH: f32 = 30.0;
const LANGUAGE_HEIGHT: f32 = 22.0;

// the top corners turn back into the screen edge, so the bar flows out of it
const FLARE: f32 = 8.0;
const BOTTOM: f32 = 18.0;

pub fn view(monitor: &Monitor) -> LayerWindow {
    let palette = Palette::read();
    let theme = Theme::from(&palette);

    let ui = Ui::read();
    let layout = Layout::read();
    let clock = Clock::read();
    let history = History::read();
    let network = Network::read();
    let battery = Battery::read();
    let workspaces = Workspaces::read();

    // the chip holds for a moment after a switch, so it needs frames of its own
    let flash = layout.flash_mix();

    if layout.flash_active() {
        request_frame();
    }

    let spaces = workspace_row(monitor, &workspaces, &theme);

    let right = Row::new(vec![
        Box::new(wifi_button(&network, &theme, ui.wifi_open)),
        Box::new(battery_button(&battery, &theme, ui.power_open)),
    ])
    .gap(GAP)
    .align(Center);

    // measured before the widgets move into their wrappers
    let spaces_width = fixed_width(&spaces);
    let right_width = fixed_width(&right);

    // the ovals and the language chip share one slot of a fixed width, so the
    // chip can take their place without the clock or the pill moving
    let slot_width = spaces_width.max(LANGUAGE_WIDTH);

    // the narrower side gets blank room, so the clock sits on the pill's own
    // middle with the same distance to the left and right edge; the left side
    // also has to fit the bell, which widens the blank right side to match
    let side = (slot_width + BELL_GAP + BELL_WIDTH).max(right_width);

    let minimal_width = FLARE * 2.0 + PADDING * 2.0 + slot_width;
    let full_width = FLARE * 2.0 + PADDING * 2.0 + side * 2.0 + CLOCK_GAP * 2.0 + CLOCK_WIDTH;

    // the window keeps its full width while the pill folds, so the compositor
    // does not resize the surface on every frame of the animation
    let window_width = full_width;

    let place = ui.bar.value();
    let pill_width = minimal_width + (full_width - minimal_width) * place;
    let pill_left = (window_width - pill_width) / 2.0;

    // the clock and the modules fade in once the pill has opened a little
    let fade = ((place - 0.4) / 0.6).clamp(0.0, 1.0);
    let fade = fade * fade * (3.0 - 2.0 * fade);

    let mut layers: Vec<Box<dyn Widget>> = Vec::new();

    // the bar itself, a canvas so the top corners can bend back outwards
    layers.push(Box::new(
        Rectangle::new()
            .width(pill_width)
            .height(HEIGHT)
            .fill(Color::TRANSPARENT)
            .translate(pill_left, 0.0)
            .child(pill_shape(pill_width, &theme)),
    ));

    // the ovals stay at the body's left padding; in the minimal pill that is
    // also the whole inner width, so they sit in the middle on their own
    layers.push(Box::new(
        Rectangle::new()
            .width(slot_width)
            .height(HEIGHT)
            .fill(Color::TRANSPARENT)
            .align_child(Center, Center)
            .translate(pill_left + FLARE + PADDING, 0.0)
            .child(language_slot(
                spaces,
                layout.current(),
                flash,
                slot_width,
                &theme,
            )),
    ));

    // leave the invisible modules out entirely, or they would catch clicks
    // meant for the ovals while the bar is folded
    if fade > 0.0 {
        let clock_x = pill_left + (pill_width - CLOCK_WIDTH) / 2.0;

        layers.push(Box::new(
            Rectangle::new()
                .width(CLOCK_WIDTH)
                .height(HEIGHT)
                .fill(Color::TRANSPARENT)
                .opacity(fade)
                .align_child(Center, Center)
                .translate(clock_x, 0.0)
                .child(clock_view(&clock, &theme)),
        ));

        // the bell takes the spot just left of the centered clock
        let bell_x = clock_x - BELL_GAP - BELL_WIDTH;

        layers.push(Box::new(
            Rectangle::new()
                .width(BELL_WIDTH)
                .height(HEIGHT)
                .fill(Color::TRANSPARENT)
                .opacity(fade)
                .align_child(Center, Center)
                .translate(bell_x, 0.0)
                .child(bell_button(&history, &theme, ui.history_open)),
        ));

        let right_x = pill_left + pill_width - FLARE - PADDING - right_width;

        layers.push(Box::new(
            Rectangle::new()
                .width(right_width)
                .height(HEIGHT)
                .fill(Color::TRANSPARENT)
                .opacity(fade)
                .align_child(Center, Center)
                .translate(right_x, 0.0)
                .child(right),
        ));
    }

    let content = Stack::new(layers).width(window_width).height(HEIGHT);

    // the transparent sides of the window let clicks through, the pill does not
    let input_x = pill_left.round() as i32;
    let input_width = pill_width.round() as i32;

    LayerWindow::new()
        .width(window_width)
        .height(HEIGHT)
        .anchor_vertical(Vertical::Top)
        .anchor_horizontal(Horizontal::Middle)
        .margin(Margin {
            top: 0,
            right: 0,
            bottom: 0,
            left: 0,
        })
        .layer(Layer::Top)
        .space(Zone::Reserve)
        .namespace("amane-bar")
        .input_region(vec![InputArea {
            x: input_x,
            y: 0,
            width: input_width,
            height: HEIGHT.round() as i32,
        }])
        .child(content)
}

/*
 * the pill is flush with the screen's top edge and its sides bend back
 * outwards into that edge; the flush top and the bent corners are what make
 * it look like the bar grows out of the screen instead of lying under it
 */
fn pill_shape(width: f32, theme: &Theme) -> Canvas {
    // a tiny pill would otherwise have its bottom corners cross each other
    let bottom = BOTTOM
        .min((width - FLARE * 2.0) / 2.0)
        .min(HEIGHT / 2.0);

    let path = Path::new()
        .move_to(0.0, 0.0)
        .arc(0.0, FLARE, FLARE, 0.0, 90.0)
        .line_to(FLARE, HEIGHT - bottom)
        .arc(FLARE + bottom, HEIGHT - bottom, bottom, 270.0, -90.0)
        .line_to(width - FLARE - bottom, HEIGHT)
        .arc(width - FLARE - bottom, HEIGHT - bottom, bottom, 180.0, -90.0)
        .line_to(width - FLARE, FLARE)
        .arc(width, FLARE, FLARE, 270.0, 90.0)
        .close()
        .fill(theme.bar);

    Canvas::new()
        .width(width)
        .height(HEIGHT)
        .shapes(vec![Box::new(path)])
}

fn fixed_width(widget: &impl Widget) -> f32 {
    match widget.width() {
        Size::Fixed(width) => width,
        Size::Parent => 0.0,
    }
}

/*
 * the ovals normally fill the slot; right after a layout switch they crossfade
 * into a chip with the new language, and the slot's fixed width keeps
 * everything to their right where it was
 */
fn language_slot(
    spaces: Row,
    language: &str,
    flash: f32,
    slot_width: f32,
    theme: &Theme,
) -> Stack {
    let mut children: Vec<Box<dyn Widget>> = Vec::new();

    // an invisible child would still take clicks, so it is left out entirely
    if flash < 1.0 {
        let ovals = Rectangle::new()
            .width(slot_width)
            .height(HEIGHT)
            .fill(Color::TRANSPARENT)
            .opacity(1.0 - flash)
            .align_child(Center, Center)
            .child(spaces);

        children.push(Box::new(ovals));
    }

    if flash > 0.0 {
        let badge = Rectangle::new()
            .width(slot_width)
            .height(HEIGHT)
            .fill(Color::TRANSPARENT)
            .opacity(flash)
            .align_child(Center, Center)
            .child(language_badge(language, theme));

        children.push(Box::new(badge));
    }

    Stack::new(children).width(slot_width).height(HEIGHT)
}

fn language_badge(language: &str, theme: &Theme) -> Rectangle {
    Rectangle::new()
        .width(LANGUAGE_WIDTH)
        .height(LANGUAGE_HEIGHT)
        .radius(Full)
        .fill(theme.accent)
        .align_child(Center, Center)
        .child(
            Text::new(String::from(language))
                .size(11.0)
                .color(theme.bar)
                .weight(Weight::Medium),
        )
}

// hours above minutes, both centered on the screen
fn clock_view(clock: &Clock, theme: &Theme) -> Column {
    let label = |text: &str| -> Box<dyn Widget> {
        Box::new(
            Text::new(String::from(text))
                .size(12.0)
                .color(theme.fg)
                .weight(Weight::Medium),
        )
    };

    Column::new(vec![label(clock.hours()), label(clock.minutes())])
        .gap(0.0)
        .align(Center)
}

fn workspace_row(monitor: &Monitor, workspaces: &Workspaces, theme: &Theme) -> Row {
    let mut buttons: Vec<Box<dyn Widget>> = Vec::new();

    for workspace in workspaces.list() {
        if workspace.output() != Some(monitor.name.as_str()) {
            continue;
        }

        let id = workspace.id();
        let focused = workspace.focused();
        let urgent = workspace.urgent();
        let occupied = workspace.windows() > 0;

        // a plain shape says more than a number: it stretches when focused
        let (width, height, fill) = if focused {
            (11.0, 24.0, theme.accent)
        } else if urgent {
            (8.0, 20.0, theme.danger)
        } else if occupied {
            (7.0, 18.0, with_alpha(theme.fg, 95))
        } else {
            (6.0, 14.0, with_alpha(theme.fg, 45))
        };

        buttons.push(Box::new(
            Rectangle::new()
                .width(width)
                .height(height)
                .radius(Full)
                .fill(fill)
                .cursor(Pointer)
                .on_click(move |_| {
                    Workspaces::focus(id);
                    touch();
                }),
        ));
    }

    Row::new(buttons).gap(5.0).align(Center)
}

fn wifi_button(network: &Network, theme: &Theme, open: bool) -> Rectangle {
    let (icon, color) = match network.link() {
        Link::Wifi => (wifi_icon(network.strength()), theme.accent),
        Link::Wired => ("\u{f0200}", theme.accent),
        Link::Other => ("\u{f04a2}", theme.accent),
        Link::Offline => ("\u{f05aa}", theme.muted),
    };

    Rectangle::new()
        .width(26.0)
        .height(24.0)
        .radius(Full)
        .fill(if open {
            theme.surface
        } else {
            Color::TRANSPARENT
        })
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(|_| toggle_wifi())
        .child(
            Text::new(icon)
                .size(15.0)
                .color(color)
                .font(ICON_FONT)
                .tight(),
        )
}

// the percentage lives in the power panel now, so the clock can stay centered
fn battery_button(battery: &Battery, theme: &Theme, open: bool) -> Rectangle {
    let (icon, color) = if !battery.present() {
        ("\u{f0425}", theme.muted)
    } else if battery.charging() {
        (charging_icon(battery.percent()), theme.accent)
    } else if battery.percent() <= 15 {
        ("\u{f0083}", theme.danger)
    } else {
        (battery_icon(battery.percent()), theme.fg)
    };

    Rectangle::new()
        .width(26.0)
        .height(24.0)
        .radius(Full)
        .fill(if open {
            theme.surface
        } else {
            Color::TRANSPARENT
        })
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(|_| toggle_power())
        .child(
            Text::new(icon)
                .size(15.0)
                .color(color)
                .font(ICON_FONT)
                .tight(),
        )
}

// the bell lights up in the accent color while there are unread notifications
// and turns into a crossed bell while Do Not Disturb is on
fn bell_button(history: &History, theme: &Theme, open: bool) -> Rectangle {
    let muted = history.muted();
    let unread = history.unread();

    let (icon, color) = if muted {
        (BELL_OFF, theme.muted)
    } else if unread > 0 {
        (BELL, theme.accent)
    } else {
        (BELL, theme.fg)
    };

    let face = Rectangle::new()
        .width(BELL_WIDTH)
        .height(24.0)
        .align_child(Center, Center)
        .child(
            Text::new(icon)
                .size(15.0)
                .color(color)
                .font(ICON_FONT)
                .tight(),
        );

    let mut layers: Vec<Box<dyn Widget>> = vec![Box::new(face)];

    // a tiny dot in the corner repeats what the color says
    if unread > 0 && !muted {
        layers.push(Box::new(
            Rectangle::new()
                .width(6.0)
                .height(6.0)
                .radius(Full)
                .fill(theme.accent)
                .translate(BELL_WIDTH - 7.0, 1.0),
        ));
    }

    Rectangle::new()
        .width(BELL_WIDTH)
        .height(24.0)
        .radius(Full)
        .fill(if open {
            theme.surface
        } else {
            Color::TRANSPARENT
        })
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(|_| toggle_history())
        .child(Stack::new(layers).width(BELL_WIDTH).height(24.0))
}

fn wifi_icon(strength: u8) -> &'static str {
    match strength {
        0..=20 => "\u{f091f}",
        21..=45 => "\u{f0922}",
        46..=70 => "\u{f0925}",
        _ => "\u{f0928}",
    }
}

fn battery_icon(percent: u8) -> &'static str {
    match percent {
        0..=10 => "\u{f007a}",
        11..=20 => "\u{f007b}",
        21..=30 => "\u{f007c}",
        31..=40 => "\u{f007d}",
        41..=50 => "\u{f007e}",
        51..=60 => "\u{f007f}",
        61..=70 => "\u{f0080}",
        71..=80 => "\u{f0081}",
        81..=90 => "\u{f0082}",
        _ => "\u{f0079}",
    }
}

fn charging_icon(percent: u8) -> &'static str {
    match percent {
        0..=10 => "\u{f089c}",
        11..=20 => "\u{f0086}",
        21..=30 => "\u{f0087}",
        31..=40 => "\u{f0088}",
        41..=50 => "\u{f089d}",
        51..=60 => "\u{f0089}",
        61..=70 => "\u{f089e}",
        71..=80 => "\u{f008a}",
        81..=90 => "\u{f008b}",
        _ => "\u{f0085}",
    }
}
