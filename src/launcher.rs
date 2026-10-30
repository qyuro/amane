use amane::{
    Apps, Center, Color, Column, DesktopApp, Horizontal, Image, Key, Keyboard, Layer, LayerWindow,
    Margin, Padding, Palette, Parent, Pointer, Rectangle, Row, Service, Text, TextInput, Vertical,
    Weight, Widget, Zone,
};

use crate::state::{close_launcher, Ui};
use crate::theme::{with_alpha, Theme};

const ICON_FONT: &str = "JetBrainsMono Nerd Font";

const WIDTH: f32 = 540.0;
const PADDING: f32 = 14.0;
const SEARCH_HEIGHT: f32 = 40.0;
const ROW_HEIGHT: f32 = 38.0;
const ROW_GAP: f32 = 3.0;

// how many programs fit on screen at once, the list follows the selection
const MAX_ROWS: usize = 8;

pub fn view() -> LayerWindow {
    let palette = Palette::read();
    let theme = Theme::from(&palette);

    let ui = Ui::read();
    let apps = Apps::read();

    let fade = ui.launcher.value();

    let found = matches(&apps, &ui.launcher_query);
    let selected = ui.launcher_selected.min(found.len().saturating_sub(1));

    let input = TextInput::new("launcher")
        .placeholder("Найти приложение…")
        .size(15.0)
        .color(theme.fg)
        .focused()
        .on_change(|text| {
            let mut ui = Ui::write();

            ui.launcher_query = text;
            ui.launcher_selected = 0;
        })
        .on_submit(|_| launch_selected());

    let search = Row::new(vec![
        Box::new(
            Text::new("\u{f002}")
                .size(14.0)
                .color(theme.muted)
                .font(ICON_FONT)
                .tight(),
        ) as Box<dyn Widget>,
        Box::new(input),
    ])
    .width(Parent)
    .gap(10.0)
    .align(Center);

    let search = Rectangle::new()
        .width(Parent)
        .height(SEARCH_HEIGHT)
        .radius(10.0)
        .fill(theme.surface)
        .padding(Padding {
            left: 12.0,
            right: 12.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_child(Center, Center)
        .child(search);

    let mut rows: Vec<Box<dyn Widget>> = Vec::new();

    if found.is_empty() {
        rows.push(Box::new(
            Rectangle::new()
                .width(Parent)
                .height(ROW_HEIGHT)
                .align_child(Center, Center)
                .child(
                    Text::new("Ничего не найдено")
                        .size(12.0)
                        .color(theme.muted),
                ),
        ));
    } else {
        for index in visible(selected, found.len()) {
            rows.push(Box::new(app_row(found[index], index == selected, &theme)));
        }
    }

    let rows_height =
        rows.len() as f32 * ROW_HEIGHT + rows.len().saturating_sub(1) as f32 * ROW_GAP;

    let height = PADDING * 2.0 + SEARCH_HEIGHT + 8.0 + rows_height;

    let list = Column::new(rows).width(Parent).gap(ROW_GAP);

    let body = Column::new(vec![Box::new(search) as Box<dyn Widget>, Box::new(list)])
        .width(Parent)
        .gap(8.0);

    let root = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .radius(16.0)
        .fill(theme.panel)
        .border(1.0, theme.border)
        .padding(PADDING)
        .opacity(fade)
        .scale(0.97 + 0.03 * fade)
        .translate(0.0, (1.0 - fade) * -12.0)
        .child(body);

    LayerWindow::new()
        .width(WIDTH)
        .height(height)
        .anchor_vertical(Vertical::Top)
        .anchor_horizontal(Horizontal::Middle)
        .margin(Margin {
            top: 120,
            right: 0,
            bottom: 0,
            left: 0,
        })
        .layer(Layer::Overlay)
        .space(Zone::Ignore)
        .keyboard(Keyboard::Exclusive)
        .visible(ui.launcher_open || fade > 0.0)
        .namespace("amane-launcher")
        .on_key(key)
        .child(root)
}

// programs whose name or description holds the query, in the order the service sorted them
fn matches<'a>(apps: &'a Apps, query: &str) -> Vec<&'a DesktopApp> {
    let query = query.trim().to_lowercase();

    apps.list()
        .iter()
        .filter(|app| {
            query.is_empty()
                || app.name().to_lowercase().contains(&query)
                || app
                    .description()
                    .is_some_and(|text| text.to_lowercase().contains(&query))
        })
        .collect()
}

// the rows to show, moved along so the selected program is always one of them
fn visible(selected: usize, count: usize) -> std::ops::Range<usize> {
    if count <= MAX_ROWS {
        return 0..count;
    }

    let start = selected
        .saturating_sub(MAX_ROWS / 2)
        .min(count - MAX_ROWS);

    start..start + MAX_ROWS
}

fn app_row(app: &DesktopApp, selected: bool, theme: &Theme) -> Rectangle {
    let launched = app.clone();

    let icon = match app.icon_path() {
        Some(path) => Rectangle::new()
            .width(22.0)
            .height(22.0)
            .fill(Image::contain(path.to_path_buf())),
        None => Rectangle::new()
            .width(22.0)
            .height(22.0)
            .radius(5.0)
            .fill(with_alpha(theme.fg, 18)),
    };

    let mut labels: Vec<Box<dyn Widget>> = vec![Box::new(
        Text::new(app.name())
            .size(13.0)
            .color(theme.fg)
            .weight(if selected {
                Weight::SemiBold
            } else {
                Weight::Regular
            })
            .elide(),
    )];

    if let Some(description) = app.description() {
        labels.push(Box::new(
            Text::new(description)
                .size(10.0)
                .color(theme.muted)
                .elide(),
        ));
    }

    let labels = Column::new(labels).width(Parent).gap(1.0);

    let contents = Row::new(vec![Box::new(icon) as Box<dyn Widget>, Box::new(labels)])
        .width(Parent)
        .gap(10.0)
        .align(Center);

    let fill = if selected {
        with_alpha(theme.accent, 38)
    } else {
        Color::TRANSPARENT
    };

    Rectangle::new()
        .width(Parent)
        .height(ROW_HEIGHT)
        .radius(9.0)
        .fill(fill)
        .padding(Padding {
            left: 10.0,
            right: 10.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_child(Center, Center)
        .cursor(Pointer)
        .on_click(move |_| {
            launched.launch();
            close_launcher();
        })
        .child(contents)
}

fn key(key: Key) {
    match key {
        Key::Escape => close_launcher(),
        Key::Up | Key::Down => move_selection(key == Key::Down),
        _ => {}
    }
}

fn move_selection(down: bool) {
    let count = {
        let ui = Ui::read();
        let apps = Apps::read();

        matches(&apps, &ui.launcher_query).len()
    };

    if count == 0 {
        return;
    }

    let mut ui = Ui::write();
    let selected = ui.launcher_selected.min(count - 1);

    ui.launcher_selected = if down {
        (selected + 1).min(count - 1)
    } else {
        selected.saturating_sub(1)
    };
}

// Enter, or a click on a row, starts the program under the selection
fn launch_selected() {
    let chosen = {
        let ui = Ui::read();
        let apps = Apps::read();

        let found = matches(&apps, &ui.launcher_query);
        let selected = ui.launcher_selected.min(found.len().saturating_sub(1));

        found.get(selected).map(|app| (*app).clone())
    };

    let Some(app) = chosen else {
        return;
    };

    app.launch();

    close_launcher();
}
