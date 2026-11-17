//! Side panel as ONE bar (`com.miguelpicado.orcadock.panel_strip`): time/date, CPU, RAM and GPU in a
//! single tall image. The daemon draws it whole in DRA mode and slices it into the three 80 px cells in BAT
//! mode (cell-aligned layout, so nothing is cut). Experimental: nothing here has run on the device.
//!
//! The canvas is in DEVICE orientation: `dw` (the panel strip's height, ~80..95) wide and `dh` (the panel
//! strip's width, 378 by default) tall, so text reads upright. The daemon rotates it like key images.
//! Layout is a list of rects and texts (`Scene`) so tests can check bounds before any pixel exists.
use crate::actions::{BuiltinAction, Ctx};
use crate::core::{esc, LocalTime, SANS};
use crate::sys::{level, Level, Sample};
use std::fmt::Write;

pub const UUID: &str = "com.miguelpicado.orcadock.panel_strip";

/// Canvas request in PANEL px (as calibration.json states them): `w` = along the long side, `h` = across.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StripCfg {
    pub w: u16,
    pub h: u16,
    /// BAT mode: layout aligned to the three 80 px cells (pitch 149) so slicing loses nothing.
    pub bat: bool,
    /// Overscan at each end, included in `w`: the layout uses `w - 2 pad`, the ends repeat the edge colour.
    pub pad: u16,
}

impl Default for StripCfg {
    fn default() -> Self {
        StripCfg { w: 378, h: 80, bat: true, pad: 0 }
    }
}

const CELL: f64 = 80.0;
const PITCH: f64 = 149.0;
/// Device width / height limits the layout is designed for (clamped, never overflows inside them).
const DW: (f64, f64) = (56.0, 200.0);
const DH: (f64, f64) = (300.0, 480.0);

const MUTED: &str = "#9ca3af";
const DIM: &str = "#6b7280";
const TRACK: &str = "#262626";

// ---- scene --------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rx: f64,
    pub fill: &'static str,
}

/// `anchor`: 's' start, 'm' middle, 'e' end. `y` is the baseline.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub x: f64,
    pub y: f64,
    pub fs: f64,
    pub anchor: char,
    pub s: String,
    pub fill: &'static str,
    pub bold: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    R(Rect),
    T(Text),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub dw: f64,
    pub dh: f64,
    pub items: Vec<Item>,
}

/// Width of `s` in em, a conservative guess for Segoe UI / Arial bold (the tests and the fit use it).
pub fn em(s: &str) -> f64 {
    s.chars()
        .map(|c| match c {
            '0'..='9' => 0.62,
            '.' | ':' | ',' | ' ' | '-' => 0.32,
            '/' => 0.42,
            '°' => 0.45,
            '%' => 0.92,
            'a'..='z' => 0.62,
            _ => 0.72,
        })
        .sum()
}

impl Text {
    pub fn width(&self) -> f64 {
        em(&self.s) * self.fs
    }
    /// (left, right) extent.
    pub fn span(&self) -> (f64, f64) {
        let w = self.width();
        match self.anchor {
            'e' => (self.x - w, self.x),
            'm' => (self.x - w / 2.0, self.x + w / 2.0),
            _ => (self.x, self.x + w),
        }
    }
}

impl Scene {
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, rx: f64, fill: &'static str) {
        self.items.push(Item::R(Rect { x, y, w, h, rx, fill }));
    }
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, x: f64, y: f64, fs: f64, anchor: char, s: &str, fill: &'static str, bold: bool) {
        self.items.push(Item::T(Text { x, y, fs, anchor, s: s.to_string(), fill, bold }));
    }

    /// Label at the left, value at the right of the same baseline; the value shrinks (never below 60 % of
    /// `max_fs`) so the two never touch.
    #[allow(clippy::too_many_arguments)]
    fn row(&mut self, x0: f64, x1: f64, y: f64, label: &str, lfs: f64, lcol: &'static str, value: &str, max_fs: f64, vcol: &'static str) {
        let room = x1 - x0 - em(label) * lfs - 4.0;
        let fs = max_fs.min(room / em(value).max(0.1)).max(max_fs * 0.6);
        self.text(x0, y, lfs, 's', label, lcol, true);
        self.text(x1, y, fs, 'e', value, vcol, true);
    }
}

fn fill_of(l: Level) -> &'static str {
    match l {
        Level::Ok => "#22c55e",
        Level::Warn => "#f59e0b",
        Level::Danger => "#fca5a5",
    }
}

/// One usage block: label + value row, then a `bh` tall bar (height 19 k + bh, returned). `pct` None = N/D.
/// Danger paints the block red.
#[allow(clippy::too_many_arguments)]
fn meter(sc: &mut Scene, top: f64, k: f64, m: f64, name: &str, pct: Option<f64>, warn: f64, danger: f64, bh: f64) -> f64 {
    let lv = pct.map(|p| level(p, warn, danger));
    let red = lv == Some(Level::Danger);
    let h = 19.0 * k + bh;
    if red {
        sc.rect(2.0, top - 3.0 * k, sc.dw - 4.0, h + 6.0 * k, 6.0 * k, "#991b1b");
    }
    let (value, vcol) = match pct {
        Some(p) => (format!("{}%", p.clamp(0.0, 100.0).round() as i64), "#ffffff"),
        None => ("N/D".to_string(), DIM),
    };
    let (x0, x1) = (m, sc.dw - m);
    sc.row(x0, x1, top + 13.0 * k, name, 11.0 * k, if red { "#fecaca" } else { MUTED }, &value, 17.0 * k, vcol);
    let (bw, by) = (x1 - x0, top + 19.0 * k);
    sc.rect(x0, by, bw, bh, bh / 2.0, if red { "#450a0a" } else { TRACK });
    if let Some(p) = pct {
        let f = (p / 100.0).clamp(0.0, 1.0);
        if f > 0.0 {
            sc.rect(x0, by, (bw * f).round().max(bh), bh, bh / 2.0, fill_of(lv.unwrap_or(Level::Ok)));
        }
    }
    h
}

/// Temperature row: `name` (with its unit) at the left, the number at the right, coloured by level. Height 18 k.
#[allow(clippy::too_many_arguments)]
fn temp(sc: &mut Scene, top: f64, k: f64, m: f64, name: &str, t: Option<f64>, warn: f64, danger: f64) -> f64 {
    let (v, c) = match t {
        Some(t) => (
            format!("{}", t.round() as i64),
            match level(t, warn, danger) {
                Level::Ok => "#e5e7eb",
                Level::Warn => "#f59e0b",
                Level::Danger => "#fca5a5",
            },
        ),
        None => ("N/D".to_string(), DIM),
    };
    sc.row(m, sc.dw - m, top + 13.0 * k, name, 11.0 * k, MUTED, &v, 17.0 * k, c);
    18.0 * k
}

const WD: [&str; 7] = ["DOM", "LUN", "MAR", "MIÉ", "JUE", "VIE", "SÁB"];
const MO: [&str; 12] = ["ENE", "FEB", "MAR", "ABR", "MAY", "JUN", "JUL", "AGO", "SEP", "OCT", "NOV", "DIC"];

/// Clock block drawn from `top` (time, weekday + day, month, tight); returns its height.
fn clock(sc: &mut Scene, top: f64, k: f64, m: f64, t: LocalTime) -> f64 {
    let (cx, avail) = (sc.dw / 2.0, sc.dw - 2.0 * m);
    let hm = format!("{:02}:{:02}", t.hour, t.minute);
    let day = format!("{} {}", WD[t.weekday as usize % 7], t.day);
    let mon = MO[(t.month as usize + 11) % 12];
    let fit = |s: &str, max: f64| max.min(avail / em(s));
    let (f1, f2, f3) = (fit(&hm, 34.0 * k), fit(&day, 16.0 * k), fit(mon, 12.0 * k));
    // cap height ~0.72 em; the three lines stacked with fixed gaps, starting at the very top of the block
    let y1 = top + 0.72 * f1;
    let y2 = y1 + 8.0 * k + 0.72 * f2;
    let y3 = y2 + 5.0 * k + 0.72 * f3;
    sc.text(cx, y1, f1, 'm', &hm, "#ffffff", true);
    sc.text(cx, y2, f2, 'm', &day, "#d4d4d4", true);
    sc.text(cx, y3, f3, 'm', mon, MUTED, true);
    y3 + 0.25 * f3 - top
}

/// Builds the bar for a `dw` x `dh` device canvas, top to bottom: clock, CPU, GPU, RAM, CPU temp, GPU temp.
/// `bat` aligns the content to the three BAT cells; otherwise the bar is continuous and the spare height goes
/// to thicker bars and even gaps (width, not height, limits the text size).
pub fn build(t: LocalTime, s: &Sample, dw: u32, dh: u32, bat: bool) -> Scene {
    let (dw, dh) = ((dw as f64).clamp(DW.0, DW.1), (dh as f64).clamp(DH.0, DH.1));
    // Scale from width (text is the limit), capped by height; BAT cells are fixed 80 px, so no growing there.
    let mut k = (dw / 80.0).min(dh / 260.0).clamp(0.7, 1.4);
    if bat {
        k = k.min(1.0);
    }
    let m = 5.0 * k;
    let mut sc = Scene { dw, dh, items: vec![Rect { x: 0.0, y: 0.0, w: dw, h: dh, rx: 0.0, fill: "#000000" }.into()] };
    let (cpu_t, gpu_t) = (s.hw.cpu_temp, s.gpu.map(|g| g.temp));
    let bh0 = 9.0 * k;
    if bat {
        // cell 0: clock; cell 1: CPU + GPU; cell 2: RAM + both temperatures
        let mut ch = Scene { dw, dh, items: vec![] };
        let hc = clock(&mut ch, 0.0, k, m, t);
        clock(&mut sc, (CELL - hc) / 2.0, k, m, t);
        let hm = 19.0 * k + bh0;
        let c1 = PITCH + (CELL - (2.0 * hm + 4.0 * k)) / 2.0;
        meter(&mut sc, c1, k, m, "CPU", s.cpu, 70.0, 90.0, bh0);
        meter(&mut sc, c1 + hm + 4.0 * k, k, m, "GPU", s.gpu.map(|g| g.util), 70.0, 90.0, bh0);
        let c2 = 2.0 * PITCH + (CELL - (hm + 2.0 * 18.0 * k + 2.0 * k)) / 2.0;
        meter(&mut sc, c2, k, m, "RAM", s.ram.map(|r| r.0), 80.0, 92.0, bh0);
        let y = c2 + hm + 2.0 * k;
        temp(&mut sc, y, k, m, "CPU °C", cpu_t, 75.0, 90.0);
        temp(&mut sc, y + 18.0 * k, k, m, "GPU °C", gpu_t, 70.0, 83.0);
        return sc;
    }
    // continuous bar: measure the clock, then spread what is left over 5 even gaps (a little goes into thicker bars)
    let edge = 3.0 * k;
    let mut probe = Scene { dw, dh, items: vec![] };
    let hc = clock(&mut probe, 0.0, k, m, t);
    let free = dh - 2.0 * edge - hc - 3.0 * (19.0 * k + bh0) - 2.0 * 18.0 * k;
    let grow = (free * 0.45 / 3.0).clamp(0.0, 1.5 * k);
    let bh = bh0 + grow;
    let g = ((free - 3.0 * grow) / 5.0).max(0.0);
    let mut y = edge;
    clock(&mut sc, y, k, m, t);
    y += hc + g;
    for (name, pct, warn, danger) in [("CPU", s.cpu, 70.0, 90.0), ("GPU", s.gpu.map(|g| g.util), 70.0, 90.0), ("RAM", s.ram.map(|r| r.0), 80.0, 92.0)] {
        y += meter(&mut sc, y, k, m, name, pct, warn, danger, bh) + g;
    }
    y += temp(&mut sc, y, k, m, "CPU °C", cpu_t, 75.0, 90.0) + g;
    temp(&mut sc, y, k, m, "GPU °C", gpu_t, 70.0, 83.0);
    sc
}

impl From<Rect> for Item {
    fn from(r: Rect) -> Item {
        Item::R(r)
    }
}

impl Scene {
    /// SVG with the `<!--strip WxH-->` marker the daemon looks for (see [`strip_dims`]).
    pub fn svg(&self) -> String {
        let mut o = format!(
            r##"<!--strip {}x{}--><svg xmlns="http://www.w3.org/2000/svg" width="{dw}" height="{dh}" viewBox="0 0 {dw} {dh}">"##,
            self.dw as u32,
            self.dh as u32,
            dw = self.dw,
            dh = self.dh
        );
        for it in &self.items {
            let _ = match it {
                Item::R(r) => write!(
                    o,
                    r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}" fill="{}"/>"##,
                    r.x, r.y, r.w, r.h, r.rx, r.fill
                ),
                Item::T(t) => write!(
                    o,
                    r##"<text x="{:.1}" y="{:.1}" text-anchor="{}" {SANS} font-weight="{}" font-size="{:.1}" fill="{}">{}</text>"##,
                    t.x,
                    t.y,
                    match t.anchor {
                        'e' => "end",
                        'm' => "middle",
                        _ => "start",
                    },
                    if t.bold { 800 } else { 600 },
                    t.fs,
                    t.fill,
                    esc(&t.s)
                ),
            };
        }
        o.push_str("</svg>");
        o
    }
}

/// SVG of the bar for a panel-px request (`cfg.w` x `cfg.h`); the canvas is device-oriented (`h` x `w`).
pub fn render_strip(t: LocalTime, s: &Sample, cfg: StripCfg) -> String {
    let pad = cfg.pad as u32;
    let inner = build(t, s, cfg.h as u32, (cfg.w as u32).saturating_sub(2 * pad), cfg.bat).svg();
    pad_svg(&inner, pad, "#000", "#000")
}

/// Wraps a strip SVG into one `pad` px longer at each end, the ends filled with `top` / `bottom` (overscan: the
/// panel shows a little more than the length the layout is made for, so nothing looks cut).
pub fn pad_svg(inner: &str, pad: u32, top: &str, bottom: &str) -> String {
    let (Some((dw, dh)), Some((_, body))) = (strip_dims(inner), inner.split_once("-->")) else {
        return inner.to_string();
    };
    if pad == 0 {
        return inner.to_string();
    }
    let full = dh + 2 * pad;
    format!(
        r##"<!--strip {dw}x{full}--><svg xmlns="http://www.w3.org/2000/svg" width="{dw}" height="{full}" viewBox="0 0 {dw} {full}"><rect width="{dw}" height="{}" fill="{top}"/><rect y="{}" width="{dw}" height="{}" fill="{bottom}"/><g transform="translate(0 {pad})">{body}</g></svg>"##,
        pad + 1,
        pad + dh - 1,
        pad + 1
    )
}

/// `(dw, dh)` of a strip SVG made by [`Scene::svg`]; None for every other SVG (keys are square 144).
pub fn strip_dims(svg: &str) -> Option<(u32, u32)> {
    let (w, h) = svg.strip_prefix("<!--strip ")?.split_once("-->")?.0.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// Y ranges (device px) of the three BAT cells inside the canvas: where the daemon slices.
pub fn cell_rows() -> [(u32, u32); 3] {
    [0, 1, 2].map(|i| ((i as f64 * PITCH) as u32, (i as f64 * PITCH + CELL) as u32))
}

pub struct StripAction;

impl BuiltinAction for StripAction {
    fn uuid(&self) -> &'static str {
        UUID
    }
    /// Sampling is throttled inside `Sys::poll` (2 s); the clock only shows minutes, so the SVG (and with it
    /// the image) changes at most once per sample or minute: no per-second redraw.
    fn tick(&mut self, ctx: &mut Ctx) {
        ctx.deck.sys.poll(ctx.now);
    }
    fn render(&self, ctx: &Ctx, _context: &str) -> String {
        render_strip(crate::orca::local_time(), &ctx.deck.sys.last, ctx.deck.strip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::Gpu;

    const T: LocalTime = LocalTime { hour: 9, minute: 5, day: 13, month: 11, weekday: 5 };

    fn full() -> Sample {
        Sample {
            cpu: Some(23.4),
            ram: Some((61.0, 20_000_000_000, 32_000_000_000)),
            gpu: Some(Gpu { temp: 58.0, util: 12.0, mem_used: 2_400_000_000, mem_total: 8_000_000_000, clock_mhz: 1500.0 }),
            ..Default::default()
        }
    }

    const SIZES: [(u32, u32, bool); 6] = [(80, 378, true), (80, 378, false), (95, 378, false), (95, 480, false), (200, 480, false), (56, 300, false)];

    fn texts(sc: &Scene) -> Vec<&Text> {
        sc.items.iter().filter_map(|i| if let Item::T(t) = i { Some(t) } else { None }).collect()
    }

    #[test]
    fn nothing_overflows_the_canvas_in_any_state() {
        let hot = Sample { cpu: Some(100.0), ram: Some((100.0, 32_000_000_000, 32_000_000_000)), gpu: Some(Gpu { temp: 99.0, util: 100.0, mem_used: 24_000_000_000, mem_total: 24_000_000_000, clock_mhz: 2800.0 }), ..Default::default() };
        let late = LocalTime { hour: 23, minute: 59, day: 31, month: 12, weekday: 3 };
        for (dw, dh, bat) in SIZES {
            for (t, s) in [(T, full()), (late, hot.clone()), (T, Sample::default())] {
                let sc = build(t, &s, dw, dh, bat);
                for it in &sc.items {
                    match it {
                        Item::R(r) => assert!(r.x >= -0.01 && r.y >= -0.01 && r.x + r.w <= sc.dw + 0.01 && r.y + r.h <= sc.dh + 0.01, "{dw}x{dh} bat={bat} rect {r:?}"),
                        Item::T(x) => {
                            let (l, rr) = x.span();
                            assert!(l >= 0.0 && rr <= sc.dw, "{dw}x{dh} bat={bat} text {:?} spans {l}..{rr}", x.s);
                            assert!(x.y - x.fs * 0.72 >= 0.0 && x.y + x.fs * 0.25 <= sc.dh, "{dw}x{dh} text {:?} y {}", x.s, x.y);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn label_and_value_of_a_row_never_overlap() {
        let hot = Sample { cpu: Some(100.0), gpu: Some(Gpu { temp: 100.0, util: 100.0, mem_used: 24_000_000_000, mem_total: 24_000_000_000, clock_mhz: 0.0 }), ..Default::default() };
        for (dw, dh, bat) in SIZES {
            let sc = build(T, &hot, dw, dh, bat);
            let ts = texts(&sc);
            for pair in ts.windows(2) {
                if pair[0].anchor == 's' && pair[1].anchor == 'e' && (pair[0].y - pair[1].y).abs() < 0.01 {
                    assert!(pair[0].span().1 < pair[1].span().0, "{dw}x{dh}: {:?} touches {:?}", pair[0].s, pair[1].s);
                }
            }
        }
    }

    #[test]
    fn bars_clamp_to_the_track() {
        for p in [-20.0, 0.0, 50.0, 100.0, 180.0] {
            let sc = build(T, &Sample { cpu: Some(p), ..Default::default() }, 80, 378, false);
            let rects: Vec<&Rect> = sc.items.iter().filter_map(|i| if let Item::R(r) = i { Some(r) } else { None }).collect();
            // CPU track = the first 9-px-tall rounded rect, its fill (if any) follows
            let i = rects.iter().position(|r| r.fill == TRACK && r.h > 8.0).expect("track");
            let track = rects[i];
            if p <= 0.0 {
                assert_ne!(rects[i + 1].fill, "#22c55e", "no fill at {p}");
            } else {
                let fill = rects[i + 1];
                assert!(fill.w <= track.w + 0.01 && fill.w >= track.h - 0.01, "fill {} of {} at {p}", fill.w, track.w);
            }
        }
    }

    #[test]
    fn levels_follow_sys_thresholds() {
        let svg = |s: Sample| build(T, &s, 80, 378, false).svg();
        assert!(svg(Sample { cpu: Some(50.0), ..Default::default() }).contains("#22c55e"));
        assert!(svg(Sample { cpu: Some(75.0), ..Default::default() }).contains("#f59e0b"));
        let red = svg(Sample { cpu: Some(95.0), ..Default::default() });
        assert!(red.contains("#991b1b") && red.contains("#fca5a5"));
        // RAM: warn 80, danger 92
        assert!(svg(Sample { ram: Some((85.0, 1, 1)), ..Default::default() }).contains("#f59e0b"));
    }

    #[test]
    fn unavailable_metrics_say_nd() {
        let sc = build(T, &Sample::default(), 80, 378, false);
        let nd = texts(&sc).iter().filter(|t| t.s == "N/D").count();
        assert_eq!(nd, 5, "CPU, GPU, RAM, CPU temp, GPU temp");
        assert!(!sc.svg().contains("#991b1b"));
    }

    #[test]
    fn clock_text_and_wrap() {
        let s = build(T, &full(), 80, 378, false).svg();
        assert!(s.contains(">09:05<") && s.contains(">VIE 13<") && s.contains(">NOV<"), "{s}");
        let wrap = LocalTime { hour: 0, minute: 0, day: 1, month: 1, weekday: 0 };
        let s = build(wrap, &full(), 80, 378, false).svg();
        assert!(s.contains(">00:00<") && s.contains(">DOM 1<") && s.contains(">ENE<"));
    }

    #[test]
    fn change_detection_is_by_svg_text() {
        // The registry repaints only when this string changes: same minute + same readings = same string,
        // a new minute or a new percent = a different one. Seconds are never drawn.
        let a = render_strip(T, &full(), StripCfg::default());
        assert_eq!(a, render_strip(T, &full(), StripCfg::default()));
        assert_ne!(a, render_strip(LocalTime { minute: 6, ..T }, &full(), StripCfg::default()));
        assert_ne!(a, render_strip(T, &Sample { cpu: Some(24.0), ..full() }, StripCfg::default()));
        assert_ne!(a, render_strip(T, &full(), StripCfg { bat: false, ..Default::default() }));
    }

    #[test]
    fn marker_round_trips_and_other_svgs_are_not_strips() {
        let svg = render_strip(T, &full(), StripCfg { w: 480, h: 95, bat: false, pad: 0 });
        assert_eq!(strip_dims(&svg), Some((95, 480)));
        assert_eq!(strip_dims(&render_strip(T, &full(), StripCfg::default())), Some((80, 378)));
        let padded = render_strip(T, &full(), StripCfg { w: 410, h: 80, bat: false, pad: 16 });
        assert_eq!(strip_dims(&padded), Some((80, 410)));
        assert!(padded.contains(r#"translate(0 16)"#) && padded.contains(r#"<!--strip 80x410-->"#));
        assert_eq!(strip_dims("<svg width=\"144\"/>"), None);
        assert_eq!(strip_dims("<!--strip axb-->"), None);
    }

    #[test]
    fn bat_content_sits_inside_the_cells() {
        let sc = build(T, &full(), 80, 378, true);
        let rows = cell_rows();
        let inside = |y0: f64, y1: f64| rows.iter().any(|&(a, b)| y0 >= a as f64 - 3.0 && y1 <= b as f64 + 3.0);
        for it in &sc.items {
            match it {
                Item::R(r) if r.fill != "#000000" => assert!(inside(r.y, r.y + r.h), "rect {r:?}"),
                Item::T(t) => assert!(inside(t.y - 0.72 * t.fs, t.y + 0.25 * t.fs), "text {:?} y {}", t.s, t.y),
                _ => {}
            }
        }
    }
}
