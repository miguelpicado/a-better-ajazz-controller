//! Per-key calibration of the experimental DRA render path (docs/research/hid/dra-render-notes.md).
//! `calibration.json` holds the saved offsets/sizes; `State` is the live calibration mode shared by the
//! device loop (physical key selects) and the editor API (`InspectorBridge::calibrate`).
use crate::proto::{parse_offsets, DRA_MAX_W, DRA_TILE_PX, PANEL_H};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{LazyLock, Mutex},
    time::Duration,
};

pub const KEYS: u8 = 18;
pub const MIN_PX: u16 = 8;
const SIDE_CELL: u16 = 80;

/// Calibration of one key, in PANEL pixels: offset from the BAT cell centre and drawn size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyCal {
    pub dx: i16,
    pub dy: i16,
    pub w: u16,
    pub h: u16,
}

impl KeyCal {
    fn clamped(self) -> KeyCal {
        let lim = PANEL_H as i16;
        KeyCal {
            dx: self.dx.clamp(-lim, lim),
            dy: self.dy.clamp(-lim, lim),
            w: self.w.clamp(MIN_PX, DRA_MAX_W),
            h: self.h.clamp(MIN_PX, DRA_MAX_W),
        }
    }
}

/// Keys 16..=18 are one strip drawn as a whole: they share the slot of key 16 (dx, dy and `h` = strip size).
pub fn slot(key: u8) -> u8 {
    if key >= 16 { 16 } else { key }
}

/// The file: `{"enabled":bool,"keys":{"1":{"dx":..,"dy":..,"w":..,"h":..},..}}`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct File {
    pub enabled: bool,
    pub keys: BTreeMap<u8, KeyCal>,
}

impl File {
    /// Unknown/out-of-range keys are skipped; a missing field takes the factory default; values are clamped.
    /// Anything that is not the documented shape is an error (the caller then starts from defaults).
    pub fn parse(s: &str) -> Result<File, String> {
        let v: Value = serde_json::from_str(s.trim_start_matches('\u{feff}')).map_err(|e| format!("bad JSON: {e}"))?;
        let o = v.as_object().ok_or("expected a JSON object")?;
        let enabled = match o.get("enabled") {
            None => false,
            Some(Value::Bool(b)) => *b,
            _ => return Err("enabled must be a boolean".into()),
        };
        let mut keys = BTreeMap::new();
        match o.get("keys") {
            None => {}
            Some(Value::Object(m)) => {
                for (k, e) in m {
                    let Some(k) = k.parse::<u8>().ok().filter(|k| (1..=KEYS).contains(k)) else { continue };
                    let e = e.as_object().ok_or_else(|| format!("key {k}: expected an object"))?;
                    let d = factory(k);
                    let num = |f: &str, d: i64| match e.get(f) {
                        None => Ok(d),
                        Some(n) => n.as_i64().ok_or_else(|| format!("key {k}: {f} must be an integer")),
                    };
                    let c = |v: i64| v.clamp(-10_000, 10_000) as i32;
                    keys.insert(
                        k,
                        KeyCal {
                            dx: c(num("dx", d.dx as i64)?) as i16,
                            dy: c(num("dy", d.dy as i64)?) as i16,
                            w: c(num("w", d.w as i64)?).max(0) as u16,
                            h: c(num("h", d.h as i64)?).max(0) as u16,
                        }
                        .clamped(),
                    );
                }
            }
            _ => return Err("keys must be an object".into()),
        }
        Ok(File { enabled, keys })
    }

    pub fn to_json(&self) -> String {
        let keys: Map<String, Value> = self
            .keys
            .iter()
            .map(|(k, c)| (k.to_string(), json!({ "dx": c.dx, "dy": c.dy, "w": c.w, "h": c.h })))
            .collect();
        serde_json::to_string_pretty(&json!({ "enabled": self.enabled, "keys": keys })).expect("json") + "\n"
    }
}

/// Factory value without env: centred 98 px tile (side cells 80 px).
fn factory(key: u8) -> KeyCal {
    if key >= 16 {
        KeyCal { dx: 0, dy: 0, w: SIDE_CELL, h: SIDE_CELL }
    } else {
        KeyCal { dx: 0, dy: 0, w: DRA_TILE_PX, h: DRA_TILE_PX }
    }
}

/// `%LOCALAPPDATA%\ABAC\calibration.json`; `ABAC_CALIBRATION` overrides the whole path.
pub fn file_path() -> Option<PathBuf> {
    std::env::var_os("ABAC_CALIBRATION")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("ABAC").join("calibration.json")))
}

/// Temp file + rename, so a crash never leaves a half-written file.
pub fn write_atomic(path: &std::path::Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

pub struct State {
    pub file: File,
    /// Env-derived value for keys the file does not list (`ABAC_DRA_TILE_PX`, `ABAC_DRA_OFFSETS`).
    defaults: Vec<KeyCal>,
    path: Option<PathBuf>,
    on: bool,
    selected: u8,
    dirty: BTreeSet<u8>,
    saved: bool,
}

impl State {
    pub fn new(file: File, tile: u16, side: u16, offsets: &str, path: Option<PathBuf>) -> State {
        let off = parse_offsets(offsets);
        let defaults = (1..=KEYS)
            .map(|k| {
                let (dx, dy) = off.get(&k).copied().unwrap_or_default();
                let mut c = if k >= 16 { KeyCal { h: side, ..factory(k) } } else { KeyCal { w: tile, h: tile, ..factory(k) } };
                c.dx = dx;
                c.dy = dy;
                c.clamped()
            })
            .collect();
        State { file, defaults, path, on: false, selected: 0, dirty: BTreeSet::new(), saved: true }
    }

    /// What is drawn for `key` now: the calibrated entry, else the env/factory default.
    pub fn eff(&self, key: u8) -> KeyCal {
        let s = slot(key);
        self.file.keys.get(&s).copied().unwrap_or(self.defaults[s as usize - 1])
    }

    pub fn on(&self) -> bool {
        self.on
    }

    /// The key being calibrated, only while the mode is on.
    pub fn selected(&self) -> Option<u8> {
        (self.on && self.selected != 0).then_some(self.selected)
    }

    fn need_on(&self) -> Result<(), String> {
        self.on.then_some(()).ok_or_else(|| "calibration mode is off".to_string())
    }

    fn mark(&mut self, key: u8) {
        if key >= 16 {
            self.dirty.extend(16..=18);
        } else {
            self.dirty.insert(key);
        }
    }

    pub fn set_on(&mut self, on: bool) -> Result<(), String> {
        if on == self.on {
            return Ok(());
        }
        if on {
            self.on = true;
            return Ok(());
        }
        // Leaving: the selected key goes back to its content, then everything is saved.
        let sel = self.selected;
        self.on = false;
        self.selected = 0;
        if sel != 0 {
            self.mark(sel);
        }
        self.save()
    }

    pub fn select(&mut self, key: u8) -> Result<(), String> {
        self.need_on()?;
        if !(1..=KEYS).contains(&key) {
            return Err(format!("key {key} outside 1..={KEYS}"));
        }
        if key == self.selected {
            return Ok(());
        }
        let old = std::mem::replace(&mut self.selected, key);
        if old != 0 {
            self.mark(old);
        }
        self.mark(key);
        Ok(())
    }

    /// Adds deltas `[dx, dy, w, h]` to a key (default: the selected one). Clamped to the panel and `w` <= 384.
    pub fn adjust(&mut self, key: Option<u8>, d: [i32; 4]) -> Result<KeyCal, String> {
        let k = self.target(key)?;
        let c = self.eff(k);
        let add = |v: i32, d: i32| v.saturating_add(d.clamp(-1000, 1000));
        let n = KeyCal {
            dx: add(c.dx as i32, d[0]).clamp(-10_000, 10_000) as i16,
            dy: add(c.dy as i32, d[1]).clamp(-10_000, 10_000) as i16,
            w: add(c.w as i32, d[2]).clamp(0, 10_000) as u16,
            h: add(c.h as i32, d[3]).clamp(0, 10_000) as u16,
        };
        self.put(k, n)
    }

    /// Replaces a key's value (the editor's numeric fields).
    pub fn set(&mut self, key: Option<u8>, v: KeyCal) -> Result<KeyCal, String> {
        let k = self.target(key)?;
        self.put(k, v)
    }

    fn target(&self, key: Option<u8>) -> Result<u8, String> {
        self.need_on()?;
        let k = key.or_else(|| self.selected()).ok_or("no key selected")?;
        (1..=KEYS).contains(&k).then_some(k).ok_or_else(|| format!("key {k} outside 1..={KEYS}"))
    }

    fn put(&mut self, k: u8, v: KeyCal) -> Result<KeyCal, String> {
        let v = v.clamped();
        self.file.keys.insert(slot(k), v);
        self.mark(k);
        self.saved = false;
        Ok(v)
    }

    /// Forgets the calibration of one key, or of all (`None`); they go back to the env/factory default.
    pub fn reset(&mut self, key: Option<u8>) -> Result<(), String> {
        self.need_on()?;
        match key {
            Some(k) if (1..=KEYS).contains(&k) => {
                self.file.keys.remove(&slot(k));
                self.mark(k);
            }
            Some(k) => return Err(format!("key {k} outside 1..={KEYS}")),
            None => {
                for k in std::mem::take(&mut self.file.keys).into_keys() {
                    self.mark(k);
                }
            }
        }
        self.saved = false;
        Ok(())
    }

    /// Keys the device loop must redraw (the selected one as a test pattern, the others as content).
    pub fn take_dirty(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.dirty).into_iter().collect()
    }

    pub fn save(&mut self) -> Result<(), String> {
        let path = self.path.as_ref().ok_or("no calibration path (LOCALAPPDATA unset)")?;
        write_atomic(path, &self.file.to_json())?;
        self.saved = true;
        Ok(())
    }

    pub fn json(&self, dra: bool) -> Value {
        let keys: Vec<Value> = (1..=KEYS)
            .map(|k| {
                let c = self.eff(k);
                json!({ "key": k, "dx": c.dx, "dy": c.dy, "w": c.w, "h": c.h, "custom": self.file.keys.contains_key(&slot(k)) })
            })
            .collect();
        json!({
            "dra": dra, "on": self.on, "selected": self.selected().unwrap_or(0), "enabled": self.file.enabled,
            "saved": self.saved, "keys": keys,
        })
    }
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| {
    let env = |k: &str| std::env::var(k).ok();
    let path = file_path();
    let file = path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| {
            File::parse(&s).unwrap_or_else(|e| {
                eprintln!("calibration.json ignored: {e}");
                File::default()
            })
        })
        .unwrap_or_default();
    let num = |k: &str, d: u64| env(k).and_then(|v| v.trim().parse().ok()).unwrap_or(d);
    let tile = num("ABAC_DRA_TILE_PX", DRA_TILE_PX as u64).clamp(1, DRA_MAX_W as u64) as u16;
    let side = num("ABAC_DRA_SIDE_PX", SIDE_CELL as u64).clamp(1, 200) as u16;
    Mutex::new(State::new(file, tile, side, &env("ABAC_DRA_OFFSETS").unwrap_or_default(), path))
});

/// Runs `f` on the shared state.
pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}

/// The `enabled` flag of the saved file (turns DRA mode on at startup, like `ABAC_DRA=1`).
pub fn file_enabled() -> bool {
    with(|s| s.file.enabled)
}

/// Gap between redraws while calibrating (`ABAC_CALIB_GAP_MS`, default 150, at least 20).
pub fn gap() -> Duration {
    Duration::from_millis(
        std::env::var("ABAC_CALIB_GAP_MS").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(150u64).max(20),
    )
}

/// Applies one editor command (see `docs`): any of `on`, `enabled`, `select`, `adjust`, `set`, `reset`, `save`,
/// in that order. Returns the new state, or the first error.
pub fn apply(s: &mut State, cmd: &Value, dra: bool) -> Result<Value, String> {
    let int = |v: &Value, f: &str| v.get(f).and_then(Value::as_i64).map(|n| n as i32);
    let key = |v: &Value| v.get("key").and_then(Value::as_u64).map(|k| k.min(255) as u8);
    if let Some(b) = cmd.get("enabled").and_then(Value::as_bool) {
        s.file.enabled = b;
        s.saved = false;
    }
    if let Some(b) = cmd.get("on").and_then(Value::as_bool) {
        if b && !dra {
            return Err("calibration needs DRA mode: set ABAC_DRA=1 (or enable it here and restart)".into());
        }
        s.set_on(b)?;
    }
    if let Some(k) = cmd.get("select").and_then(Value::as_u64) {
        s.select(k.min(255) as u8)?;
    }
    if let Some(a) = cmd.get("adjust") {
        let d = |f| int(a, f).unwrap_or(0);
        s.adjust(key(a), [d("dx"), d("dy"), d("w"), d("h")])?;
    }
    if let Some(a) = cmd.get("set") {
        let k = s.target(key(a))?;
        let c = s.eff(k);
        let f = |n: &str, d: i32| int(a, n).unwrap_or(d);
        let v = KeyCal {
            dx: f("dx", c.dx as i32).clamp(-10_000, 10_000) as i16,
            dy: f("dy", c.dy as i32).clamp(-10_000, 10_000) as i16,
            w: f("w", c.w as i32).clamp(0, 10_000) as u16,
            h: f("h", c.h as i32).clamp(0, 10_000) as u16,
        };
        s.set(Some(k), v)?;
    }
    match cmd.get("reset") {
        Some(Value::String(t)) if t == "all" => s.reset(None)?,
        Some(Value::Number(n)) => s.reset(Some(n.as_u64().unwrap_or(0).min(255) as u8))?,
        _ => {}
    }
    if cmd.get("save").and_then(Value::as_bool) == Some(true) {
        s.save()?;
    }
    Ok(s.json(dra))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> State {
        State::new(File::default(), 98, 80, "", None)
    }

    #[test]
    fn parse_serialise_roundtrip_and_defaults() {
        let f = File::parse(r#"{"enabled":true,"keys":{"1":{"dx":-3,"dy":4,"w":90,"h":92},"7":{"dx":2}}}"#).unwrap();
        assert!(f.enabled);
        assert_eq!(f.keys[&1], KeyCal { dx: -3, dy: 4, w: 90, h: 92 });
        assert_eq!(f.keys[&7], KeyCal { dx: 2, dy: 0, w: 98, h: 98 }); // missing fields = factory default
        assert_eq!(File::parse(&f.to_json()).unwrap(), f);
        assert_eq!(File::parse("{}").unwrap(), File::default());
        assert_eq!(File::parse("\u{feff}{}").unwrap(), File::default());
    }

    #[test]
    fn parse_clamps_skips_and_rejects_invalid() {
        let f = File::parse(r#"{"keys":{"0":{},"19":{},"x":{},"2":{"w":9999,"h":0,"dx":99999}}}"#).unwrap();
        assert_eq!(f.keys.len(), 1);
        let c = f.keys[&2];
        assert_eq!((c.w, c.h), (DRA_MAX_W, MIN_PX));
        assert!(c.dx.abs() <= PANEL_H as i16);
        for bad in ["", "nope", "[]", r#"{"enabled":1}"#, r#"{"keys":[]}"#, r#"{"keys":{"1":5}}"#, r#"{"keys":{"1":{"dx":"a"}}}"#] {
            assert!(File::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn env_defaults_fill_uncalibrated_keys() {
        let s = State::new(File::default(), 90, 70, "3:5,-6;16:1,2", None);
        assert_eq!(s.eff(3), KeyCal { dx: 5, dy: -6, w: 90, h: 90 });
        assert_eq!(s.eff(4), KeyCal { dx: 0, dy: 0, w: 90, h: 90 });
        assert_eq!(s.eff(17), s.eff(16)); // strip shares one slot
        assert_eq!((s.eff(18).dx, s.eff(18).dy, s.eff(18).h), (1, 2, 70));
    }

    #[test]
    fn selection_adjust_reset_need_mode_on() {
        let mut s = st();
        assert!(s.select(3).is_err() && s.adjust(None, [1, 0, 0, 0]).is_err() && s.reset(None).is_err());
        s.set_on(true).unwrap();
        assert!(s.adjust(None, [1, 0, 0, 0]).is_err()); // nothing selected
        s.select(3).unwrap();
        assert_eq!(s.take_dirty(), vec![3]);
        let c = s.adjust(None, [5, -2, 4, 0]).unwrap();
        assert_eq!(c, KeyCal { dx: 5, dy: -2, w: 102, h: 98 });
        assert_eq!(s.take_dirty(), vec![3]);
        s.select(5).unwrap();
        assert_eq!(s.take_dirty(), vec![3, 5]); // old goes back to content, new gets the pattern
        s.adjust(Some(3), [0, 0, 1000, 1000]).unwrap(); // explicit key, clamps
        assert_eq!((s.eff(3).w, s.eff(3).h), (DRA_MAX_W, DRA_MAX_W));
        s.reset(Some(3)).unwrap();
        assert_eq!(s.eff(3), KeyCal { dx: 0, dy: 0, w: 98, h: 98 });
        assert!(s.select(0).is_err() && s.select(19).is_err() && s.reset(Some(40)).is_err());
    }

    #[test]
    fn strip_keys_share_a_slot_and_redraw_together() {
        let mut s = st();
        s.set_on(true).unwrap();
        s.select(17).unwrap();
        assert_eq!(s.take_dirty(), vec![16, 17, 18]);
        s.adjust(None, [3, 0, 0, 0]).unwrap();
        assert_eq!(s.eff(16).dx, 3);
        assert_eq!(s.eff(18).dx, 3);
    }

    #[test]
    fn reset_all_marks_only_calibrated_keys_and_leaving_saves() {
        let dir = std::env::temp_dir().join(format!("abac-calib-{}", std::process::id()));
        let path = dir.join("calibration.json");
        let mut s = State::new(File::default(), 98, 80, "", Some(path.clone()));
        s.set_on(true).unwrap();
        s.select(2).unwrap();
        s.adjust(None, [1, 1, 0, 0]).unwrap();
        s.take_dirty();
        s.reset(None).unwrap();
        assert_eq!(s.take_dirty(), vec![2]);
        s.adjust(None, [7, 0, 0, 0]).unwrap();
        s.take_dirty();
        assert!(!s.json(true)["saved"].as_bool().unwrap());
        s.set_on(false).unwrap(); // leave: selected back to content + saved
        assert_eq!(s.take_dirty(), vec![2]);
        assert!(s.json(true)["saved"].as_bool().unwrap());
        assert_eq!(s.selected(), None);
        let back = File::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back.keys[&2].dx, 7);
        assert!(!dir.join("calibration.json.tmp").exists());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn apply_commands_and_dra_gate() {
        let mut s = st();
        let e = apply(&mut s, &json!({"on": true}), false).unwrap_err();
        assert!(e.contains("DRA"));
        let v = apply(&mut s, &json!({"on": true, "select": 4, "adjust": {"dx": 2, "dy": -1}}), true).unwrap();
        assert_eq!((v["on"].as_bool(), v["selected"].as_u64()), (Some(true), Some(4)));
        assert_eq!(v["keys"][3]["dx"], 2);
        let v = apply(&mut s, &json!({"set": {"w": 60, "h": 70}}), true).unwrap();
        assert_eq!((v["keys"][3]["w"].as_u64(), v["keys"][3]["h"].as_u64(), v["keys"][3]["custom"].as_bool()), (Some(60), Some(70), Some(true)));
        let v = apply(&mut s, &json!({"reset": 4}), true).unwrap();
        assert_eq!(v["keys"][3]["custom"], false);
        apply(&mut s, &json!({"adjust": {"dx": 1}}), true).unwrap();
        let v = apply(&mut s, &json!({"reset": "all", "enabled": true}), true).unwrap();
        assert_eq!((v["enabled"].as_bool(), v["keys"][3]["custom"].as_bool()), (Some(true), Some(false)));
        // save without a path reports instead of panicking
        assert!(apply(&mut s, &json!({"save": true}), true).is_err());
    }

    #[test]
    fn write_atomic_replaces_file() {
        let dir = std::env::temp_dir().join(format!("abac-calib-w-{}", std::process::id()));
        let p = dir.join("c.json");
        write_atomic(&p, "a").unwrap();
        write_atomic(&p, "b").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "b");
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_dir(&dir);
    }
}
