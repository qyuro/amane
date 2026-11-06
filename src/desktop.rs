use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use amane::{
    Center, Color, Column, Horizontal, Layer, LayerWindow, Margin, Parent, Rectangle, Row, Service,
    Text, Vertical, Weight, Widget, Zone,
};

const WIDTH: f32 = 190.0;
const ROW_HEIGHT: f32 = 16.0;
const GAP: f32 = 6.0;
const METERS: f32 = 7.0;
const HEIGHT: f32 = ROW_HEIGHT * METERS + GAP * (METERS - 1.0);

const POLL: Duration = Duration::from_millis(50);
const NET_SAMPLE: Duration = Duration::from_millis(500);
const DISK_SAMPLE: Duration = Duration::from_secs(5);
const GPU_SAMPLE: Duration = Duration::from_secs(1);

// where the widget sits, in widget units
const WIDGET_X: f32 = 18.0;
const WIDGET_Y: f32 = 46.0;

const GIB: f32 = 1024.0 * 1024.0 * 1024.0;

/// Network throughput, disk use, and eased copies of the CPU and memory loads
/// for the desktop widget.
pub struct Load {
    cpu: f32,
    memory: f32,
    disk: f32,
    gpu: f32,
    gpu_memory: f32,

    down: f32,
    up: f32,

    disk_used: u64,
    disk_total: u64,
    gpu_used: u64,
    gpu_total: u64,

    interface: String,
    rx: u64,
    tx: u64,
    sampled: Instant,
    disk_sampled: Instant,
    gpu_sampled: Instant,

    target_down: f32,
    target_up: f32,
    target_gpu: f32,
    target_gpu_memory: f32,
}

impl Service for Load {
    fn new() -> Self {
        let (interface, rx, tx) = network().unwrap_or_default();
        let (disk_used, disk_total) = disk();
        let gpu = gpu().unwrap_or_default();

        Self {
            cpu: 0.0,
            memory: 0.0,
            disk: percent(disk_used, disk_total),
            gpu: f32::from(gpu.busy),
            gpu_memory: percent(gpu.used, gpu.total),
            down: 0.0,
            up: 0.0,
            disk_used,
            disk_total,
            gpu_used: gpu.used,
            gpu_total: gpu.total,
            interface,
            rx,
            tx,
            sampled: Instant::now(),
            disk_sampled: Instant::now(),
            gpu_sampled: Instant::now(),
            target_down: 0.0,
            target_up: 0.0,
            target_gpu: f32::from(gpu.busy),
            target_gpu_memory: percent(gpu.used, gpu.total),
        }
    }

    fn interval() -> Duration {
        POLL
    }

    fn update(&mut self) -> bool {
        let before = (
            self.cpu,
            self.memory,
            self.disk,
            self.gpu,
            self.gpu_memory,
            self.down,
            self.up,
            self.disk_used,
            self.gpu_used,
        );

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

        if self.gpu_sampled.elapsed() >= GPU_SAMPLE {
            if let Some(gpu) = gpu() {
                self.gpu_used = gpu.used;
                self.gpu_total = gpu.total;
                self.target_gpu = f32::from(gpu.busy);
                self.target_gpu_memory = percent(gpu.used, gpu.total);
            }

            self.gpu_sampled = Instant::now();
        }

        self.gpu = ease(self.gpu, self.target_gpu);
        self.gpu_memory = ease(self.gpu_memory, self.target_gpu_memory);

        let changed = (
            self.cpu,
            self.memory,
            self.disk,
            self.gpu,
            self.gpu_memory,
            self.down,
            self.up,
            self.disk_used,
            self.gpu_used,
        ) != before;



        changed
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

    pub fn gpu(&self) -> f32 {
        self.gpu
    }

    pub fn gpu_memory(&self) -> f32 {
        self.gpu_memory
    }

    pub fn gpu_used(&self) -> u64 {
        self.gpu_used
    }

    pub fn gpu_total(&self) -> u64 {
        self.gpu_total
    }



    pub fn down(&self) -> f32 {
        self.down
    }

    pub fn up(&self) -> f32 {
        self.up
    }
}

/// The desktop load widget: plain white readings, no panel behind them.
pub fn view() -> LayerWindow {
    let load = Load::read();

    let vram = if load.gpu_total() > 0 {
        format!(
            "{:.1}/{:.1}G",
            load.gpu_used() as f32 / GIB,
            load.gpu_total() as f32 / GIB
        )
    } else {
        format!("{:.0}%", load.gpu_memory())
    };

    let rows: Vec<Box<dyn Widget>> = vec![
        Box::new(row("ЦП", format!("{:.0}%", load.cpu()))),
        Box::new(row("ОЗУ", format!("{:.0}%", load.memory()))),
        Box::new(row("ДИСК", format!("{:.0}%", load.disk()))),
        Box::new(row("↓", rate(load.down()))),
        Box::new(row("↑", rate(load.up()))),
        Box::new(row("GPU", format!("{:.0}%", load.gpu()))),
        Box::new(row("VRAM", vram)),
    ];

    let body = Column::new(rows).width(Parent).gap(GAP);

    LayerWindow::new()
        .width(WIDTH)
        .height(HEIGHT)
        .anchor_vertical(Vertical::Top)
        .anchor_horizontal(Horizontal::Left)
        .margin(Margin {
            top: WIDGET_Y as i32,
            right: 0,
            bottom: 0,
            left: WIDGET_X as i32,
        })
        .layer(Layer::Background)
        .space(Zone::Ignore)
        .click_through()
        .namespace("amane-desktop")
        .child(body)
}

fn row(label: &'static str, value: String) -> Rectangle {
    Rectangle::new()
        .width(Parent)
        .height(ROW_HEIGHT)
        .fill(Color::TRANSPARENT)
        .align_child(Center, Center)
        .child(
            Row::new(vec![
                Box::new(
                    Text::new(label)
                        .size(10.5)
                        .color(Color::rgba(255, 255, 255, 170))
                        .tight(),
                ) as Box<dyn Widget>,
                Box::new(
                    Rectangle::new()
                        .width(Parent)
                        .height(1.0)
                        .fill(Color::TRANSPARENT),
                ),
                Box::new(
                    Text::new(value)
                        .size(10.5)
                        .color(Color::WHITE)
                        .weight(Weight::Medium),
                ),
            ])
            .width(Parent)
            .gap(10.0)
            .align(Center),
        )
}

#[derive(Default)]
struct Gpu {
    busy: u8,
    used: u64,
    total: u64,
}

// the amdgpu driver exposes load and VRAM in sysfs; the first card with those
// files is the one driving the display
fn gpu() -> Option<Gpu> {
    let entries = fs::read_dir("/sys/class/drm").ok()?;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if !name.starts_with("card") || name.contains('-') {
            continue;
        }

        let device = entry.path().join("device");

        let Some(busy) = read_number(&device.join("gpu_busy_percent")) else {
            continue;
        };

        let mut total = read_number(&device.join("mem_info_vram_total")).unwrap_or(0);
        let mut used = read_number(&device.join("mem_info_vram_used")).unwrap_or(0);

        // integrated cards report their memory through GTT instead
        if total == 0 {
            total = read_number(&device.join("mem_info_gtt_total")).unwrap_or(0);
            used = read_number(&device.join("mem_info_gtt_used")).unwrap_or(0);
        }

        return Some(Gpu {
            busy: busy.min(100) as u8,
            used,
            total,
        });
    }

    None
}

fn read_number(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
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
