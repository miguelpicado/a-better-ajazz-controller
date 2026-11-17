//! Side panel as ONE bar (`com.miguelpicado.orcadock.panel_weather`): Spain and Colombia clocks over a small
//! picture of the weather in Sada (sunny sky and green ground, sun behind clouds, grey clouds, wet glass with
//! drops when it rains, snow, storm; moon and stars at night). Same canvas and `<!--strip WxH-->` marker as
//! `panel_strip`, so the daemon paints it the same way (whole bar in DRA mode, three slices in BAT mode).
//!
//! Weather: Open-Meteo (no key) fetched with the system `curl` on a background thread every 20 min.
use crate::actions::{BuiltinAction, Ctx};
use crate::core::{civil_from_days, days_from_civil, esc, SANS};
use crate::strip::em;
use serde_json::Value;
use std::fmt::Write;
use std::sync::{Mutex, Once};
use std::time::Duration;

pub const UUID: &str = "com.miguelpicado.orcadock.panel_weather";

// ponytail: Sada fixed in the URL; a setting if another place is ever wanted.
const URL: &str = "https://api.open-meteo.com/v1/forecast?latitude=43.35&longitude=-8.25&current=temperature_2m,weather_code,is_day&daily=temperature_2m_max,temperature_2m_min&timezone=Europe%2FMadrid&forecast_days=1";

const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;

#[derive(Clone, Debug, PartialEq)]
pub struct Wx {
    /// WMO weather code
    pub code: i64,
    pub day: bool,
    pub temp: f64,
    pub max: f64,
    pub min: f64,
}

/// Open-Meteo forecast JSON -> `Wx`.
pub fn parse(json: &str) -> Option<Wx> {
    let v: Value = serde_json::from_str(json).ok()?;
    let (c, d) = (&v["current"], &v["daily"]);
    Some(Wx {
        code: c["weather_code"].as_i64()?,
        day: c["is_day"].as_i64() != Some(0),
        temp: c["temperature_2m"].as_f64()?,
        max: d["temperature_2m_max"][0].as_f64()?,
        min: d["temperature_2m_min"][0].as_f64()?,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sky {
    Clear,
    Partly,
    Cloudy,
    Fog,
    Rain,
    Snow,
    Storm,
}

/// WMO code -> picture.
pub fn sky(code: i64) -> Sky {
    match code {
        0 | 1 => Sky::Clear,
        2 => Sky::Partly,
        45 | 48 => Sky::Fog,
        51..=67 | 80..=82 => Sky::Rain,
        71..=77 | 85 | 86 => Sky::Snow,
        95..=99 => Sky::Storm,
        _ => Sky::Cloudy,
    }
}

fn hm(ms: i64, off_h: i64) -> (i64, i64) {
    let t = (ms + off_h * HOUR).rem_euclid(DAY);
    (t / HOUR, t / 60_000 % 60)
}

/// Wall clocks (h, m) at `ms` UTC: Spain (CET; CEST from the last Sunday of March to the last Sunday of October,
/// 01:00 UTC both) and Colombia (UTC-5, no DST). Independent of this PC's time zone.
pub fn clocks(ms: i64) -> ((i64, i64), (i64, i64)) {
    let (y, _, _) = civil_from_days(ms.div_euclid(DAY));
    let last_sunday = |m| {
        let d = days_from_civil(y, m, 31);
        (d - (d + 4).rem_euclid(7)) * DAY + HOUR // 1970-01-01 was a Thursday
    };
    let es = if (last_sunday(3)..last_sunday(10)).contains(&ms) { 2 } else { 1 };
    (hm(ms, es), hm(ms, -5))
}

/// Deterministic pseudo-random sequence in 0..1 (drops, stars and flakes stay put from one minute to the next).
fn rnd(n: usize, seed: u32) -> Vec<f64> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (s >> 8) as f64 / (1u32 << 24) as f64
        })
        .collect()
}

/// Text with a soft dark copy under it, so it reads on a bright sky too.
#[allow(clippy::too_many_arguments)]
fn text(o: &mut String, x: f64, y: f64, fs: f64, s: &str, fill: &str, op: f64, weight: u32) {
    let d = (fs * 0.06).max(0.8);
    let _ = write!(
        o,
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" {SANS} font-weight="{weight}" font-size="{fs:.1}" fill="#000" fill-opacity=".38">{e}</text><text x="{x:.1}" y="{y:.1}" text-anchor="middle" {SANS} font-weight="{weight}" font-size="{fs:.1}" fill="{fill}" fill-opacity="{op}">{e}</text>"##,
        x + d * 0.6,
        y + d,
        e = esc(s)
    );
}

struct Block {
    lines: Vec<(String, f64, f64, &'static str, f64, u32)>, // text, size, gap before, fill, opacity, weight
    flag: Option<[&'static str; 3]>,
}

impl Block {
    fn height(&self) -> f64 {
        self.lines.iter().map(|l| l.2 + 0.72 * l.1).sum::<f64>() + 0.25 * self.lines.last().map_or(0.0, |l| l.1)
    }
    fn draw(&self, o: &mut String, top: f64, w: f64, k: f64) {
        let mut y = top;
        for (i, (s, fs, gap, fill, op, wt)) in self.lines.iter().enumerate() {
            y += gap + 0.72 * fs;
            let mut cx = w / 2.0;
            if let (0, Some(f)) = (i, self.flag) {
                // flag left of the first line, the pair centred
                let (fw, fh) = (11.0 * k, 7.0 * k);
                let total = fw + 3.0 * k + em(s) * fs;
                let x0 = (w - total) / 2.0;
                let y0 = y - 0.36 * fs - fh / 2.0;
                let bands: [f64; 3] = if f[0] == "#fcd116" { [0.5, 0.25, 0.25] } else { [0.25, 0.5, 0.25] };
                let mut yy = y0;
                for (c, b) in f.iter().zip(bands) {
                    let _ = write!(o, r##"<rect x="{x0:.1}" y="{yy:.1}" width="{fw:.1}" height="{:.1}" fill="{c}"/>"##, fh * b);
                    yy += fh * b;
                }
                cx = x0 + fw + 3.0 * k + em(s) * fs / 2.0;
            }
            text(o, cx, y, *fs, s, fill, *op, *wt);
        }
    }
}

struct Pal {
    /// zenith .. horizon
    sky: [&'static str; 4],
    sea: [&'static str; 2],
    far: &'static str,
    near: [&'static str; 2],
}

fn pal(sk: Sky, night: bool) -> Pal {
    use Sky::*;
    let p = |sky, sea, far, near| Pal { sky, sea, far, near };
    match (sk, night) {
        (Clear, false) => p(["#08378a", "#1c68cc", "#6aaeea", "#dcedf8"], ["#2f78c0", "#0a2f5c"], "#5d7fa3", ["#2c5a33", "#0f2616"]),
        (Partly, false) => p(["#1a4b94", "#3878c4", "#88bce4", "#dfe9f1"], ["#3a7bb8", "#0d335e"], "#6683a3", ["#2c5a33", "#0f2616"]),
        (Clear | Partly, true) => p(["#01030c", "#06102b", "#12234c", "#2b3f6c"], ["#14284f", "#03081a"], "#17244a", ["#0b1712", "#030806"]),
        (Fog, _) if !night => p(["#8b949d", "#a6aeb6", "#c2c8cd", "#d9dde0"], ["#b2bac1", "#87919a"], "#a9b1b8", ["#6b776f", "#46524a"]),
        (Snow, false) => p(["#76838f", "#97a2ae", "#bfc7d0", "#e0e5ea"], ["#7c8b99", "#4a5968"], "#c6cfd8", ["#e3e9ef", "#b9c3cd"]),
        (Snow, true) => p(["#151b28", "#252e40", "#3a4457", "#545f72"], ["#2b3546", "#10151f"], "#5a6578", ["#a7b2be", "#6f7a87"]),
        (Storm, _) => p(["#0f0c18", "#1f1a2e", "#363149", "#544d66"], ["#2c3043", "#0f121c"], "#3a3d50", ["#121c15", "#05090a"]),
        (Rain, false) => p(["#1e2630", "#313c49", "#4b5766", "#6a7582"], ["#45525f", "#1b2531"], "#495562", ["#1c2a21", "#0a120d"]),
        (Cloudy, false) => p(["#46546a", "#68778a", "#95a2b0", "#c3cbd3"], ["#6a7c8d", "#304256"], "#6d7b89", ["#2d4033", "#132017"]),
        _ => p(["#06080d", "#0f131b", "#1a202b", "#29303c"], ["#1d2531", "#070a10"], "#1f2632", ["#0a100c", "#030605"]),
    }
}

/// A cloud layer: fractal noise thresholded at \`t\` (lower = more cover) and lit from the upper left, inside the
/// \`x,y,w,h\` box with soft top/bottom edges. \`light\` tints the lit side, \`op\` the whole layer.
#[allow(clippy::too_many_arguments)]
fn clouds(o: &mut String, id: &str, (x, y, w, h): (f64, f64, f64, f64), freq: (f64, f64), seed: u32, t: f64, light: &str, op: f64) {
    let k = 3.2;
    let _ = write!(
        o,
        r##"<defs><linearGradient id="{id}b" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0"/><stop offset=".25" stop-color="#fff"/><stop offset=".75" stop-color="#fff"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient><filter id="{id}" x="0" y="0" width="1" height="1" color-interpolation-filters="sRGB"><feTurbulence type="fractalNoise" baseFrequency="{:.4} {:.4}" numOctaves="5" seed="{seed}" result="n"/><feColorMatrix in="n" type="matrix" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 {k} 0 0 0 {:.3}" result="d"/><feComposite in="d" in2="SourceGraphic" operator="in" result="m"/><feDiffuseLighting in="n" surfaceScale="3.5" diffuseConstant="1.45" lighting-color="{light}" result="l"><feDistantLight azimuth="235" elevation="38"/></feDiffuseLighting><feComposite in="l" in2="m" operator="in" result="c"/><feGaussianBlur in="c" stdDeviation=".55"/></filter></defs><rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="url(#{id}b)" filter="url(#{id})" opacity="{op}"/>"##,
        freq.0,
        freq.1,
        -t * k
    );
}

/// The bar for a \`w\` x \`h\` device canvas (w across, h along) plus \`pad\` px of overscan at each end, time \`ms\`
/// UTC, weather \`wx\` (None = no data yet). A small photo-like scene of Sada's bay: sky, clouds, sun or moon, sea
/// with its glitter, the far shore and the near coast (village lights at night); rain = fogged glass with drops.
pub fn render(w: u32, h: u32, bat: bool, pad: u32, ms: i64, wx: Option<&Wx>) -> String {
    let (w, h) = ((w as f64).clamp(56.0, 200.0), (h as f64).clamp(300.0, 480.0));
    let k = (w / 80.0).clamp(0.7, 1.4);
    let m = 5.0 * k;
    let fit = |s: &str, max: f64| max.min((w - 2.0 * m) / em(s));
    let sk = wx.map_or(Sky::Cloudy, |x| sky(x.code));
    let night = wx.is_some_and(|x| !x.day);
    let wet = matches!(sk, Sky::Rain | Sky::Storm);
    let p = pal(sk, night);

    let mut o = format!(
        r##"<!--strip {}x{}--><svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><linearGradient id="sk" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{}"/><stop offset=".35" stop-color="{}"/><stop offset=".7" stop-color="{}"/><stop offset=".8" stop-color="{}"/></linearGradient><linearGradient id="sea" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{}"/><stop offset="1" stop-color="{}"/></linearGradient><linearGradient id="nr" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{}"/><stop offset="1" stop-color="{}"/></linearGradient><radialGradient id="bloom"><stop offset="0" stop-color="#fffbe8"/><stop offset=".12" stop-color="#fff3c4" stop-opacity=".9"/><stop offset=".35" stop-color="#ffd37a" stop-opacity=".35"/><stop offset="1" stop-color="#ffb347" stop-opacity="0"/></radialGradient><radialGradient id="halo"><stop offset="0" stop-color="#e9efff" stop-opacity=".5"/><stop offset=".3" stop-color="#c9d6f5" stop-opacity=".18"/><stop offset="1" stop-color="#c9d6f5" stop-opacity="0"/></radialGradient><radialGradient id="moon" cx=".42" cy=".38" r=".7"><stop offset="0" stop-color="#fffef4"/><stop offset=".7" stop-color="#e8e4cf"/><stop offset="1" stop-color="#c9c4ab"/></radialGradient><radialGradient id="glint" cy=".1" r=".9"><stop offset="0" stop-color="#fff6d6" stop-opacity=".85"/><stop offset=".4" stop-color="#ffe2a0" stop-opacity=".3"/><stop offset="1" stop-color="#ffe2a0" stop-opacity="0"/></radialGradient><radialGradient id="dr" cx=".5" cy=".62" r=".62" fx=".5" fy=".85"><stop offset="0" stop-color="#e6f0fb" stop-opacity=".6"/><stop offset=".65" stop-color="#7f93a8" stop-opacity=".22"/><stop offset="1" stop-color="#04080f" stop-opacity=".7"/></radialGradient><filter id="soft" x="-1" y="-1" width="3" height="3"><feGaussianBlur stdDeviation="{:.2}"/></filter><filter id="glass" x="0" y="0" width="1" height="1"><feGaussianBlur stdDeviation="{:.2}"/></filter></defs>"##,
        w as u32, h as u32, p.sky[0], p.sky[1], p.sky[2], p.sky[3], p.sea[0], p.sea[1], p.near[0], p.near[1], 1.2 * k, 1.1 * k
    );

    // ---- text blocks (measured first: the sky picture fits between them) ----
    let ((eh, emn), (ch, cmn)) = clocks(ms);
    let (es, co) = (format!("{eh:02}:{emn:02}"), format!("{ch:02}:{cmn:02}"));
    let clock = |label: &str, t: &str, flag| Block {
        lines: vec![
            (label.to_string(), fit(label, 9.0 * k), 0.0, "#ffffff", 0.9, 700),
            (t.to_string(), fit(t, 30.0 * k), 3.0 * k, "#ffffff", 1.0, 800),
        ],
        flag: Some(flag),
    };
    let es_b = clock("ESPAÑA", &es, ["#c60b1e", "#ffc400", "#c60b1e"]);
    let co_b = clock("COLOMBIA", &co, ["#fcd116", "#003893", "#ce1126"]);
    let (tt, sub) = match wx {
        Some(x) => (format!("{}°", x.temp.round() as i64), format!("↑{}° ↓{}°", x.max.round() as i64, x.min.round() as i64)),
        None => ("--°".to_string(), "Sada".to_string()),
    };
    let t_b = Block {
        lines: vec![
            (tt.clone(), fit(&tt, 28.0 * k), 0.0, "#ffffff", 1.0, 800),
            (sub.clone(), fit(&sub, 11.0 * k), 3.0 * k, "#ffffff", 0.92, 700),
        ],
        flag: None,
    };
    let hz = (h * 0.8).round(); // horizon
    let land = h * 0.885; // top of the near coast
    let (es_t, co_t, t_t) = if bat {
        (40.0 - es_b.height() / 2.0, 189.0 - co_b.height() / 2.0, 338.0 - t_b.height() / 2.0)
    } else {
        let es_t = 7.0 * k;
        let co_t = es_t + es_b.height() + 9.0 * k;
        (es_t, co_t, hz - 7.0 * k - t_b.height())
    };
    let (gap0, gap1) = (co_t + co_b.height(), t_t);
    let cy = (gap0 + gap1) / 2.0;
    let r = (0.13 * w).min((gap1 - gap0) * 0.2).max(5.0 * k);
    let (sx, sy) = (w * 0.62, cy - r * 0.3);

    // ---- background scene (blurred behind fogged glass when it rains) ----
    let head = std::mem::take(&mut o);
    let _ = write!(o, r##"<rect width="{w}" height="{hz}" fill="url(#sk)"/>"##);
    let lit_sky = matches!(sk, Sky::Clear | Sky::Partly);
    if night && lit_sky {
        let v = rnd(3 * 46, 7);
        for p3 in v.chunks(3) {
            let (x, y, s) = (p3[0] * w, p3[1] * hz * 0.97, p3[2]);
            let rr = (0.25 + s * s * 0.75) * k;
            let _ = write!(o, r##"<circle cx="{x:.1}" cy="{y:.1}" r="{rr:.2}" fill="#fff" opacity="{:.2}"/>"##, 0.35 + 0.65 * s);
            if s > 0.85 {
                let _ = write!(o, r##"<circle cx="{x:.1}" cy="{y:.1}" r="{:.2}" fill="#cfe0ff" opacity=".5" filter="url(#soft)"/>"##, rr * 2.2);
            }
        }
    }
    // sun / moon (a diffuse bright patch behind thick cloud by day)
    match (night, lit_sky) {
        (false, true) => {
            let _ = write!(o, r##"<circle cx="{sx:.1}" cy="{sy:.1}" r="{:.1}" fill="url(#bloom)"/><ellipse cx="{sx:.1}" cy="{sy:.1}" rx="{:.1}" ry="{:.2}" fill="#fff6d8" opacity=".35" filter="url(#soft)"/><circle cx="{sx:.1}" cy="{sy:.1}" r="{:.1}" fill="#fffdf2"/>"##, r * 4.2, w * 0.75, 0.9 * k, r * 0.55);
        }
        (true, true) => {
            let _ = write!(o, r##"<circle cx="{sx:.1}" cy="{sy:.1}" r="{:.1}" fill="url(#halo)"/><circle cx="{sx:.1}" cy="{sy:.1}" r="{:.1}" fill="url(#moon)"/>"##, r * 3.6, r * 0.62);
            for (dx, dy, cr) in [(-0.2, -0.15, 0.17), (0.18, 0.12, 0.12), (-0.05, 0.28, 0.1), (0.25, -0.22, 0.08)] {
                let _ = write!(o, r##"<circle cx="{:.1}" cy="{:.1}" r="{:.2}" fill="#9d9880" opacity=".28"/>"##, sx + dx * r, sy + dy * r, cr * r);
            }
        }
        (false, false) if sk != Sky::Storm => {
            let _ = write!(o, r##"<circle cx="{sx:.1}" cy="{sy:.1}" r="{:.1}" fill="url(#bloom)" opacity=".3"/>"##, r * 4.0);
        }
        _ => {}
    }
    // clouds: high thin layer, then the main deck
    let dim = if night { "#7d8796" } else { "#ffffff" };
    match sk {
        Sky::Clear => clouds(&mut o, "c0", (0.0, hz * 0.62, w, hz * 0.33), (0.06, 0.35), 4, 0.66, dim, 0.3),
        Sky::Partly => {
            clouds(&mut o, "c0", (0.0, hz * 0.05, w, hz * 0.5), (0.05, 0.12), 5, 0.52, dim, 0.7);
            clouds(&mut o, "c1", (0.0, gap0 - r, w, gap1 - gap0 + 2.0 * r), (0.035, 0.06), 9, 0.5, dim, 0.95);
        }
        Sky::Cloudy | Sky::Snow => {
            let l = if night { "#6b7380" } else { "#eef1f4" };
            clouds(&mut o, "c0", (0.0, 0.0, w, hz * 0.6), (0.03, 0.05), 3, 0.36, l, 0.95);
            clouds(&mut o, "c1", (0.0, hz * 0.35, w, hz * 0.6), (0.04, 0.07), 8, 0.42, l, 0.9);
        }
        Sky::Fog => {
            clouds(&mut o, "c0", (0.0, 0.0, w, hz), (0.02, 0.04), 2, 0.3, if night { "#5d6570" } else { "#f2f4f6" }, 0.8);
        }
        Sky::Rain | Sky::Storm => {
            let l = if sk == Sky::Storm { "#8a84a8" } else if night { "#4a525e" } else { "#9aa4b0" };
            clouds(&mut o, "c0", (0.0, 0.0, w, hz * 0.75), (0.03, 0.05), 6, 0.28, l, 1.0);
            clouds(&mut o, "c1", (0.0, hz * 0.3, w, hz * 0.65), (0.05, 0.08), 12, 0.38, l, 0.9);
        }
    }
    if sk == Sky::Storm {
        let (x0, y0) = (w * 0.45, hz * 0.42);
        let path = format!(
            "M{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}",
            x0, y0, x0 - 4.0 * k, y0 + 18.0 * k, x0 + 1.0 * k, y0 + 22.0 * k, x0 - 5.0 * k, y0 + 44.0 * k, x0 - 2.0 * k, y0 + 48.0 * k, x0 - 7.0 * k, hz
        );
        let _ = write!(o, r##"<circle cx="{x0:.1}" cy="{y0:.1}" r="{:.1}" fill="#b9b2ff" opacity=".35" filter="url(#soft)"/><path d="{path}" fill="none" stroke="#c9c3ff" stroke-width="{:.1}" opacity=".7" filter="url(#soft)"/><path d="{path}" fill="none" stroke="#fbfaff" stroke-width="{:.2}" stroke-linejoin="round"/>"##, 14.0 * k, 3.5 * k, 0.9 * k);
    }
    // sea, with the sun's or moon's path on it
    let _ = write!(o, r##"<rect y="{hz}" width="{w}" height="{:.1}" fill="url(#sea)"/>"##, h - hz);
    if lit_sky {
        let col = if night { "#dfe6ff" } else { "#fff3cf" };
        let _ = write!(o, r##"<clipPath id="sc"><rect y="{hz}" width="{w}" height="{:.1}"/></clipPath><g clip-path="url(#sc)">"##, h - hz);
        let _ = write!(o, r##"<ellipse cx="{sx:.1}" cy="{hz:.1}" rx="{:.1}" ry="{:.1}" fill="url(#glint)" opacity="{}"/>"##, w * 0.2, (land - hz) * 1.4, if night { 0.55 } else { 0.9 });
        for p3 in rnd(3 * 40, 21).chunks(3) {
            let d = p3[1]; // 0 = horizon, 1 = shore
            let x = sx + (p3[0] - 0.5) * w * (0.15 + 0.35 * d);
            let y = hz + 1.0 + d * (land - hz + 2.0);
            let near = 1.0 - ((x - sx).abs() / (w * 0.3)).min(1.0);
            let _ = write!(o, r##"<ellipse cx="{x:.1}" cy="{y:.1}" rx="{:.2}" ry="{:.2}" fill="{col}" opacity="{:.2}"/>"##, (0.6 + p3[2] * 1.8) * k * (0.5 + d), 0.28 * k, near * (0.35 + 0.6 * p3[2]));
        }
        o.push_str("</g>");
    } else {
        // dull sheen of a grey sea
        let _ = write!(o, r##"<rect y="{hz}" width="{w}" height="{:.1}" fill="#fff" opacity=".06"/>"##, 1.5 * k);
    }
    // far shore (the other side of the bay) in haze, and a headland on the right
    let _ = write!(
        o,
        r##"<path d="M0 {:.1}C{:.1} {:.1} {:.1} {:.1} {:.1} {hz:.1}H0Z" fill="{}" opacity=".9"/><path d="M{:.1} {hz:.1}C{:.1} {:.1} {w:.1} {:.1} {w:.1} {:.1}V{hz:.1}Z" fill="{}" opacity=".75"/>"##,
        hz - 5.0 * k, w * 0.15, hz - 8.0 * k, w * 0.3, hz - 2.0 * k, w * 0.48, p.far,
        w * 0.78, w * 0.86, hz - 4.5 * k, hz - 5.0 * k, hz - 3.0 * k, p.far
    );
    // near coast: a slope down to the water, lit edge on top
    let edge = format!(
        "M0 {:.1}C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}S{:.1} {:.1} {w:.1} {:.1}",
        land - 6.0 * k, w * 0.2, land - 8.0 * k, w * 0.35, land, w * 0.55, land + 2.0 * k, w * 0.85, land + 1.0 * k, land - 1.0 * k
    );
    let _ = write!(o, r##"<path d="{edge}V{h:.1}H0Z" fill="url(#nr)"/><path d="{edge}" fill="none" stroke="#fff" stroke-opacity=".12" stroke-width=".6"/>"##);
    if night {
        for (i, p2) in rnd(2 * 11, 33).chunks(2).enumerate() {
            let x = p2[0] * w;
            let y = land + 1.5 * k + p2[1] * (h - land) * 0.45 + (x / w - 0.55).abs() * 4.0 * k;
            let c = if i % 3 == 0 { "#ffd9a0" } else { "#ffbe5c" };
            let _ = write!(o, r##"<circle cx="{x:.1}" cy="{y:.1}" r="{:.2}" fill="{c}" opacity=".5" filter="url(#soft)"/><circle cx="{x:.1}" cy="{y:.1}" r="{:.2}" fill="#fff1d6"/>"##, 1.6 * k, 0.45 * k);
        }
    }
    if sk == Sky::Fog {
        let _ = write!(o, r##"<rect y="{:.1}" width="{w}" height="{:.1}" fill="{}" opacity=".55" filter="url(#soft)"/>"##, hz - 18.0 * k, 30.0 * k, p.sky[3]);
    }
    if wet {
        // rain falling behind the glass
        for p3 in rnd(3 * 60, 41).chunks(3) {
            let (x, y, l) = (p3[0] * (w + 10.0), p3[1] * h, (6.0 + p3[2] * 10.0) * k);
            let _ = write!(o, r##"<path d="M{x:.1} {y:.1}l{:.1} {l:.1}" stroke="#dbe6f2" stroke-opacity=".22" stroke-width=".5"/>"##, -l * 0.18);
        }
    }
    if sk == Sky::Snow {
        for (i, p3) in rnd(3 * 70, 11).chunks(3).enumerate() {
            let (x, y) = (p3[0] * w, p3[1] * h);
            if i % 5 == 0 {
                let _ = write!(o, r##"<circle cx="{x:.1}" cy="{y:.1}" r="{:.1}" fill="#fff" opacity=".5" filter="url(#soft)"/>"##, (1.8 + p3[2] * 1.6) * k);
            } else {
                let _ = write!(o, r##"<circle cx="{x:.1}" cy="{y:.1}" r="{:.2}" fill="#fff" opacity="{:.2}"/>"##, (0.4 + p3[2] * 0.9) * k, 0.55 + p3[2] * 0.4);
            }
        }
    }
    let scene = std::mem::replace(&mut o, head);
    o.push_str(&scene);
    if wet {
        let _ = write!(o, r##"<g filter="url(#glass)">{scene}</g>"##);
    }

    // ---- wet glass: refracting drops (light below, dark rim, highlight, soft shadow) and a few trails ----
    if wet {
        let _ = write!(o, r##"<rect width="{w}" height="{h}" fill="#b8c7d6" opacity=".06"/>"##);
        for p3 in rnd(3 * 160, 77).chunks(3) {
            let _ = write!(o, r##"<circle cx="{:.1}" cy="{:.1}" r="{:.2}" fill="#dfe9f4" opacity=".22"/>"##, p3[0] * w, p3[1] * h, (0.25 + p3[2] * 0.35) * k);
        }
        for (i, p3) in rnd(3 * 30, 3).chunks(3).enumerate() {
            let (x, y, rr) = (p3[0] * w, p3[1] * h, (1.3 + p3[2] * p3[2] * 3.4) * k);
            if i % 6 == 0 {
                let _ = write!(o, r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}" fill="#cfdbe8" opacity=".14"/>"##, x - rr * 0.35, y - rr * 7.0, rr * 0.7, rr * 7.0, rr * 0.35);
            }
            let _ = write!(o, r##"<ellipse cx="{:.1}" cy="{:.1}" rx="{rr:.2}" ry="{:.2}" fill="#000" opacity=".22" filter="url(#soft)"/><ellipse cx="{x:.1}" cy="{y:.1}" rx="{rr:.2}" ry="{:.2}" fill="url(#dr)"/><ellipse cx="{:.1}" cy="{:.1}" rx="{:.2}" ry="{:.2}" fill="#fff" opacity=".85"/>"##, x + rr * 0.25, y + rr * 0.35, rr * 1.12, rr * 1.12, x - rr * 0.32, y - rr * 0.45, rr * 0.3, rr * 0.2);
        }
    }

    // ---- text on top ----
    es_b.draw(&mut o, es_t, w, k);
    co_b.draw(&mut o, co_t, w, k);
    t_b.draw(&mut o, t_t, w, k);
    if !bat {
        let ink = if sk == Sky::Snow && !night { "#334155" } else { "#ffffff" };
        text(&mut o, w / 2.0, h - 4.0 * k, 8.0 * k, "SADA", ink, 0.85, 700);
    }
    o.push_str("</svg>");
    crate::strip::pad_svg(&o, pad, p.sky[0], p.near[1])
}

static WX: Mutex<Option<Wx>> = Mutex::new(None);

fn fetch() -> Option<Wx> {
    let mut c = std::process::Command::new("curl");
    c.args(["-s", "-m", "15", URL]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: the daemon has no console
    }
    let out = c.output().ok()?;
    parse(&String::from_utf8_lossy(&out.stdout))
}

/// Starts the fetch loop once, the first time a weather bar is shown.
/// ponytail: it then runs for the life of the process (3 small requests an hour), even off this scene.
fn start() {
    static ON: Once = Once::new();
    ON.call_once(|| {
        let _ = std::thread::Builder::new().name("weather".into()).spawn(|| loop {
            let w = fetch();
            let ok = w.is_some();
            if ok {
                *WX.lock().unwrap_or_else(|e| e.into_inner()) = w;
            }
            std::thread::sleep(Duration::from_secs(if ok { 20 * 60 } else { 120 }));
        });
    });
}

pub struct WeatherAction;

impl BuiltinAction for WeatherAction {
    fn uuid(&self) -> &'static str {
        UUID
    }
    fn tick(&mut self, _ctx: &mut Ctx) {
        start();
    }
    /// Only minutes and the weather are drawn, so the SVG (and the image) changes at most once a minute.
    fn render(&self, ctx: &Ctx, _context: &str) -> String {
        let wx = WX.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let s = ctx.deck.strip;
        let pad = s.pad as u32;
        render(s.h as u32, (s.w as u32).saturating_sub(2 * pad), s.bat, pad, ctx.now, wx.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parse_iso_ms;
    use crate::strip::strip_dims;

    fn at(iso: &str) -> i64 {
        parse_iso_ms(iso).unwrap()
    }

    #[test]
    fn spain_dst_and_colombia() {
        assert_eq!(clocks(at("2026-11-17T11:50:00Z")), ((12, 50), (6, 50)));
        assert_eq!(clocks(at("2026-03-29T00:59:00Z")).0, (1, 59));
        assert_eq!(clocks(at("2026-03-29T01:00:00Z")).0, (3, 0));
        assert_eq!(clocks(at("2026-10-25T00:59:00Z")).0, (2, 59));
        assert_eq!(clocks(at("2026-10-25T01:00:00Z")).0, (2, 0));
        assert_eq!(clocks(at("2026-07-01T03:00:00Z")), ((5, 0), (22, 0)));
    }

    #[test]
    fn parses_open_meteo() {
        let j = r#"{"current":{"temperature_2m":13.6,"weather_code":61,"is_day":1},"daily":{"temperature_2m_max":[15.2],"temperature_2m_min":[9.4]}}"#;
        assert_eq!(parse(j), Some(Wx { code: 61, day: true, temp: 13.6, max: 15.2, min: 9.4 }));
        assert_eq!(parse("{}"), None);
        assert_eq!(sky(61), Sky::Rain);
        assert_eq!(sky(2), Sky::Partly);
        assert_eq!(sky(96), Sky::Storm);
    }

    #[test]
    fn every_sky_renders_inside_the_canvas() {
        let t = at("2026-11-17T23:59:00Z");
        for code in [0, 2, 3, 45, 61, 73, 95] {
            for day in [true, false] {
                for (w, h, bat) in [(80, 378, false), (80, 378, true), (95, 378, false), (56, 300, false), (200, 480, false)] {
                    let wx = Wx { code, day, temp: -0.4, max: 21.0, min: -3.0 };
                    let svg = render(w, h, bat, 0, t, Some(&wx));
                    assert_eq!(strip_dims(&svg), Some((w, h)));
                    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{code} {day} {w}x{h}");
                    assert!(svg.contains(">00:59<") && svg.contains(">18:59<") && svg.contains(">0°<"), "{svg}");
                    let px = crate::raster::svg_to_rgba_wh(&svg, w, h).unwrap();
                    assert_eq!(px.len(), (w * h * 4) as usize);
                }
            }
        }
        let none = render(80, 378, false, 0, t, None);
        assert_eq!(strip_dims(&render(80, 378, false, 16, t, None)), Some((80, 410)));
        assert!(none.contains(">--°<"));
        assert_eq!(none, render(80, 378, false, 0, t + 30_000, None), "same minute, same image");
    }

    /// `ABAC_WX_DUMP=<dir> cargo test -p abac-builtin dump_previews -- --ignored --nocapture`: one BMP per sky,
    /// rasterised by resvg exactly as on the device (2x), and the render time.
    #[test]
    #[ignore]
    fn dump_previews() {
        let dir = std::path::PathBuf::from(std::env::var("ABAC_WX_DUMP").unwrap());
        let t = at("2026-11-17T11:50:00Z");
        for code in [0, 2, 3, 45, 61, 73, 95] {
            for day in [true, false] {
                let wx = Wx { code, day, temp: 13.6, max: 16.0, min: 9.0 };
                let svg = render(80, 378, false, 16, t, Some(&wx));
                let (w, h) = strip_dims(&svg).unwrap();
                let (w, h) = (w * 2, h * 2);
                let t0 = std::time::Instant::now();
                let px = crate::raster::svg_to_rgba_wh(&svg, w, h).unwrap();
                println!("wx-{code}-{day}: {} ms, {} B svg", t0.elapsed().as_millis(), svg.len());
                let row = (w * 3).div_ceil(4) * 4;
                let mut b = Vec::new();
                let size = 54 + row * h;
                for v in [0x4d42u16 as u32] { b.extend_from_slice(&(v as u16).to_le_bytes()); }
                for v in [size, 0, 54, 40, w, h] { b.extend_from_slice(&v.to_le_bytes()); }
                b.extend_from_slice(&1u16.to_le_bytes());
                b.extend_from_slice(&24u16.to_le_bytes());
                for v in [0u32, row * h, 2835, 2835, 0, 0] { b.extend_from_slice(&v.to_le_bytes()); }
                for y in (0..h).rev() {
                    let mut r: Vec<u8> = (0..w).flat_map(|x| { let i = ((y * w + x) * 4) as usize; [px[i + 2], px[i + 1], px[i]] }).collect();
                    r.resize(row as usize, 0);
                    b.extend_from_slice(&r);
                }
                std::fs::write(dir.join(format!("wx-{code}-{day}.bmp")), b).unwrap();
            }
        }
    }
}
