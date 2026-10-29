use amane::{
    AccessPoint, Align, Battery, Button, Center, Color, Column, Horizontal, Key, Keyboard, Layer,
    LayerWindow, Link, Margin, Network, Padding, Palette, Parent, Pointer, Rectangle, Row,
    ScrollArea, Service, Text, TextInput, Vertical, Weight, Widget, Zone,
};

use crate::power::{Power, PowerMode};
use crate::state::{close_power, close_wifi, Ui};
use crate::theme::{with_alpha, Theme};

const ICON_FONT: &str = "JetBrainsMono Nerd Font";

pub fn wifi() -> LayerWindow {
    let palette = Palette::read();
    let theme = Theme::from(&palette);

    let ui = Ui::read();
    let network = Network::read();

    let mut content: Vec<Box<dyn Widget>> = Vec::new();

    content.push(Box::new(header(
        "Wi-Fi",
        &theme,
        vec![
            Box::new(icon_button("\u{f0450}", &theme, |_| Network::scan())),
            Box::new(icon_button("\u{f0156}", &theme, |_| close_wifi())),
        ],
    )));

    if network.wifi_enabled() {
        content.push(Box::new(network_list(&network, &ui, &theme)));

        if !ui.selected.is_empty() {
            content.push(Box::new(password_field(&ui.selected, &theme)));
        }

        content.push(Box::new(connection_footer(&network, &theme)));
    } else {
        content.push(Box::new(
            Text::new("Wi-Fi выключен")
                .size(13.0)
                .color(theme.muted),
        ));
        content.push(Box::new(wide_button("Включить Wi-Fi", &theme, |_| {
            Network::set_wifi(true);
            Network::scan();
        })));
    }

    let body = Column::new(content).width(Parent).gap(10.0);

    // the disabled state has only two rows, so it does not need the full list height
    let height = if network.wifi_enabled() { 420.0 } else { 190.0 };

    let root = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .radius(18.0)
        .fill(theme.panel)
        .border(1.0, theme.border)
        .padding(16.0)
        .child(body);

    LayerWindow::new()
        .width(380.0)
        .height(height)
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
        .visible(ui.wifi_open)
        .namespace("amane-wifi")
        .on_key(|key| {
            if key == Key::Escape {
                close_wifi();
            }
        })
        .child(root)
}

pub fn power() -> LayerWindow {
    let palette = Palette::read();
    let theme = Theme::from(&palette);

    let ui = Ui::read();
    let power = PowerMode::read();
    let battery = Battery::read();

    let mut content: Vec<Box<dyn Widget>> = Vec::new();

    content.push(Box::new(header(
        "Режим питания",
        &theme,
        vec![Box::new(icon_button("\u{f0156}", &theme, |_| {
            close_power()
        }))],
    )));

    let battery_text = if battery.present() {
        let state = if battery.charging() {
            " · заряжается"
        } else {
            ""
        };

        format!("Батарея {}%{state}", battery.percent())
    } else {
        String::from("Питание от сети")
    };

    content.push(Box::new(
        Text::new(battery_text).size(12.0).color(theme.muted),
    ));

    for mode in Power::ALL {
        content.push(Box::new(mode_row(mode, power.mode() == mode, &theme)));
    }

    content.push(Box::new(
        Text::new(format!("tuned: {}", power.profile()))
            .size(10.5)
            .color(theme.muted),
    ));

    let body = Column::new(content).width(Parent).gap(8.0);

    let root = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .radius(18.0)
        .fill(theme.panel)
        .border(1.0, theme.border)
        .padding(16.0)
        .child(body);

    LayerWindow::new()
        .width(330.0)
        .height(278.0)
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
        .visible(ui.power_open)
        .namespace("amane-power")
        .on_key(|key| {
            if key == Key::Escape {
                close_power();
            }
        })
        .child(root)
}

fn header(title: &str, theme: &Theme, controls: Vec<Box<dyn Widget>>) -> Row {
    let mut items: Vec<Box<dyn Widget>> = Vec::new();

    items.push(Box::new(
        Text::new(title)
            .size(16.0)
            .color(theme.fg)
            .weight(Weight::SemiBold),
    ));

    // pushes the buttons to the right edge
    items.push(Box::new(
        Rectangle::new()
            .width(Parent)
            .height(1.0)
            .fill(Color::TRANSPARENT),
    ));

    items.extend(controls);

    Row::new(items).width(Parent).gap(6.0).align(Center)
}

fn icon_button(
    icon: &'static str,
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
                .color(theme.fg)
                .font(ICON_FONT)
                .tight(),
        )
}

fn wide_button(
    label: &str,
    theme: &Theme,
    clicked: impl Fn(Button) + 'static,
) -> Rectangle {
    Rectangle::new()
        .width(Parent)
        .height(34.0)
        .radius(10.0)
        .fill(theme.surface)
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(clicked)
        .child(Text::new(label).size(13.0).color(theme.fg))
}

fn small_button(
    label: &str,
    theme: &Theme,
    clicked: impl Fn(Button) + 'static,
) -> Rectangle {
    Rectangle::new()
        .width(92.0)
        .height(26.0)
        .radius(8.0)
        .fill(theme.surface)
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(clicked)
        .child(Text::new(label).size(11.0).color(theme.fg))
}

fn network_list(network: &Network, ui: &Ui, theme: &Theme) -> ScrollArea {
    let mut access_points: Vec<&AccessPoint> = network.access_points().iter().collect();

    access_points.sort_by(|first, second| {
        second
            .active()
            .cmp(&first.active())
            .then_with(|| second.strength().cmp(&first.strength()))
            .then_with(|| first.ssid().cmp(second.ssid()))
    });

    let mut rows: Vec<Box<dyn Widget>> = Vec::new();

    for access_point in access_points {
        rows.push(Box::new(access_row(access_point, ui, theme)));
    }

    if rows.is_empty() {
        rows.push(Box::new(
            Text::new("Сети не найдены")
                .size(12.0)
                .color(theme.muted),
        ));
    }

    ScrollArea::new(
        "wifi-list",
        Column::new(rows).width(Parent).gap(3.0),
    )
    .height(230.0)
}

fn access_row(access_point: &AccessPoint, ui: &Ui, theme: &Theme) -> Rectangle {
    let ssid = String::from(access_point.ssid());
    let active = access_point.active();
    let secured = access_point.secured();
    let saved = access_point.saved();
    let strength = access_point.strength();
    let selected = ui.selected == ssid;

    let fill = if active {
        with_alpha(theme.accent, 55)
    } else if selected {
        theme.surface
    } else {
        Color::TRANSPARENT
    };

    let icon = if active {
        "\u{f012c}" // check
    } else if secured {
        "\u{f033e}" // lock
    } else {
        wifi_icon(strength)
    };

    let icon_color = if active { theme.accent } else { theme.muted };

    let label = if ssid.is_empty() {
        String::from("(скрытая сеть)")
    } else {
        ssid.clone()
    };

    let mut items: Vec<Box<dyn Widget>> = vec![
        Box::new(
            Text::new(icon)
                .size(13.0)
                .color(icon_color)
                .font(ICON_FONT)
                .tight(),
        ),
        Box::new(
            Text::new(label)
                .size(13.0)
                .color(theme.fg)
                .weight(if active {
                    Weight::SemiBold
                } else {
                    Weight::Regular
                })
                .elide(),
        ),
        Box::new(
            Text::new(format!("{strength}%"))
                .size(10.5)
                .color(theme.muted),
        ),
    ];

    let contents = Row::new(std::mem::take(&mut items))
        .width(Parent)
        .gap(8.0)
        .align(Center);

    Rectangle::new()
        .width(Parent)
        .height(32.0)
        .radius(9.0)
        .fill(fill)
        .padding(Padding {
            left: 8.0,
            right: 8.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(move |_| choose_network(ssid.clone(), active, secured, saved))
        .child(contents)
}

fn choose_network(ssid: String, active: bool, secured: bool, saved: bool) {
    if active {
        close_wifi();
        return;
    }

    if secured && !saved {
        TextInput::set_text("wifi-password", "");

        let mut ui = Ui::write();

        ui.selected = ssid;
        ui.wifi_open = true;
        ui.power_open = false;

        return;
    }

    Network::connect(&ssid, None);
    close_wifi();
}

fn password_field(selected: &str, theme: &Theme) -> Column {
    let ssid = String::from(selected);
    let hint = Text::new(format!("Пароль для {selected}"))
        .size(10.5)
        .color(theme.muted)
        .elide();

    let field = Rectangle::new()
        .width(Parent)
        .height(34.0)
        .radius(9.0)
        .fill(theme.surface)
        .padding(Padding {
            left: 10.0,
            right: 10.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_child(Center, Center)
        .child(
            TextInput::new("wifi-password")
                .placeholder("пароль")
                .password()
                .size(13.0)
                .color(theme.fg)
                .on_submit(move |password| submit_password(&ssid, &password)),
        );

    Column::new(vec![
        Box::new(hint) as Box<dyn Widget>,
        Box::new(field),
    ])
    .width(Parent)
    .gap(4.0)
}

fn submit_password(ssid: &str, password: &str) {
    if ssid.is_empty() || password.is_empty() {
        return;
    }

    Network::connect(ssid, Some(password));

    TextInput::set_text("wifi-password", "");

    let mut ui = Ui::write();

    ui.selected.clear();
    ui.wifi_open = false;
}

fn connection_footer(network: &Network, theme: &Theme) -> Row {
    let label = match network.link() {
        Link::Wifi => format!("{} · {}%", network.ssid(), network.strength()),
        Link::Wired => String::from("Ethernet"),
        Link::Other => String::from("Подключено"),
        Link::Offline => String::from("Нет подключения"),
    };

    let mut items: Vec<Box<dyn Widget>> = vec![Box::new(
        Text::new(label)
            .size(10.5)
            .color(theme.muted)
            .elide(),
    )];

    if network.connected() {
        items.push(Box::new(small_button("Отключить", theme, |_| {
            Network::disconnect()
        })));
    }

    Row::new(items).width(Parent).gap(8.0).align(Center)
}

fn mode_row(mode: Power, active: bool, theme: &Theme) -> Rectangle {
    let fill = if active {
        with_alpha(theme.accent, 65)
    } else {
        theme.surface
    };

    let border = if active {
        theme.accent
    } else {
        Color::TRANSPARENT
    };

    let labels = Column::new(vec![
        Box::new(
            Text::new(mode.label())
                .size(13.0)
                .color(theme.fg)
                .weight(if active {
                    Weight::SemiBold
                } else {
                    Weight::Regular
                }),
        ) as Box<dyn Widget>,
        Box::new(Text::new(mode.hint()).size(10.0).color(theme.muted)),
    ])
    .gap(1.0)
    .align(Align::Start);

    let mut items: Vec<Box<dyn Widget>> = vec![
        Box::new(
            Text::new(mode.icon())
                .size(16.0)
                .color(if active { theme.accent } else { theme.muted })
                .font(ICON_FONT)
                .tight(),
        ),
        Box::new(labels),
        Box::new(
            Rectangle::new()
                .width(Parent)
                .height(1.0)
                .fill(Color::TRANSPARENT),
        ),
        Box::new(
            Text::new(if active { "\u{f012c}" } else { "" })
                .size(14.0)
                .color(theme.accent)
                .font(ICON_FONT)
                .tight(),
        ),
    ];

    let contents = Row::new(std::mem::take(&mut items))
        .width(Parent)
        .gap(8.0)
        .align(Center);

    Rectangle::new()
        .width(Parent)
        .height(48.0)
        .radius(11.0)
        .fill(fill)
        .border(1.0, border)
        .padding(Padding {
            left: 10.0,
            right: 10.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(move |_| {
            PowerMode::set(mode);
            close_power();
        })
        .child(contents)
}

fn wifi_icon(strength: u8) -> &'static str {
    match strength {
        0..=20 => "\u{f091f}",
        21..=45 => "\u{f0922}",
        46..=70 => "\u{f0925}",
        _ => "\u{f0928}",
    }
}
