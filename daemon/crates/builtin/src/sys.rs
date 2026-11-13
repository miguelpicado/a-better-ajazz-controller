//! System monitor keys (CPU, RAM, disk, network, GPU) and power keys (lock, log off, restart,
//! shut down, Task Manager). They take the UUIDs of the HotSpot plugins the stock profiles use,
//! so those scenes work unchanged. Data comes straight from Windows (`GetSystemTimes`, NVML,
//! HWiNFO shared memory): no plugin process, no extra crate.
use crate::actions::{BuiltinAction, Ctx, Deck, Feedback};
use crate::core::{esc, svg, SANS};
use std::collections::HashMap;

/// Sampling period while a monitor key is visible. Nothing runs otherwise.
const SAMPLE_MS: i64 = 2000;
/// A power key must be pressed twice within this time (one stray press must not reboot the PC).
const ARM_MS: i64 = 4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    Cpu,
    CpuTemp,
    CpuClock,
    CpuPower,
    Ram,
    Disk,
    Net,
    GpuTemp,
    GpuUtil,
    GpuMem,
    GpuClock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    Lock,
    Logoff,
    Restart,
    Shutdown,
    TaskManager,
}

/// New UUIDs of this project's own plugin, plus the HotSpot ones older scenes still use.
const MONITORS: [(&str, Metric); 22] = [
    ("com.miguelpicado.orcadock.cpu", Metric::Cpu),
    ("com.hotspot.stream.cpu", Metric::Cpu),
    ("com.miguelpicado.orcadock.cpu_temp", Metric::CpuTemp),
    ("com.hotspot.stream.cpu.temperature", Metric::CpuTemp),
    ("com.miguelpicado.orcadock.cpu_clock", Metric::CpuClock),
    ("com.hotspot.stream.Cpu_Clock", Metric::CpuClock),
    ("com.miguelpicado.orcadock.cpu_power", Metric::CpuPower),
    ("com.hotspot.stream.Cpu_Package_Power", Metric::CpuPower),
    ("com.miguelpicado.orcadock.ram", Metric::Ram),
    ("com.hotspot.stream.memory", Metric::Ram),
    ("com.miguelpicado.orcadock.disk", Metric::Disk),
    ("com.hotspot.stream.PhysicalDisk", Metric::Disk),
    ("com.miguelpicado.orcadock.net", Metric::Net),
    ("com.hotspot.stream.NetWork", Metric::Net),
    ("com.miguelpicado.orcadock.gpu_temp", Metric::GpuTemp),
    ("com.hotspot.stream.Gpu_Temperature", Metric::GpuTemp),
    ("com.miguelpicado.orcadock.gpu_usage", Metric::GpuUtil),
    ("com.hotspot.stream.Gpu_Core_Utility", Metric::GpuUtil),
    ("com.miguelpicado.orcadock.gpu_mem", Metric::GpuMem),
    ("com.hotspot.stream.Gpu_Memory_Utility", Metric::GpuMem),
    ("com.miguelpicado.orcadock.gpu_clock", Metric::GpuClock),
    ("com.hotspot.stream.Gpu_Core_Clock", Metric::GpuClock),
];

/// Side cells (80 px, the three small screens at the right): each holds two stacked half-tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Time,
    Cpu,
    Gpu,
}

const PANELS: [(&str, Panel); 3] = [
    ("com.miguelpicado.orcadock.panel_time", Panel::Time),
    ("com.miguelpicado.orcadock.panel_cpu", Panel::Cpu),
    ("com.miguelpicado.orcadock.panel_gpu", Panel::Gpu),
];

const POWERS: [(&str, Power); 9] = [
    ("com.miguelpicado.orcadock.logoff", Power::Logoff),
    ("com.miguelpicado.orcadock.lock", Power::Lock),
    ("com.krabs.windowsgizmos.Lock", Power::Lock),
    ("com.miguelpicado.orcadock.restart", Power::Restart),
    ("com.krabs.windowsgizmos.Reboot", Power::Restart),
    ("com.miguelpicado.orcadock.shutdown", Power::Shutdown),
    ("com.krabs.windowsgizmos.Shutdown", Power::Shutdown),
    ("com.miguelpicado.orcadock.taskmgr", Power::TaskManager),
    ("com.krabs.windowsgizmos.TaskManager", Power::TaskManager),
];

/// Every system action, ready for the registry.
pub fn actions() -> Vec<Box<dyn BuiltinAction>> {
    let mut v: Vec<Box<dyn BuiltinAction>> = Vec::new();
    for (uuid, metric) in MONITORS {
        v.push(Box::new(MonitorAction { uuid, metric }));
    }
    for (uuid, panel) in PANELS {
        v.push(Box::new(PanelAction { uuid, panel }));
    }
    v.push(Box::new(crate::strip::StripAction));
    for (uuid, power) in POWERS {
        v.push(Box::new(PowerAction {
            uuid,
            power,
            armed: HashMap::new(),
        }));
    }
    v
}

/// Last readings. `None` = the source is not available (no NVIDIA driver, HWiNFO not running).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sample {
    /// 0..100
    pub cpu: Option<f64>,
    /// (used %, used bytes, total bytes)
    pub ram: Option<(f64, u64, u64)>,
    /// (free bytes, total bytes) of the system drive
    pub disk: Option<(u64, u64)>,
    /// (down, up) bytes per second
    pub net: Option<(f64, f64)>,
    pub gpu: Option<Gpu>,
    pub hw: Hw,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gpu {
    pub temp: f64,
    pub util: f64,
    pub mem_used: u64,
    pub mem_total: u64,
    pub clock_mhz: f64,
}

/// What only HWiNFO knows (needs a kernel driver to read).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hw {
    pub cpu_temp: Option<f64>,
    pub cpu_clock_mhz: Option<f64>,
    pub cpu_power_w: Option<f64>,
}

/// Sampler owned by the `Deck`; `poll` costs well under a millisecond.
#[derive(Default)]
pub struct Sys {
    pub last: Sample,
    at: i64,
    cpu_prev: Option<(u64, u64)>,
    net_prev: Option<(i64, u64, u64)>,
    gpu: win::GpuHandle,
}

impl Sys {
    pub fn poll(&mut self, now: i64) {
        if self.at != 0 && now - self.at < SAMPLE_MS {
            return;
        }
        self.at = now;
        let cpu = win::cpu_times();
        self.last.cpu = match (self.cpu_prev, cpu) {
            (Some((i0, t0)), Some((i1, t1))) if t1 > t0 => {
                Some((100.0 * (1.0 - (i1 - i0) as f64 / (t1 - t0) as f64)).clamp(0.0, 100.0))
            }
            _ => self.last.cpu,
        };
        self.cpu_prev = cpu;
        self.last.ram = win::memory();
        self.last.disk = win::system_drive();
        let net = win::net_octets();
        self.last.net = match (self.net_prev, net) {
            (Some((t0, d0, u0)), Some((d1, u1))) if now > t0 => {
                let s = (now - t0) as f64 / 1000.0;
                Some((
                    d1.wrapping_sub(d0) as u32 as f64 / s,
                    u1.wrapping_sub(u0) as u32 as f64 / s,
                ))
            }
            _ => self.last.net,
        };
        self.net_prev = net.map(|(d, u)| (now, d, u));
        self.last.gpu = self.gpu.read();
        self.last.hw = win::hwinfo().map(|b| parse_hwinfo(&b)).unwrap_or_default();
    }
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

fn rd<const N: usize>(b: &[u8], at: usize) -> Option<[u8; N]> {
    b.get(at..at + N)?.try_into().ok()
}
fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    rd::<4>(b, at).map(u32::from_le_bytes)
}
fn f64_at(b: &[u8], at: usize) -> Option<f64> {
    rd::<8>(b, at).map(f64::from_le_bytes)
}

/// Reads the CPU readings out of the HWiNFO "Shared Memory Support" block (SM2 layout).
/// Header: signature "HWiS", offset/size/count of sensors and of readings. Reading: type,
/// sensor index, id, 128-byte label, 128-byte user label, 16-byte unit, f64 value.
pub fn parse_hwinfo(b: &[u8]) -> Hw {
    let mut hw = Hw::default();
    if u32_at(b, 0) != Some(0x5369_5748) {
        return hw;
    }
    let (Some(ofs), Some(size), Some(n)) = (u32_at(b, 32), u32_at(b, 36), u32_at(b, 40)) else {
        return hw;
    };
    // Lower rank wins: the first label of the list is the most specific one.
    let (mut t_rank, mut c_rank) = (usize::MAX, usize::MAX);
    for i in 0..n as usize {
        let at = ofs as usize + i * size as usize;
        let (Some(kind), Some(v), Some(label)) = (
            u32_at(b, at),
            f64_at(b, at + 284),
            b.get(at + 12..at + 140),
        ) else {
            break;
        };
        let label = cstr(label);
        let rank = |names: &[&str]| names.iter().position(|n| label == *n || label.starts_with(n));
        match kind {
            1 => {
                if let Some(r) = rank(&["CPU Package", "CPU (Tctl/Tdie)", "Core Max"]) {
                    if r < t_rank {
                        (t_rank, hw.cpu_temp) = (r, Some(v));
                    }
                }
            }
            5 if label == "CPU Package Power" => hw.cpu_power_w = Some(v),
            6 => {
                if let Some(r) = rank(&["Average Effective Clock", "Core 0 Clock"]) {
                    if r < c_rank {
                        (c_rank, hw.cpu_clock_mhz) = (r, Some(v));
                    }
                }
            }
            _ => {}
        }
    }
    hw
}

// ---- rendering ----------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    Ok,
    Warn,
    Danger,
}

/// Bar turns orange from `warn` and the whole key red from `danger` (fractions of the full scale).
pub(crate) fn level(frac: f64, warn: f64, danger: f64) -> Level {
    if frac >= danger {
        Level::Danger
    } else if frac >= warn {
        Level::Warn
    } else {
        Level::Ok
    }
}

fn size(bytes: u64) -> String {
    let g = bytes as f64 / 1e9;
    if g >= 1000.0 {
        format!("{:.1}T", g / 1000.0)
    } else {
        format!("{g:.0}G")
    }
}

fn rate(bps: f64) -> String {
    if bps >= 1e6 {
        format!("{:.1}M", bps / 1e6)
    } else {
        format!("{:.0}K", bps / 1e3)
    }
}

/// Tile: small label, big value (`unit` right beside it, smaller), and an optional fill bar with
/// `sub` written inside it. With no bar the value is centred in the space left.
fn tile(
    label: &str,
    value: &str,
    unit: &str,
    sub: &str,
    bar: Option<(f64, Level)>,
    live: bool,
) -> String {
    let danger = bar.is_some_and(|(_, l)| l == Level::Danger);
    let bg = if danger { "#991b1b" } else { "#000" };
    let n = value.chars().count() as f64 + unit.chars().count() as f64 * 0.55;
    let (vs, y) = if bar.is_some() {
        (if n <= 3.0 { 56 } else if n <= 4.0 { 50 } else if n <= 6.0 { 42 } else { 34 }, 80)
    } else {
        (if n <= 3.0 { 56 } else if n <= 5.0 { 46 } else if n <= 7.0 { 40 } else { 32 }, 94)
    };
    let unit = if unit.is_empty() {
        String::new()
    } else {
        format!(r##"<tspan font-size="{}" font-weight="700" fill="#d4d4d4" dx="5">{}</tspan>"##, vs * 2 / 5, esc(unit))
    };
    let bar = bar
        .map(|(f, l)| {
            let (track, fill) = match l {
                Level::Ok => ("#262626", "#22c55e"),
                Level::Warn => ("#262626", "#f59e0b"),
                Level::Danger => ("#450a0a", "#fca5a5"),
            };
            format!(
                r##"<rect x="10" y="94" width="124" height="38" rx="11" fill="{track}"/><rect x="10" y="94" width="{}" height="38" rx="11" fill="{fill}"/><text x="72" y="121" text-anchor="middle" {SANS} font-weight="800" font-size="21" fill="#fff" stroke="#000" stroke-width="4" stroke-linejoin="round" paint-order="stroke">{}</text>"##,
                (124.0 * f.clamp(0.0, 1.0)).round().max(22.0),
                esc(sub)
            )
        })
        .unwrap_or_else(|| {
            if sub.is_empty() {
                String::new()
            } else {
                format!(r##"<text x="72" y="130" text-anchor="middle" {SANS} font-weight="700" font-size="26" fill="#d4d4d4">{}</text>"##, esc(sub))
            }
        });
    svg(&format!(
        r##"<rect width="144" height="144" fill="{bg}"/><g opacity="{}"><text x="72" y="28" text-anchor="middle" {SANS} font-weight="700" font-size="21" fill="{}">{}</text><text x="72" y="{y}" text-anchor="middle" {SANS} font-weight="800" font-size="{vs}" fill="#fff">{}{unit}</text>{bar}</g>"##,
        if live { 1.0 } else { 0.4 },
        if danger { "#fecaca" } else { "#9ca3af" },
        esc(label),
        esc(value),
    ))
}

pub fn render_metric(m: Metric, s: &Sample) -> String {
    let pct = |p: f64| format!("{}%", p.round() as i64);
    let none = |label: &str, why: &str| tile(label, "N/D", "", why, None, false);
    match m {
        Metric::Cpu => match s.cpu {
            Some(p) => tile("CPU", &pct(p), "", "", Some((p / 100.0, level(p, 70.0, 90.0))), true),
            None => tile("CPU", "--", "", "", None, false),
        },
        Metric::Ram => match s.ram {
            Some((p, used, total)) => tile(
                "RAM",
                &pct(p),
                "",
                &format!("{}/{}B", size(used).trim_end_matches('G'), size(total)),
                Some((p / 100.0, level(p, 80.0, 92.0))),
                true,
            ),
            None => tile("RAM", "--", "", "", None, false),
        },
        Metric::Disk => match s.disk {
            Some((free, total)) => {
                let used = 100.0 * (1.0 - free as f64 / total.max(1) as f64);
                tile(
                    "DISCO C:",
                    &size(free),
                    "",
                    &format!("de {}", size(total)),
                    Some((used / 100.0, level(used, 85.0, 95.0))),
                    true,
                )
            }
            None => tile("DISCO C:", "--", "", "", None, false),
        },
        Metric::Net => match s.net {
            Some((d, u)) => tile("RED", &format!("↓{}", rate(d)), "", &format!("↑{}", rate(u)), None, true),
            None => tile("RED", "--", "", "", None, false),
        },
        Metric::CpuTemp => match s.hw.cpu_temp {
            Some(t) => tile("CPU TEMP", &format!("{}°", t.round() as i64), "", "", Some((t / 100.0, level(t, 75.0, 90.0))), true),
            None => none("CPU TEMP", "falta HWiNFO"),
        },
        Metric::CpuClock => match s.hw.cpu_clock_mhz {
            Some(c) => tile("RELOJ CPU", &format!("{:.1}", c / 1000.0), "", "GHz", None, true),
            None => none("RELOJ CPU", "falta HWiNFO"),
        },
        Metric::CpuPower => match s.hw.cpu_power_w {
            Some(w) => tile("CPU POT.", &format!("{}", w.round() as i64), "W", "", None, true),
            None => none("CPU POT.", "falta HWiNFO"),
        },
        Metric::GpuTemp => match s.gpu {
            Some(g) => tile("GPU TEMP", &format!("{}°", g.temp.round() as i64), "", "", Some((g.temp / 100.0, level(g.temp, 70.0, 83.0))), true),
            None => none("GPU TEMP", "sin NVIDIA"),
        },
        Metric::GpuUtil => match s.gpu {
            Some(g) => tile("GPU", &pct(g.util), "", "", Some((g.util / 100.0, level(g.util, 70.0, 90.0))), true),
            None => none("GPU", "sin NVIDIA"),
        },
        Metric::GpuMem => match s.gpu {
            Some(g) => {
                let f = g.mem_used as f64 / g.mem_total.max(1) as f64;
                tile(
                    "VRAM",
                    &pct(100.0 * f),
                    "",
                    &format!("{:.1}/{:.0}G", g.mem_used as f64 / 1e9, g.mem_total as f64 / 1e9),
                    Some((f, level(f, 0.8, 0.92))),
                    true,
                )
            }
            None => none("VRAM", "sin NVIDIA"),
        },
        Metric::GpuClock => match s.gpu {
            Some(g) => tile("RELOJ GPU", &format!("{:.1}", g.clock_mhz / 1000.0), "", "GHz", None, true),
            None => none("RELOJ GPU", "sin NVIDIA"),
        },
    }
}

/// One half (72 units tall, from `y0`) of a side cell: label, value and a thin bar.
/// Danger paints the half red, like the full-size tiles.
fn half(y0: i32, label: &str, value: &str, bar: Option<(f64, Level)>) -> String {
    let danger = bar.is_some_and(|(_, l)| l == Level::Danger);
    let bg = if danger { format!(r##"<rect y="{y0}" width="144" height="72" fill="#991b1b"/>"##) } else { String::new() };
    let bar = bar
        .map(|(f, l)| {
            let fill = match l {
                Level::Ok => "#22c55e",
                Level::Warn => "#f59e0b",
                Level::Danger => "#fca5a5",
            };
            format!(
                r##"<rect x="14" y="{}" width="116" height="9" rx="4" fill="#262626"/><rect x="14" y="{}" width="{}" height="9" rx="4" fill="{fill}"/>"##,
                y0 + 60,
                y0 + 60,
                (116.0 * f.clamp(0.0, 1.0)).round().max(9.0)
            )
        })
        .unwrap_or_default();
    format!(
        r##"{bg}<text x="72" y="{}" text-anchor="middle" {SANS} font-weight="700" font-size="20" fill="{}">{}</text><text x="72" y="{}" text-anchor="middle" {SANS} font-weight="800" font-size="{}" fill="#fff">{}</text>{bar}"##,
        y0 + 19,
        if danger { "#fecaca" } else { "#9ca3af" },
        esc(label),
        y0 + if bar.is_empty() { 62 } else { 56 },
        if bar.is_empty() { 50 } else { 40 },
        esc(value)
    )
}

fn panel_body(top: String, bottom: String) -> String {
    svg(&format!(
        r##"<rect width="144" height="144" fill="#000"/>{top}<rect x="10" y="71" width="124" height="2" fill="#262626"/>{bottom}"##
    ))
}

pub fn render_panel_time(t: crate::core::LocalTime) -> String {
    const WD: [&str; 7] = ["DOM", "LUN", "MAR", "MIÉ", "JUE", "VIE", "SÁB"];
    const MO: [&str; 12] = [
        "ENE", "FEB", "MAR", "ABR", "MAY", "JUN", "JUL", "AGO", "SEP", "OCT", "NOV", "DIC",
    ];
    let hm = format!("{:02}:{:02}", t.hour, t.minute);
    let label = format!("{} {}", WD[t.weekday as usize % 7], MO[(t.month as usize + 11) % 12]);
    panel_body(
        format!(
            r##"<text x="72" y="56" text-anchor="middle" {SANS} font-weight="800" font-size="46" fill="#fff">{hm}</text>"##
        ),
        half(72, &label, &t.day.to_string(), None),
    )
}

pub fn render_panel(p: Panel, s: &Sample) -> String {
    let pct = |p: f64| format!("{}%", p.round() as i64);
    let temp = |t: f64, warn: f64, danger: f64| (format!("{}°", t.round() as i64), Some((t / 100.0, level(t, warn, danger))));
    let (a, b) = match p {
        Panel::Cpu => (
            s.cpu.map(|c| (pct(c), Some((c / 100.0, level(c, 70.0, 90.0))))),
            s.hw.cpu_temp.map(|t| temp(t, 75.0, 90.0)),
        ),
        Panel::Gpu => (
            s.gpu.map(|g| (pct(g.util), Some((g.util / 100.0, level(g.util, 70.0, 90.0))))),
            s.gpu.map(|g| temp(g.temp, 70.0, 83.0)),
        ),
        Panel::Time => (None, None),
    };
    let name = if p == Panel::Cpu { "CPU" } else { "GPU" };
    let part = |y0, label: String, v: Option<(String, Option<(f64, Level)>)>| match v {
        Some((value, bar)) => half(y0, &label, &value, bar),
        None => half(y0, &label, "--", None),
    };
    panel_body(part(0, name.into(), a), part(72, format!("{name} TEMP"), b))
}

fn render_power(p: Power, armed: bool) -> String {
    let (icon, label) = match p {
        Power::Lock => (r##"<rect width="18" height="11" x="3" y="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>"##, "Bloquear"),
        Power::Logoff => (r##"<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><path d="m16 17 5-5-5-5"/><path d="M21 12H9"/>"##, "Cerrar sesión"),
        Power::Restart => (r##"<path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/>"##, "Reiniciar"),
        Power::Shutdown => (r##"<path d="M12 2v10"/><path d="M18.4 6.6a9 9 0 1 1-12.77.04"/>"##, "Apagar"),
        Power::TaskManager => (r##"<path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"/>"##, "Administrador"),
    };
    let (bg, ink) = if armed { ("#b91c1c", "#fff") } else { ("#000", "#e5e7eb") };
    let label = if armed { "¿Seguro?" } else { label };
    svg(&format!(
        r##"<rect width="144" height="144" fill="{bg}"/><g transform="translate(36 22) scale(3)" fill="none" stroke="{ink}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{icon}</g><text x="72" y="124" text-anchor="middle" {SANS} font-weight="700" font-size="20" fill="{ink}">{}</text>"##,
        esc(label)
    ))
}

// ---- actions ------------------------------------------------------------------------------

struct MonitorAction {
    uuid: &'static str,
    metric: Metric,
}

impl BuiltinAction for MonitorAction {
    fn uuid(&self) -> &'static str {
        self.uuid
    }
    fn tick(&mut self, ctx: &mut Ctx) {
        ctx.deck.sys.poll(ctx.now);
    }
    fn render(&self, ctx: &Ctx, _context: &str) -> String {
        render_metric(self.metric, &ctx.deck.sys.last)
    }
}

struct PanelAction {
    uuid: &'static str,
    panel: Panel,
}

impl BuiltinAction for PanelAction {
    fn uuid(&self) -> &'static str {
        self.uuid
    }
    fn tick(&mut self, ctx: &mut Ctx) {
        if self.panel != Panel::Time {
            ctx.deck.sys.poll(ctx.now);
        }
    }
    fn render(&self, ctx: &Ctx, _context: &str) -> String {
        match self.panel {
            Panel::Time => render_panel_time(crate::orca::local_time()),
            p => render_panel(p, &ctx.deck.sys.last),
        }
    }
}

struct PowerAction {
    uuid: &'static str,
    power: Power,
    /// Key context -> instant the confirmation window ends.
    armed: HashMap<String, i64>,
}

impl BuiltinAction for PowerAction {
    fn uuid(&self) -> &'static str {
        self.uuid
    }
    fn tick(&mut self, ctx: &mut Ctx) {
        self.armed.retain(|_, end| *end > ctx.now);
    }
    fn render(&self, ctx: &Ctx, context: &str) -> String {
        render_power(self.power, self.armed.get(context).is_some_and(|&e| e > ctx.now))
    }
    fn on_key(&mut self, ctx: &mut Ctx, context: &str) -> Feedback {
        // Lock and Task Manager are harmless; the rest need a second press.
        let harmless = matches!(self.power, Power::Lock | Power::TaskManager);
        if !harmless && self.armed.remove(context).is_none_or(|end| end <= ctx.now) {
            self.armed.insert(context.to_string(), ctx.now + ARM_MS);
            return Feedback::None;
        }
        match win::run(self.power) {
            Ok(()) => Feedback::Ok,
            Err(e) => {
                ctx.deck.last_error = Some(format!("{:?} failed: {e}", self.power));
                Feedback::Alert
            }
        }
    }
    fn activity(&self, _deck: &Deck, context: &str, now: i64) -> (bool, bool) {
        // Armed: tick every second so the key goes back to normal when the window ends.
        (false, self.armed.get(context).is_some_and(|&e| e > now))
    }
}

// ---- Windows ------------------------------------------------------------------------------

#[cfg(windows)]
mod win {
    use super::Power;
    use std::ffi::c_void;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    type Handle = *mut c_void;

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        lo: u32,
        hi: u32,
    }
    impl FileTime {
        fn ticks(&self) -> u64 {
            (self.hi as u64) << 32 | self.lo as u64
        }
    }

    #[repr(C)]
    struct MemStatus {
        len: u32,
        load: u32,
        total: u64,
        avail: u64,
        rest: [u64; 5],
    }

    extern "system" {
        fn GetSystemTimes(idle: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
        fn GlobalMemoryStatusEx(s: *mut MemStatus) -> i32;
        fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
        fn LoadLibraryW(name: *const u16) -> Handle;
        fn GetProcAddress(m: Handle, name: *const u8) -> *mut c_void;
        fn OpenFileMappingA(access: u32, inherit: i32, name: *const u8) -> Handle;
        fn MapViewOfFile(h: Handle, access: u32, hi: u32, lo: u32, bytes: usize) -> *mut c_void;
        fn UnmapViewOfFile(p: *const c_void) -> i32;
        fn VirtualQuery(p: *const c_void, info: *mut MemInfo, len: usize) -> usize;
        fn CloseHandle(h: Handle) -> i32;
    }
    #[link(name = "iphlpapi")]
    extern "system" {
        fn GetIfTable(t: *mut u32, size: *mut u32, order: i32) -> u32;
    }
    #[link(name = "user32")]
    extern "system" {
        fn LockWorkStation() -> i32;
    }

    #[repr(C)]
    struct MemInfo {
        base: *mut c_void,
        alloc_base: *mut c_void,
        alloc_prot: u32,
        part: u16,
        size: usize,
        state: u32,
        prot: u32,
        kind: u32,
    }

    /// (idle, kernel + user) in 100 ns ticks. Kernel time already includes idle.
    pub fn cpu_times() -> Option<(u64, u64)> {
        let (mut i, mut k, mut u) = (FileTime::default(), FileTime::default(), FileTime::default());
        (unsafe { GetSystemTimes(&mut i, &mut k, &mut u) } != 0).then(|| (i.ticks(), k.ticks() + u.ticks()))
    }

    /// (used %, used bytes, total bytes)
    pub fn memory() -> Option<(f64, u64, u64)> {
        let mut s = MemStatus { len: std::mem::size_of::<MemStatus>() as u32, load: 0, total: 0, avail: 0, rest: [0; 5] };
        (unsafe { GlobalMemoryStatusEx(&mut s) } != 0)
            .then(|| (s.load as f64, s.total - s.avail, s.total))
    }

    /// (free, total) bytes of the system drive.
    /// ponytail: always `%SystemDrive%`; per-key drive choice would need the action settings.
    pub fn system_drive() -> Option<(u64, u64)> {
        let root = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()) + "\\";
        let w: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        (unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) } != 0)
            .then_some((avail, total))
    }

    /// Summed (received, sent) 32-bit octet counters of the connected Ethernet and Wi-Fi adapters
    /// (virtual adapters would count the same traffic twice). Callers diff with wrapping math.
    pub fn net_octets() -> Option<(u64, u64)> {
        let mut size = 0u32;
        unsafe { GetIfTable(std::ptr::null_mut(), &mut size, 0) };
        let mut buf = vec![0u32; (size as usize).div_ceil(4).max(1)];
        if unsafe { GetIfTable(buf.as_mut_ptr(), &mut size, 0) } != 0 {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, buf.len() * 4) };
        let n = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?) as usize;
        let at = |row: usize, off: usize| {
            bytes
                .get(4 + row * 860 + off..4 + row * 860 + off + 4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        };
        let (mut rx, mut tx) = (0u64, 0u64);
        for r in 0..n {
            // MIB_IFROW: dwType @516 (6 Ethernet, 71 Wi-Fi), dwOperStatus @544 (4 connected, 5 operational)
            if matches!(at(r, 516)?, 6 | 71) && matches!(at(r, 544)?, 4 | 5) {
                rx += at(r, 552)? as u64;
                tx += at(r, 576)? as u64;
            }
        }
        Some((rx, tx))
    }

    /// Bytes of the HWiNFO shared-memory block, or None when HWiNFO is not publishing it.
    pub fn hwinfo() -> Option<Vec<u8>> {
        unsafe {
            let h = OpenFileMappingA(4, 0, b"Global\\HWiNFO_SENS_SM2\0".as_ptr());
            if h.is_null() {
                return None;
            }
            let p = MapViewOfFile(h, 4, 0, 0, 0);
            let out = (!p.is_null()).then(|| {
                let mut mi = std::mem::zeroed::<MemInfo>();
                VirtualQuery(p, &mut mi, std::mem::size_of::<MemInfo>());
                std::slice::from_raw_parts(p as *const u8, mi.size).to_vec()
            });
            if !p.is_null() {
                UnmapViewOfFile(p);
            }
            CloseHandle(h);
            out
        }
    }

    type NvmlGet = unsafe extern "C" fn(Handle, *mut c_void) -> i32;
    type NvmlGetK = unsafe extern "C" fn(Handle, u32, *mut u32) -> i32;

    /// NVML (`nvml.dll`, installed with the NVIDIA driver) loaded on first use. Missing driver = None.
    #[derive(Default)]
    pub struct GpuHandle {
        tried: bool,
        dev: Option<Handle>,
        lib: Option<Handle>,
    }
    // The handles are only used from the builtin thread.
    unsafe impl Send for GpuHandle {}

    impl GpuHandle {
        fn proc(&self, name: &[u8]) -> Option<*mut c_void> {
            let p = unsafe { GetProcAddress(self.lib?, name.as_ptr()) };
            (!p.is_null()).then_some(p)
        }

        fn open(&mut self) {
            self.tried = true;
            let w: Vec<u16> = "nvml.dll".encode_utf16().chain(Some(0)).collect();
            let lib = unsafe { LoadLibraryW(w.as_ptr()) };
            if lib.is_null() {
                return;
            }
            self.lib = Some(lib);
            let (Some(init), Some(by_index)) = (self.proc(b"nvmlInit_v2\0"), self.proc(b"nvmlDeviceGetHandleByIndex_v2\0")) else {
                return;
            };
            unsafe {
                let init: unsafe extern "C" fn() -> i32 = std::mem::transmute(init);
                let by_index: unsafe extern "C" fn(u32, *mut Handle) -> i32 = std::mem::transmute(by_index);
                let mut d: Handle = std::ptr::null_mut();
                if init() == 0 && by_index(0, &mut d) == 0 {
                    self.dev = Some(d);
                }
            }
        }

        pub fn read(&mut self) -> Option<super::Gpu> {
            if !self.tried {
                self.open();
            }
            let d = self.dev?;
            unsafe {
                let k: NvmlGetK = std::mem::transmute(self.proc(b"nvmlDeviceGetTemperature\0")?);
                let clk: NvmlGetK = std::mem::transmute(self.proc(b"nvmlDeviceGetClockInfo\0")?);
                let util: NvmlGet = std::mem::transmute(self.proc(b"nvmlDeviceGetUtilizationRates\0")?);
                let mem: NvmlGet = std::mem::transmute(self.proc(b"nvmlDeviceGetMemoryInfo\0")?);
                let (mut t, mut c) = (0u32, 0u32);
                let (mut u, mut m) = ([0u32; 2], [0u64; 3]);
                if k(d, 0, &mut t) != 0
                    || clk(d, 0, &mut c) != 0
                    || util(d, u.as_mut_ptr() as *mut c_void) != 0
                    || mem(d, m.as_mut_ptr() as *mut c_void) != 0
                {
                    return None;
                }
                Some(super::Gpu { temp: t as f64, util: u[0] as f64, mem_used: m[2], mem_total: m[0], clock_mhz: c as f64 })
            }
        }
    }

    pub fn run(p: Power) -> Result<(), String> {
        if p == Power::Lock {
            return (unsafe { LockWorkStation() } != 0).then_some(()).ok_or_else(|| "LockWorkStation".into());
        }
        let (exe, args): (&str, &[&str]) = match p {
            Power::Logoff => ("shutdown.exe", &["/l"]),
            Power::Restart => ("shutdown.exe", &["/r", "/t", "0"]),
            Power::Shutdown => ("shutdown.exe", &["/s", "/t", "0"]),
            _ => ("taskmgr.exe", &[]),
        };
        Command::new(exe)
            .args(args)
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .map(drop)
            .map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
mod win {
    use super::Power;
    pub fn cpu_times() -> Option<(u64, u64)> { None }
    pub fn memory() -> Option<(f64, u64, u64)> { None }
    pub fn system_drive() -> Option<(u64, u64)> { None }
    pub fn net_octets() -> Option<(u64, u64)> { None }
    pub fn hwinfo() -> Option<Vec<u8>> { None }
    #[derive(Default)]
    pub struct GpuHandle;
    impl GpuHandle {
        pub fn read(&mut self) -> Option<super::Gpu> { None }
    }
    pub fn run(_: Power) -> Result<(), String> { Err("Windows only".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(readings: &[(u32, &str, f64)]) -> Vec<u8> {
        let mut b = vec![0u8; 48 + readings.len() * 316];
        b[0..4].copy_from_slice(&0x5369_5748u32.to_le_bytes());
        b[32..36].copy_from_slice(&48u32.to_le_bytes());
        b[36..40].copy_from_slice(&316u32.to_le_bytes());
        b[40..44].copy_from_slice(&(readings.len() as u32).to_le_bytes());
        for (i, (kind, label, v)) in readings.iter().enumerate() {
            let at = 48 + i * 316;
            b[at..at + 4].copy_from_slice(&kind.to_le_bytes());
            b[at + 12..at + 12 + label.len()].copy_from_slice(label.as_bytes());
            b[at + 284..at + 292].copy_from_slice(&v.to_le_bytes());
        }
        b
    }

    #[test]
    fn hwinfo_picks_cpu_package_over_core_max() {
        let hw = parse_hwinfo(&block(&[
            (1, "Core Max", 71.0),
            (1, "GPU Temperature", 40.0),
            (1, "CPU Package", 64.0),
            (5, "CPU Package Power", 38.5),
            (6, "Core 0 Clock", 4700.0),
        ]));
        assert_eq!(hw.cpu_temp, Some(64.0));
        assert_eq!(hw.cpu_power_w, Some(38.5));
        assert_eq!(hw.cpu_clock_mhz, Some(4700.0));
        assert_eq!(parse_hwinfo(&[0; 64]), Hw::default());
        assert_eq!(parse_hwinfo(&[]), Hw::default());
    }

    #[test]
    fn tiles_render_with_and_without_data() {
        let s = Sample { cpu: Some(23.4), ..Default::default() };
        assert!(render_metric(Metric::Cpu, &s).contains("23%"));
        assert!(render_metric(Metric::CpuTemp, &s).contains("N/D"));
        assert!(render_power(Power::Shutdown, true).contains("Seguro"));
        assert!(render_panel(Panel::Cpu, &s).contains("23%"));
        assert!(render_panel(Panel::Gpu, &s).contains("--"));
    }
}
