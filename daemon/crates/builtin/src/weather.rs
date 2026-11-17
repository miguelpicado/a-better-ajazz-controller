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

fn cloud(o: &mut String, cx: f64, cy: f64, s: f64, fill: &str, op: f64) {
    let _ = write!(
        o,
        r##"<g fill="{fill}" opacity="{op}"><circle cx="{:.1}" cy="{cy:.1}" r="{:.1}"/><circle cx="{cx:.1}" cy="{:.1}" r="{:.1}"/><circle cx="{:.1}" cy="{cy:.1}" r="{:.1}"/><rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}"/></g>"##,
        cx - 0.45 * s,
        0.3 * s,
        cy - 0.17 * s,
        0.38 * s,
        cx + 0.42 * s,
        0.28 * s,
        cx - 0.75 * s,
        cy - 0.05 * s,
        1.5 * s,
        0.33 * s,
        0.16 * s
    );
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

/// The bar for a `w` x `h` device canvas (w across, h along), time `ms` UTC, weather `wx` (None = no data yet).
pub fn render(w: u32, h: u32, bat: bool, ms: i64, wx: Option<&Wx>) -> String {
    let (w, h) = ((w as f64).clamp(56.0, 200.0), (h as f64).clamp(300.0, 480.0));
    let k = (w / 80.0).clamp(0.7, 1.4);
    let m = 5.0 * k;
    let fit = |s: &str, max: f64| max.min((w - 2.0 * m) / em(s));
    let sk = wx.map_or(Sky::Cloudy, |x| sky(x.code));
    let night = wx.is_some_and(|x| !x.day);
    let wet = matches!(sk, Sky::Rain | Sky::Storm);

    let (top, mid, bot) = match (sk, night) {
        (Sky::Clear | Sky::Partly, false) => ("#1565c0", "#3d8fe0", "#9fd3f7"),
        (Sky::Clear | Sky::Partly, true) => ("#050b1f", "#0f1f45", "#27406e"),
        (Sky::Snow, _) => ("#6b7c90", "#94a3b5", "#c9d2dc"),
        (Sky::Rain | Sky::Storm, _) if night => ("#0b1018", "#1a2230", "#2b3546"),
        (Sky::Rain | Sky::Storm, _) => ("#253041", "#3a4659", "#5b6879"),
        (_, true) => ("#10151f", "#1f2836", "#36414f"),
        _ => ("#5a6b7e", "#7f8fa1", "#b4bfcb"),
    };
    let (hill_b, hill_f) = match sk {
        Sky::Snow => ("#cfd9e3", "#eef3f7"),
        _ if night => ("#13261a", "#1c3a25"),
        Sky::Clear | Sky::Partly => ("#2e7d32", "#5cb848"),
        _ => ("#2a4a33", "#3c6a46"),
    };

    let mut o = format!(
        r##"<!--strip {}x{}--><svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><linearGradient id="sk" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{top}"/><stop offset=".55" stop-color="{mid}"/><stop offset="1" stop-color="{bot}"/></linearGradient><radialGradient id="gl"><stop offset="0" stop-color="#fff6c2" stop-opacity=".95"/><stop offset=".35" stop-color="#ffd54a" stop-opacity=".55"/><stop offset="1" stop-color="#ffd54a" stop-opacity="0"/></radialGradient><radialGradient id="mg"><stop offset="0" stop-color="#f5f3ce" stop-opacity=".35"/><stop offset="1" stop-color="#f5f3ce" stop-opacity="0"/></radialGradient><radialGradient id="dr" cx=".35" cy=".3" r=".8"><stop offset="0" stop-color="#fff" stop-opacity=".55"/><stop offset=".5" stop-color="#d6e6ff" stop-opacity=".12"/><stop offset="1" stop-color="#060a12" stop-opacity=".45"/></radialGradient></defs><rect width="{w}" height="{h}" fill="url(#sk)"/>"##,
        w as u32, h as u32
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
    let ground = h - 0.12 * h;
    let (es_t, co_t, t_t) = if bat {
        // centred in the three 80 px BAT cells (pitch 149)
        (40.0 - es_b.height() / 2.0, 189.0 - co_b.height() / 2.0, 338.0 - t_b.height() / 2.0)
    } else {
        let es_t = 7.0 * k;
        let co_t = es_t + es_b.height() + 9.0 * k;
        (es_t, co_t, ground - 6.0 * k - t_b.height())
    };
    let (gap0, gap1) = (co_t + co_b.height(), t_t);

    // ---- sky picture ----
    let cy = (gap0 + gap1) / 2.0;
    let r = (0.17 * w).min((gap1 - gap0) * 0.28).max(6.0 * k);
    let (sx, sy) = (w * 0.6, cy - r * 0.15);
    if night && matches!(sk, Sky::Clear | Sky::Partly) {
        for (i, v) in rnd(28, 7).chunks(2).enumerate() {
            let op = 0.45 + 0.5 * ((i * 37) % 10) as f64 / 10.0;
            let _ = write!(o, r##"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="#fff" opacity="{op:.2}"/>"##, v[0] * w, v[1] * ground * 0.95, (0.5 + (i % 3) as f64 * 0.3) * k);
        }
    }
    match (sk, night) {
        (Sky::Clear | Sky::Partly, false) => {
            let _ = write!(o, r##"<circle cx="{sx:.1}" cy="{sy:.1}" r="{:.1}" fill="url(#gl)"/><circle cx="{sx:.1}" cy="{sy:.1}" r="{r:.1}" fill="#ffd23f"/>"##, r * 2.6);
        }
        (Sky::Clear | Sky::Partly, true) => {
            // crescent lit on the left: the left half disc minus a shallower arc
            let (rr, r2) = (r * 0.85, r * 1.1);
            let _ = write!(o, r##"<circle cx="{sx:.1}" cy="{sy:.1}" r="{:.1}" fill="url(#mg)"/><path d="M{sx:.1} {:.1}A{rr:.1} {rr:.1} 0 1 0 {sx:.1} {:.1}A{r2:.1} {r2:.1} 0 0 1 {sx:.1} {:.1}Z" fill="#f5f3ce"/>"##, r * 2.2, sy - rr, sy + rr, sy - rr);
        }
        _ => {}
    }
    let (c1, c2, c3) = if night { ("#5b6676", "#475262", "#3a4453") } else { ("#ffffff", "#e3e8ee", "#c9d1da") };
    match sk {
        Sky::Clear => {}
        Sky::Partly => {
            cloud(&mut o, sx - r * 0.9, sy + r * 0.75, r * 2.6, c1, 0.95);
            cloud(&mut o, w * 0.22, cy - r * 1.4, r * 1.5, c2, 0.75);
        }
        Sky::Cloudy | Sky::Fog | Sky::Snow => {
            cloud(&mut o, w * 0.3, cy - r * 0.9, r * 2.4, c2, 0.85);
            cloud(&mut o, w * 0.72, cy - r * 0.2, r * 2.6, c3, 0.9);
            cloud(&mut o, w * 0.42, cy + r * 0.8, r * 2.3, c1, 0.9);
        }
        Sky::Rain | Sky::Storm => {
            cloud(&mut o, w * 0.32, cy - r * 0.6, r * 2.6, "#4b5563", 0.95);
            cloud(&mut o, w * 0.72, cy + r * 0.2, r * 2.5, "#374151", 0.95);
        }
    }
    if sk == Sky::Storm {
        let (x, y) = (w * 0.55, cy + r * 0.6);
        let _ = write!(o, r##"<path d="M{:.1} {y:.1}L{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}Z" fill="#ffe066"/>"##, x, x - r * 0.5, y + r * 0.9, x - r * 0.1, y + r * 0.9, x - r * 0.35, y + r * 1.7, x + r * 0.45, y + r * 0.6, x + r * 0.05, y + r * 0.6);
    }
    if sk == Sky::Fog {
        for (i, f) in [0.3, 0.45, 0.6, 0.75].iter().enumerate() {
            let _ = write!(o, r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}" fill="#fff" opacity=".22"/>"##, if i % 2 == 0 { -4.0 } else { w * 0.15 }, h * f, w * 0.95, 4.0 * k, 2.0 * k);
        }
    }

    // ---- ground: two soft hills and the place name ----
    let _ = write!(
        o,
        r##"<path d="M0 {:.1}Q{:.1} {:.1} {w:.1} {:.1}V{h:.1}H0Z" fill="{hill_b}"/><path d="M0 {:.1}Q{:.1} {:.1} {w:.1} {:.1}V{h:.1}H0Z" fill="{hill_f}"/>"##,
        ground - 2.0 * k, w * 0.75, ground - 12.0 * k, ground + 2.0 * k,
        ground + 8.0 * k, w * 0.3, ground - 4.0 * k, ground + 6.0 * k
    );
    if sk == Sky::Snow {
        let v = rnd(60, 11);
        for (i, p) in v.chunks(2).enumerate() {
            let _ = write!(o, r##"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="#fff" opacity="{:.2}"/>"##, p[0] * w, p[1] * h, (0.8 + (i % 4) as f64 * 0.45) * k, 0.55 + (i % 5) as f64 * 0.09);
        }
    }

    // ---- wet glass: drops with a highlight, a few with a trail ----
    if wet {
        let _ = write!(o, r##"<rect width="{w}" height="{h}" fill="#a9bdd3" opacity=".07"/>"##);
        let v = rnd(3 * 34, 3);
        for (i, p) in v.chunks(3).enumerate() {
            let (x, y, rr) = (p[0] * w, p[1] * h, (1.1 + p[2] * 2.6) * k);
            if i % 7 == 0 {
                let _ = write!(o, r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}" fill="#fff" opacity=".1"/>"##, x - rr * 0.45, y - rr * 6.0, rr * 0.9, rr * 6.0, rr * 0.45);
            }
            let _ = write!(o, r##"<ellipse cx="{x:.1}" cy="{y:.1}" rx="{rr:.1}" ry="{:.1}" fill="url(#dr)" stroke="#fff" stroke-opacity=".2" stroke-width=".6"/><circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="#fff" opacity=".75"/>"##, rr * 1.15, x - rr * 0.35, y - rr * 0.45, rr * 0.28);
        }
    }

    // ---- text on top ----
    es_b.draw(&mut o, es_t, w, k);
    co_b.draw(&mut o, co_t, w, k);
    t_b.draw(&mut o, t_t, w, k);
    if !bat {
        let ink = if sk == Sky::Snow { "#475569" } else { "#ffffff" };
        text(&mut o, w / 2.0, h - 4.0 * k, 8.0 * k, "SADA", ink, 0.85, 700);
    }
    o.push_str("</svg>");
    o
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
        render(s.h as u32, s.w as u32, s.bat, ctx.now, wx.as_ref())
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
                    let svg = render(w, h, bat, t, Some(&wx));
                    assert_eq!(strip_dims(&svg), Some((w, h)));
                    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{code} {day} {w}x{h}");
                    assert!(svg.contains(">00:59<") && svg.contains(">18:59<") && svg.contains(">0°<"), "{svg}");
                    let px = crate::raster::svg_to_rgba_wh(&svg, w, h).unwrap();
                    assert_eq!(px.len(), (w * h * 4) as usize);
                }
            }
        }
        let none = render(80, 378, false, t, None);
        assert!(none.contains(">--°<"));
        assert_eq!(none, render(80, 378, false, t + 30_000, None), "same minute, same image");
    }

    /// `ABAC_WX_DUMP=<dir> cargo test -p abac-builtin dump_previews -- --ignored`: one SVG per sky to look at.
    #[test]
    #[ignore]
    fn dump_previews() {
        let dir = std::path::PathBuf::from(std::env::var("ABAC_WX_DUMP").unwrap());
        let t = at("2026-11-17T11:50:00Z");
        for code in [0, 2, 3, 45, 61, 73, 95] {
            for day in [true, false] {
                let wx = Wx { code, day, temp: 13.6, max: 16.0, min: 9.0 };
                std::fs::write(dir.join(format!("wx-{code}-{day}.svg")), render(90, 378, false, t, Some(&wx))).unwrap();
            }
        }
    }
}
