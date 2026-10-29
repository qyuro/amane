use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use amane::{output, Color, Palette, Service};

/// Watches the wallpaper shown by awww and rebuilds the palette when it changes.
///
/// The palette itself is the normal `Palette` service, so every view that reads
/// it is redrawn with the new colors automatically.
pub struct Wallpaper {
    path: Option<String>,
    ticks: u32,
}

impl Service for Wallpaper {
    fn new() -> Self {
        let path = current_wallpaper();

        apply(path.as_deref());

        Self { path, ticks: 0 }
    }

    fn interval() -> Duration {
        Duration::from_secs(5)
    }

    fn update(&mut self) -> bool {
        self.ticks += 1;

        // the cache is cheap to read; ask awww itself once a minute in case it did
        // not rewrite the cache file
        let path = if self.ticks % 12 == 0 {
            from_awww_query().or_else(from_awww_cache)
        } else {
            from_awww_cache()
        };

        if path == self.path {
            return false;
        }

        apply(path.as_deref());
        self.path = path;

        false
    }
}

fn apply(path: Option<&str>) {
    let Some(path) = path else {
        return;
    };

    Palette::write().open(path, 16);
}

fn current_wallpaper() -> Option<String> {
    if let Ok(path) = env::var("AMANE_WALLPAPER") {
        let path = path.trim();

        if !path.is_empty() {
            return Some(String::from(path));
        }
    }

    from_awww_cache().or_else(from_awww_query)
}

fn from_awww_cache() -> Option<String> {
    let home = env::var("HOME").ok()?;
    let folder = PathBuf::from(home).join(".cache/awww");

    let mut fallback = None;

    for version in fs::read_dir(folder).ok()?.flatten() {
        let folder = version.path();

        if !folder.is_dir() {
            continue;
        }

        for entry in fs::read_dir(folder).ok()?.flatten() {
            let file = entry.path();
            let Ok(text) = fs::read_to_string(&file) else {
                continue;
            };

            let text = text.trim();

            if !text.starts_with('/') || !PathBuf::from(text).exists() {
                continue;
            }

            // the monitor this laptop uses, if awww has one entry per output
            if file.file_name().and_then(|name| name.to_str()) == Some("eDP-1") {
                return Some(String::from(text));
            }

            fallback.get_or_insert_with(|| String::from(text));
        }
    }

    fallback
}

fn from_awww_query() -> Option<String> {
    let text = output("awww query 2>/dev/null");

    for line in text.lines() {
        let Some((_, image)) = line.split_once("currently displaying: image: ") else {
            continue;
        };

        let image = image.trim();

        if !image.is_empty() {
            return Some(String::from(image));
        }
    }

    None
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub bar: Color,
    pub panel: Color,
    pub fg: Color,
    pub muted: Color,
    pub accent: Color,
    pub surface: Color,
    pub border: Color,
    pub danger: Color,
}

impl Theme {
    pub fn from(palette: &Palette) -> Self {
        let background = palette.background();
        let foreground = palette.foreground();
        let accent = palette.accent();

        Self {
            bar: with_alpha(background, 226),
            panel: with_alpha(background, 246),
            fg: foreground,
            muted: with_alpha(foreground, 155),
            accent,
            surface: with_alpha(foreground, 22),
            border: with_alpha(accent, 95),
            danger: Color::rgb(0xf3, 0x8b, 0xa8),
        }
    }
}

pub fn with_alpha(color: Color, alpha: u8) -> Color {
    Color::rgba(color.red(), color.green(), color.blue(), alpha)
}
