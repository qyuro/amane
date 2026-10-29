use amane::{
    Battery, Center, Color, Column, Full, Horizontal, InputArea, Layer, LayerWindow, Link, Margin,
    Monitor, Network, Palette, Parent, Padding, Pointer, Rectangle, Row, Service, Size, Text,
    Vertical, Weight, Widget, Workspaces, Zone,
};

use crate::clock::Clock;
use crate::state::{toggle_power, toggle_wifi, Ui};
use crate::theme::{with_alpha, Theme};

const ICON_FONT: &str = "JetBrainsMono Nerd Font";

pub fn view(monitor: &Monitor) -> LayerWindow {
    let palette = Palette::read();
    let theme = Theme::from(&palette);

    let ui = Ui::read();
    let clock = Clock::read();
    let network = Network::read();
    let battery = Battery::read();
    let workspaces = Workspaces::read();

    let spaces = workspace_row(monitor, &workspaces, &theme);
    let clock = clock_view(&clock, &theme);

    let right = Row::new(vec![
        Box::new(wifi_button(&network, &theme, ui.wifi_open)),
        Box::new(battery_button(&battery, &theme, ui.power_open)),
    ])
    .gap(9.0)
    .align(Center);

    // the clock has to sit on the middle of the screen, so the narrower side
    // gets its free space back by shifting the whole pill
    let spaces_width = fixed_width(&spaces);
    let right_width = fixed_width(&right);

    let content = Row::new(vec![
        Box::new(spaces) as Box<dyn Widget>,
        Box::new(clock),
        Box::new(right),
    ])
    .gap(9.0)
    .align(Center);

    // the pill is only as wide as its contents, and the rest of the window stays clear
    let pill_width = fixed_width(&content) + 28.0;

    /*
     * the bubble hangs from the top edge with its corners rounded back into
     * the screen, so it reads as if the bar is flowing out of it
     */
    let pill = Rectangle::new()
        .width(pill_width)
        .height(34.0)
        .radius_top_left(12.0)
        .radius_top_right(12.0)
        .radius_bottom_left(18.0)
        .radius_bottom_right(18.0)
        .fill(theme.bar)
        .padding(Padding {
            left: 14.0,
            right: 14.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_child(Center, Center)
        .child(content);

    let centered_left = ((monitor.width as f32 - pill_width) / 2.0).max(0.0);
    let shift = (right_width - spaces_width) / 2.0;
    let pill_left = (centered_left + shift).max(0.0);

    // the pill is drawn centered in the root, so the root keeps half the shift
    // as padding on one side
    let offset = (pill_left - centered_left) * 2.0;

    let root = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .fill(Color::TRANSPARENT)
        .padding(Padding {
            left: offset.max(0.0),
            right: (-offset).max(0.0),
            top: 0.0,
            bottom: 0.0,
        })
        .align_child(Center, Center)
        .child(pill);

    /*
     * a full-width surface with a transparent middle would still swallow clicks,
     * so only the pill itself is part of the input region
     */
    let input_x = (pill_left - 2.0).max(0.0).round() as i32;
    let input_width = (pill_width + 4.0).round() as i32;

    LayerWindow::new()
        .width(Full)
        .height(34.0)
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
        .visible(!ui.bar_hidden)
        .input_region(vec![InputArea {
            x: input_x,
            y: 0,
            width: input_width,
            height: 34,
        }])
        .child(root)
}

fn fixed_width(widget: &impl Widget) -> f32 {
    match widget.width() {
        Size::Fixed(width) => width,
        Size::Parent => 0.0,
    }
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
                .on_click(move |_| Workspaces::focus(id)),
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

    let label = if battery.present() {
        format!("{}%", battery.percent())
    } else {
        String::from("AC")
    };

    let content = Row::new(vec![
        Box::new(
            Text::new(icon)
                .size(15.0)
                .color(color)
                .font(ICON_FONT)
                .tight(),
        ) as Box<dyn Widget>,
        Box::new(
            Text::new(label)
                .size(12.0)
                .color(theme.fg)
                .weight(Weight::Medium),
        ),
    ])
    .gap(4.0)
    .align(Center);

    Rectangle::new()
        .width(56.0)
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
        .child(content)
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
