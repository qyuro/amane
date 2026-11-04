use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use amane::{
    Button, Center, Color, Column, Horizontal, Image, Key, Keyboard, Layer, LayerWindow, Margin,
    Palette, Parent, Padding, Pointer, Rectangle, Row, Service, Start, Text, Vertical, Weight,
    Widget, Zone,
};

use crate::state::{close_wallpapers, set_wallpaper_page, Ui};
use crate::theme::{with_alpha, Theme};

const ICON_FONT: &str = "JetBrainsMono Nerd Font";

const WIDTH: f32 = 460.0;
const HEIGHT: f32 = 410.0;
const PADDING: f32 = 16.0;

// three wallpapers a row and three rows a page; the pages keep the thumbnails
// big enough to recognise the picture before clicking it
const COLUMNS: usize = 3;
const ROWS: usize = 3;
const PAGE: usize = COLUMNS * ROWS;

const CARD_WIDTH: f32 = 132.0;
const CARD_HEIGHT: f32 = 94.0;
const PREVIEW_HEIGHT: f32 = 70.0;
const GRID_GAP: f32 = 10.0;

const THUMB_WIDTH: u32 = 320;
const THUMB_HEIGHT: u32 = 180;

const FOLDER: &str = "Wall";

/// One wallpaper file, with the cached picture shown in the picker.
pub struct Wallpaper {
    path: PathBuf,
    thumb: PathBuf,

    // canonical path, so the awww cache and the scan compare the same file
    key: String,

    name: String,
}

impl Wallpaper {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn thumb(&self) -> &Path {
        &self.thumb
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Every wallpaper under `~/Wall`, and the one awww is showing.
pub struct Wallpapers {
    entries: Vec<Wallpaper>,
    current: Option<String>,

    // bumped when a thumbnail is ready, so the picker draws it
    revision: u32,
}

impl Service for Wallpapers {
    fn new() -> Self {
        let home = env::var("HOME").ok().map(PathBuf::from);
        let root = home.as_ref().map(|home| home.join(FOLDER));
        let cache = home.as_ref().map(|home| home.join(".cache/amane/wallpapers"));

        let mut entries = Vec::new();

        if let (Some(root), Some(cache)) = (root.as_ref(), cache.as_ref()) {
            if fs::create_dir_all(cache).is_ok() {
                walk(root, cache, 0, &mut entries);
            }
        }

        entries.sort_by(|first, second| {
            first
                .name
                .to_lowercase()
                .cmp(&second.name.to_lowercase())
                .then_with(|| first.key.cmp(&second.key))
        });

        let current = crate::theme::current_wallpaper().map(|path| canonical(&PathBuf::from(path)));

        Self {
            entries,
            current,
            revision: 0,
        }
    }

    fn interval() -> Duration {
        Duration::from_secs(4)
    }

    // the highlight follows awww when the wallpaper changes without the picker
    fn update(&mut self) -> bool {
        let current = crate::theme::current_wallpaper().map(|path| canonical(&PathBuf::from(path)));

        if current == self.current {
            return false;
        }

        self.current = current;

        true
    }
}

impl Wallpapers {
    pub fn entries(&self) -> &[Wallpaper] {
        &self.entries
    }

    pub fn revision(&self) -> u32 {
        self.revision
    }

    fn is_current(&self, wallpaper: &Wallpaper) -> bool {
        self.current.as_deref() == Some(wallpaper.key.as_str())
    }

    // starts making the missing thumbnails of the page on screen
    fn ensure_page(&self, page: usize) {
        for wallpaper in self.entries.iter().skip(page * PAGE).take(PAGE) {
            if wallpaper.thumb.exists() {
                continue;
            }

            queue(&wallpaper.path, &wallpaper.thumb);
        }
    }

    fn touch() {
        let mut wallpapers = Wallpapers::write();

        wallpapers.revision = wallpapers.revision.wrapping_add(1);
    }
}

/// Reads a folder of wallpapers, skipping the files the image decoder and the
/// thumbnailer have no use for.
fn walk(folder: &Path, cache: &Path, depth: usize, entries: &mut Vec<Wallpaper>) {
    // a wallpaper collection does not need deep recursion, and a symlink loop
    // would otherwise keep the scan going forever
    if depth > 4 {
        return;
    }

    let Ok(files) = fs::read_dir(folder) else {
        return;
    };

    for file in files.flatten() {
        let path = file.path();

        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some(".git") {
                continue;
            }

            walk(&path, cache, depth + 1, entries);

            continue;
        }

        if !supported(&path) {
            continue;
        }

        let Some(name) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };

        entries.push(Wallpaper {
            thumb: thumbnail_path(cache, &path),
            key: canonical(&path),
            name: String::from(name),
            path,
        });
    }
}

fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "svg"
            )
        })
}

fn canonical(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

// the path and the file's modification time in one short name, so a replaced
// wallpaper gets a new thumbnail and the old one is not shown for it
fn thumbnail_path(cache: &Path, source: &Path) -> PathBuf {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;

    let mut mix = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };

    mix(source.to_string_lossy().as_bytes());

    if let Ok(modified) = fs::metadata(source).and_then(|metadata| metadata.modified()) {
        if let Ok(since) = modified.duration_since(UNIX_EPOCH) {
            mix(&since.as_nanos().to_le_bytes());
        }
    }

    cache.join(format!("{hash:016x}.png"))
}

// one worker builds the thumbnails in the order the pages ask for them, so
// opening the picker never waits for 200 pictures to be converted
fn queue(source: &Path, thumb: &Path) {
    static QUEUED: LazyLock<Mutex<HashSet<PathBuf>>> =
        LazyLock::new(|| Mutex::new(HashSet::new()));

    {
        let mut queued = QUEUED.lock().unwrap_or_else(|error| error.into_inner());

        if !queued.insert(thumb.to_path_buf()) {
            return;
        }
    }

    let _ = sender().send((source.to_path_buf(), thumb.to_path_buf()));
}

fn sender() -> &'static mpsc::Sender<(PathBuf, PathBuf)> {
    static SENDER: OnceLock<mpsc::Sender<(PathBuf, PathBuf)>> = OnceLock::new();

    SENDER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<(PathBuf, PathBuf)>();

        thread::spawn(move || {
            for (source, thumb) in receiver {
                make(&source, &thumb);
                Wallpapers::touch();
            }
        });

        sender
    })
}

fn make(source: &Path, thumb: &Path) {
    // magick picks the format from the extension, so the partial file ends in .png
    let temporary = thumb.with_extension("part.png");

    let done = Command::new("magick")
        .arg(source)
        .args([
            "-auto-orient",
            "-thumbnail",
            "320x180^",
            "-gravity",
            "center",
            "-extent",
            "320x180",
            "-strip",
        ])
        .arg(&temporary)
        .status()
        .is_ok_and(|status| status.success());

    if done {
        let _ = fs::rename(&temporary, thumb);
    } else {
        let _ = fs::remove_file(&temporary);
    }
}

/// Uses the picture as the wallpaper and rebuilds the shell colors from it.
pub fn apply(path: &Path) {
    let file = path.to_string_lossy().into_owned();

    let _ = Command::new("awww")
        .args([
            "img",
            &file,
            "--transition-type",
            "grow",
            "--transition-duration",
            "0.5",
        ])
        .spawn();

    {
        let mut wallpapers = Wallpapers::write();

        wallpapers.current = Some(canonical(path));
    }

    // decoding the wallpaper must not hold up the click that chose it
    let path = path.to_path_buf();

    thread::spawn(move || {
        Palette::write().open(&path.to_string_lossy(), 16);
    });
}

pub fn view() -> LayerWindow {
    let palette = Palette::read();
    let theme = Theme::from(&palette);

    let ui = Ui::read();
    let wallpapers = Wallpapers::read();

    let fade = ui.wallpaper.value();

    let entries = wallpapers.entries();
    let pages = entries.len().div_ceil(PAGE).max(1);
    let page = ui.wallpaper_page.min(pages - 1);

    // this page's pictures are the ones worth converting right now
    wallpapers.ensure_page(page);

    // read the counter so a finished thumbnail wakes this window
    let _ = wallpapers.revision();

    let mut rows: Vec<Box<dyn Widget>> = Vec::new();

    let page_entries: Vec<&Wallpaper> = entries.iter().skip(page * PAGE).take(PAGE).collect();

    for chunk in page_entries.chunks(COLUMNS) {
        let mut cards: Vec<Box<dyn Widget>> = chunk
            .iter()
            .map(|wallpaper| {
                Box::new(card(wallpaper, wallpapers.is_current(wallpaper), &theme))
                    as Box<dyn Widget>
            })
            .collect();

        // keep the last row aligned with the ones above it
        while cards.len() < COLUMNS {
            cards.push(Box::new(
                Rectangle::new()
                    .width(CARD_WIDTH)
                    .height(CARD_HEIGHT)
                    .fill(Color::TRANSPARENT),
            ));
        }

        rows.push(Box::new(
            Row::new(cards)
                .width(Parent)
                .gap(GRID_GAP)
                .justify(Center),
        ));
    }

    for _ in rows.len()..ROWS {
        rows.push(Box::new(
            Rectangle::new()
                .width(Parent)
                .height(CARD_HEIGHT)
                .fill(Color::TRANSPARENT),
        ));
    }

    let grid = Column::new(rows)
        .width(Parent)
        .gap(GRID_GAP)
        .align(Center);

    let body = Column::new(vec![
        Box::new(header(&theme)) as Box<dyn Widget>,
        Box::new(grid),
        Box::new(footer(page, pages, &theme)),
    ])
    .width(Parent)
    .gap(10.0);

    let root = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .radius(18.0)
        .fill(theme.panel)
        .border(1.0, theme.border)
        .padding(Padding {
            left: PADDING,
            right: PADDING,
            top: PADDING,
            bottom: PADDING,
        })
        .opacity(fade)
        .scale(0.98 + 0.02 * fade)
        .translate(0.0, (1.0 - fade) * -10.0)
        .child(body);

    LayerWindow::new()
        .width(WIDTH)
        .height(HEIGHT)
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
        .visible(ui.wallpaper_open || fade > 0.0)
        .namespace("amane-wallpapers")
        .on_key(|key| {
            if key == Key::Escape {
                close_wallpapers();
            }
        })
        .child(root)
}

fn card(wallpaper: &Wallpaper, current: bool, theme: &Theme) -> Rectangle {
    let path = wallpaper.path().to_path_buf();
    let name = String::from(wallpaper.name());
    let thumb = wallpaper.thumb().to_path_buf();

    // asking the image loader for a file that does not exist yet makes it
    // remember the failure, so the placeholder is drawn until the file is there
    let preview: Box<dyn Widget> = if thumb.exists() {
        Box::new(
            Rectangle::new()
                .width(Parent)
                .height(PREVIEW_HEIGHT)
                .radius(6.0)
                .fill(
                    Image::cover(thumb)
                        .thumbnail(THUMB_WIDTH, THUMB_HEIGHT),
                )
                .clip(),
        )
    } else {
        Box::new(
            Rectangle::new()
                .width(Parent)
                .height(PREVIEW_HEIGHT)
                .radius(6.0)
                .fill(with_alpha(theme.fg, 14))
                .align_child(Center, Center)
                .child(
                    Text::new("\u{f03e}")
                        .size(18.0)
                        .color(theme.muted)
                        .font(ICON_FONT)
                        .tight(),
                ),
        )
    };

    let label = Text::new(name)
        .size(10.5)
        .color(if current { theme.accent } else { theme.muted })
        .weight(if current {
            Weight::Medium
        } else {
            Weight::Regular
        })
        .elide();

    let contents = Column::new(vec![preview, Box::new(label)])
        .width(Parent)
        .gap(3.0)
        .align(Start);

    Rectangle::new()
        .width(CARD_WIDTH)
        .height(CARD_HEIGHT)
        .radius(9.0)
        .fill(with_alpha(theme.fg, 10))
        .border(
            if current { 2.0 } else { 1.0 },
            if current {
                theme.accent
            } else {
                with_alpha(theme.fg, 18)
            },
        )
        .padding(Padding {
            left: 4.0,
            right: 4.0,
            top: 4.0,
            bottom: 4.0,
        })
        .align_child(Start, Start)
        .cursor(Pointer)
        .on_click(move |_| apply(&path))
        .child(contents)
}

fn header(theme: &Theme) -> Row {
    Row::new(vec![
        Box::new(
            Text::new("Обои")
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
        Box::new(icon_button("\u{f0156}", theme, |_| close_wallpapers())),
    ])
    .width(Parent)
    .gap(6.0)
    .align(Center)
}

fn footer(page: usize, pages: usize, theme: &Theme) -> Row {
    let label = Text::new(format!("{} / {}", page + 1, pages))
        .size(11.0)
        .color(theme.muted);

    let mut items: Vec<Box<dyn Widget>> = vec![Box::new(
        Rectangle::new()
            .width(Parent)
            .height(1.0)
            .fill(Color::TRANSPARENT),
    )];

    items.push(Box::new(icon_button(
        "\u{f0141}",
        theme,
        move |_| set_wallpaper_page(page.saturating_sub(1)),
    )));
    items.push(Box::new(label));
    items.push(Box::new(icon_button(
        "\u{f0142}",
        theme,
        move |_| set_wallpaper_page((page + 1).min(pages - 1)),
    )));

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
