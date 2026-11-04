use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};

use amane::{
    Center, Color, Column, End, Horizontal, Layer, LayerWindow, Margin, Palette, Parent, Rectangle,
    Row, Service, Start, Text, Vertical, Weight, Widget, Zone,
};

use crate::theme::{with_alpha, Theme};

const WIDTH: f32 = 240.0;
const PADDING: f32 = 14.0;
const ROW_HEIGHT: f32 = 18.0;
const GAP: f32 = 7.0;
const HEIGHT: f32 = PADDING * 2.0 + ROW_HEIGHT * 5.0 + GAP * 4.0;

const LABEL_WIDTH: f32 = 32.0;
const VALUE_WIDTH: f32 = 62.0;
const BAR_WIDTH: f32 = WIDTH - PADDING * 2.0 - LABEL_WIDTH - VALUE_WIDTH - GAP * 2.0;

// bars for the network fill up at a 100 Mbit link, the usual ceiling at home
const NET_FULL: f32 = 12_500_000.0;

const POLL: Duration = Duration::from_millis(50);
const NET_SAMPLE: Duration = Duration::from_millis(500);
const DISK_SAMPLE: Duration = Duration::from_secs(5);

/// Network throughput, disk use, and eased copies of the CPU and memory loads
/// for the desktop widget.
pub struct Load {
    cpu: f32,
    memory: f32,
    disk: f32,

    down: f32,
    up: f32,

    disk_used: u64,
    disk_total: u64,

    interface: String,
    rx: u64,
    tx: u64,
    sampled: Instant,
    disk_sampled: Instant,

    target_down: f32,
    target_up: f32,
}

impl Service for Load {
    fn new() -> Self {
        let (interface, rx, tx) = network().unwrap_or_default();
        let (disk_used, disk_total) = disk();

        Self {
            cpu: 0.0,
            memory: 0.0,
            disk: percent(disk_used, disk_total),
            down: 0.0,
            up: 0.0,
            disk_used,
            disk_total,
            interface,
            rx,
            tx,
            sampled: Instant::now(),
            disk_sampled: Instant::now(),
            target_down: 0.0,
            target_up: 0.0,
        }
    }

    fn interval() -> Duration {
        POLL
    }

    fn update(&mut self) -> bool {
        let before = (self.cpu, self.memory, self.disk, self.down, self.up, self.disk_used);

        self.cpu = ease(self.cpu, f32::from(amane::Cpu::read().percent()));
        self.memory = ease(self.memory, f32::from(amane::Memory::read().percent()));

        if self.sampled.elapsed() >= NET_SAMPLE {
            if let Some((interface, rx, tx)) = network() {
                let seconds = self.sampled.elapsed().as_secs_f32();

                if interface == self.interface && seconds > 0.0 {
                    let down = rx.saturating_sub(self.rx) as f32 / seconds;
                    let up = tx.saturating_sub(self.tx) as f32 / seconds;

                    // a short average keeps the numbers readable
                    self.target_down = self.target_down * 0.4 + down * 0.6;
                    self.target_up = self.target_up * 0.4 + up * 0.6;
                } else {
                    self.target_down = 0.0;
                    self.target_up = 0.0;
                }

                self.interface = interface;
                self.rx = rx;
                self.tx = tx;
            }

            self.sampled = Instant::now();
        }

        self.down = ease(self.down, self.target_down);
        self.up = ease(self.up, self.target_up);

        if self.disk_sampled.elapsed() >= DISK_SAMPLE {
            let (used, total) = disk();

            self.disk_used = used;
            self.disk_total = total;
            self.disk_sampled = Instant::now();
        }

        self.disk = ease(self.disk, percent(self.disk_used, self.disk_total));

        (self.cpu, self.memory, self.disk, self.down, self.up, self.disk_used) != before
    }
}

impl Load {
    pub fn cpu(&self) -> f32 {
        self.cpu
    }

    pub fn memory(&self) -> f32 {
        self.memory
    }

    pub fn disk(&self) -> f32 {
        self.disk
    }

    pub fn down(&self) -> f32 {
        self.down
    }

    pub fn up(&self) -> f32 {
        self.up
    }

    pub fn down_percent(&self) -> f32 {
        (self.down / NET_FULL * 100.0).clamp(0.0, 100.0)
    }

    pub fn up_percent(&self) -> f32 {
        (self.up / NET_FULL * 100.0).clamp(0.0, 100.0)
    }
}

/// The desktop load widget, drawn under normal windows by the same renderer.
pub fn view() -> LayerWindow {
    let palette = Palette::read();
    let theme = Theme::from(&palette);
    let load = Load::read();

    let rows: Vec<Box<dyn Widget>> = vec![
        Box::new(meter("ЦП", load.cpu(), format!("{:.0}%", load.cpu()), &theme, 255)),
        Box::new(meter("ОЗУ", load.memory(), format!("{:.0}%", load.memory()), &theme, 255)),
        Box::new(meter("ДИСК", load.disk(), format!("{:.0}%", load.disk()), &theme, 255)),
        Box::new(meter("↓", load.down_percent(), rate(load.down()), &theme, 255)),
        Box::new(meter("↑", load.up_percent(), rate(load.up()), &theme, 140)),
    ];

    let body = Column::new(rows).width(Parent).gap(GAP);

    let root = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .radius(14.0)
        .fill(theme.panel)
        .border(1.0, theme.border)
        .padding(amane::Padding {
            left: PADDING,
            right: PADDING,
            top: PADDING,
            bottom: PADDING,
        })
        .child(body);

    LayerWindow::new()
        .width(WIDTH)
        .height(HEIGHT)
        .anchor_vertical(Vertical::Bottom)
        .anchor_horizontal(Horizontal::Right)
        .margin(Margin {
            top: 0,
            right: 18,
            bottom: 18,
            left: 0,
        })
        // the backdrop rule in niri keeps this one surface shared by every
        // workspace, the same way the wallpaper is
        .layer(Layer::Background)
        .space(Zone::Ignore)
        .click_through()
        .namespace("amane-desktop")
        .child(root)
}

fn meter(label: &'static str, percent: f32, value: String, theme: &Theme, alpha: u8) -> Row {
    let fill = with_alpha(theme.accent, alpha);

    let bar = Rectangle::new()
        .width(BAR_WIDTH)
        .height(4.0)
        .radius(amane::Full)
        .fill(with_alpha(theme.fg, 22))
        .align_child(Start, Center)
        .clip();

    let bar = if percent > 0.5 {
        bar.child(
            Rectangle::new()
                .width((BAR_WIDTH * percent / 100.0).max(2.0))
                .height(4.0)
                .radius(amane::Full)
                .fill(fill),
        )
    } else {
        bar
    };

    Row::new(vec![
        Box::new(
            Rectangle::new()
                .width(LABEL_WIDTH)
                .height(ROW_HEIGHT)
                .fill(Color::TRANSPARENT)
                .align_child(Start, Center)
                .child(Text::new(label).size(10.5).color(theme.muted).tight()),
        ) as Box<dyn Widget>,
        Box::new(bar),
        Box::new(
            Rectangle::new()
                .width(VALUE_WIDTH)
                .height(ROW_HEIGHT)
                .fill(Color::TRANSPARENT)
                .align_child(End, Center)
                .child(
                    Text::new(value)
                        .size(10.5)
                        .color(theme.fg)
                        .weight(Weight::Medium),
                ),
        ),
    ])
    .width(Parent)
    .gap(GAP)
    .align(Center)
}

// the default route picks the interface, so a VPN or ethernet is measured
// instead of whatever happens to move bytes
fn network() -> Option<(String, u64, u64)> {
    let route = fs::read_to_string("/proc/net/route").ok()?;

    let interface = route
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();

            if fields.len() < 8 || fields[1] != "00000000" || fields[7] != "00000000" {
                return None;
            }

            let metric = fields[6].parse::<u32>().unwrap_or(u32::MAX);

            Some((metric, String::from(fields[0])))
        })
        .min_by_key(|(metric, _)| *metric)
        .map(|(_, interface)| interface)
        .unwrap_or_else(|| String::from("wlp13s0"));

    let dev = fs::read_to_string("/proc/net/dev").ok()?;
    let line = dev
        .lines()
        .find(|line| line.trim_start().starts_with(&format!("{interface}:")))?;

    let (_, counters) = line.split_once(':')?;
    let fields: Vec<&str> = counters.split_whitespace().collect();

    let rx = fields.first()?.parse().ok()?;
    let tx = fields.get(8)?.parse().ok()?;

    Some((interface, rx, tx))
}

fn disk() -> (u64, u64) {
    let output = Command::new("df")
        .args(["-B1", "--output=size,used", "/"])
        .output();

    let Ok(output) = output else {
        return (0, 0);
    };

    let text = String::from_utf8_lossy(&output.stdout);

    let Some(line) = text.lines().filter(|line| !line.trim().is_empty()).next_back() else {
        return (0, 0);
    };

    let mut numbers = line.split_whitespace().filter_map(|field| field.parse().ok());

    let total = numbers.next().unwrap_or(0);
    let used = numbers.next().unwrap_or(0);

    (used, total)
}

fn percent(used: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }

    (used as f32 / total as f32 * 100.0).clamp(0.0, 100.0)
}

// moves a shown value toward its target, snapping once it is close enough that
// the difference would not be visible
fn ease(current: f32, target: f32) -> f32 {
    let next = current + (target - current) * 0.22;

    if (target - next).abs() < 0.35 {
        target
    } else {
        next
    }
}

fn rate(bytes: f32) -> String {
    const K: f32 = 1024.0;

    if bytes < K {
        format!("{bytes:.0} B/s")
    } else if bytes < K * K {
        format!("{:.0} KB/s", bytes / K)
    } else if bytes < K * K * K {
        format!("{:.1} MB/s", bytes / (K * K))
    } else {
        format!("{:.2} GB/s", bytes / (K * K * K))
    }
}
