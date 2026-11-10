//! Built-in actions and their registry. The daemon calls `Registry::appear/disappear` as scenes
//! load, `tick` every ~400 ms (paint what it returns) and `key_down` on a press.
use crate::core::{self, Agent, Counts, QuotaItem};
use crate::orca::Orca;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

pub const POLL_MS: i64 = 1000;
/// Spinner/blink frame length; Stream Dock dropped faster image updates.
pub const SPIN_MS: i64 = 400;
pub const QUOTA_MS: i64 = 30_000;
/// quota-status.json older than this: quota keys dim (orca-dock-rs not running).
pub const QUOTA_STALE_MS: i64 = 10 * 60_000;
const REPOS_MS: i64 = 30_000;
/// workerList is slow (~450 ms) and changes rarely.
const WORKERS_MS: i64 = 10_000;
const WAKE_EVERY_MS: i64 = 3000;
const WAKE_FOR_MS: i64 = 45_000;

/// How long an agent key blinks after turning done / waiting.
fn blink_ms(s: core::State) -> Option<i64> {
    match s {
        core::State::Done => Some(20_000),
        core::State::Question => Some(60_000),
        _ => None,
    }
}

/// Visual answer to a key press (Stream Dock `showOk` / `showAlert`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feedback {
    None,
    Ok,
    Alert,
}

/// Shared data of all Orca Deck actions: one Orca poll and one quota read feed every key.
pub struct Deck {
    pub orca: Box<dyn Orca>,
    pub quota_dir: PathBuf,
    /// Main agents in display order.
    pub agents: Vec<Agent>,
    pub counts: Counts,
    /// Agent key -> blink end (ms). Set when an agent turns waiting/done; cleared on press,
    /// next change or timeout.
    pub blinking: HashMap<String, i64>,
    pub quota: Vec<QuotaItem>,
    pub quota_stale: bool,
    /// Last failure of a poll, cleared by the next success. The daemon may log it.
    pub last_error: Option<String>,
    repos: Vec<Value>,
    workers: Vec<Value>,
    runs: Vec<Value>,
    agents_at: i64,
    repos_at: i64,
    workers_at: i64,
    quota_at: i64,
    /// Forced refresh: (give up at, quota-status.json mtime before the wake).
    wake: Option<(i64, Option<i64>)>,
}

impl Deck {
    pub fn new(orca: Box<dyn Orca>, quota_dir: PathBuf) -> Self {
        Deck {
            orca,
            quota_dir,
            agents: Vec::new(),
            counts: Counts::default(),
            blinking: HashMap::new(),
            quota: Vec::new(),
            quota_stale: false,
            last_error: None,
            repos: Vec::new(),
            workers: Vec::new(),
            runs: Vec::new(),
            agents_at: i64::MIN / 2,
            repos_at: i64::MIN / 2,
            workers_at: i64::MIN / 2,
            quota_at: i64::MIN / 2,
            wake: None,
        }
    }

    /// `QUOTA_DIR` if set, else `%APPDATA%\OrcaDeck\quota` (the daemon's data dir); with no
    /// `APPDATA` (non-Windows) it falls back to `<temp>\OrcaDeck\quota`.
    pub fn default_quota_dir() -> PathBuf {
        if let Some(dir) = std::env::var_os("QUOTA_DIR") {
            return PathBuf::from(dir);
        }
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("OrcaDeck").join("quota")
    }

    pub fn agent(&self, key: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.key == key)
    }

    pub fn is_blinking(&self, key: &str, now: i64) -> bool {
        self.blinking.get(key).is_some_and(|&end| end > now)
    }

    /// Refreshes agents and counts, at most every POLL_MS.
    /// ponytail: blocking calls, and workerList (~450 ms) every 10 s stalls the caller; run the
    /// registry on its own thread, or move workerList to a background thread if the spinner stutters.
    pub fn poll_agents(&mut self, now: i64) {
        if now - self.agents_at < POLL_MS {
            return;
        }
        self.agents_at = now;
        match self.fetch_agents(now) {
            Ok(()) => self.last_error = None,
            Err(e) => self.last_error = Some(format!("agents poll failed: {e}")),
        }
    }

    fn fetch_agents(&mut self, now: i64) -> Result<(), String> {
        let list = |v: Value, k: &str| v.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
        if now - self.repos_at > REPOS_MS {
            self.repos = list(self.orca.call("repo.list", json!({}))?, "repos");
            self.repos_at = now;
        }
        if now - self.workers_at >= WORKERS_MS {
            self.workers_at = now;
            if let Ok(r) = self.orca.call("orchestration.runList", json!({})) {
                self.runs = list(r, "runs");
            }
            if let Ok(w) = self.orca.call("orchestration.workerList", json!({})) {
                self.workers = list(w, "workers");
            }
        }
        let ps = list(self.orca.call("worktree.ps", json!({}))?, "worktrees");
        let terms = list(self.orca.call("terminal.list", json!({}))?, "terminals");
        let next = core::main_agents(&ps, &terms, &self.workers, &self.repos, &self.runs);
        for a in &next {
            match self.agent(&a.key).map(|p| p.state) {
                Some(was) if was != a.state => match blink_ms(a.state) {
                    Some(ms) => {
                        self.blinking.insert(a.key.clone(), now + ms);
                    }
                    None => {
                        self.blinking.remove(&a.key);
                    }
                },
                _ => {}
            }
        }
        self.blinking.retain(|k, end| *end > now && next.iter().any(|a| &a.key == k));
        self.agents = next;
        self.counts = core::agent_counts(&ps);
        Ok(())
    }

    fn quota_file(&self) -> PathBuf {
        self.quota_dir.join("quota-status.json")
    }

    fn quota_mtime(&self) -> Option<i64> {
        let t = std::fs::metadata(self.quota_file()).ok()?.modified().ok()?;
        Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64)
    }

    /// Re-reads quota-status.json every QUOTA_MS, or every 3 s during a forced refresh.
    /// Read-only: orca-dock-rs owns the file, so both always show the same numbers.
    pub fn poll_quota(&mut self, now: i64) {
        let every = if self.wake.is_some() { WAKE_EVERY_MS } else { QUOTA_MS };
        if now - self.quota_at < every {
            return;
        }
        self.quota_at = now;
        let mtime = self.quota_mtime();
        let read = std::fs::read_to_string(self.quota_file())
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Value>(&s).map_err(|e| e.to_string()));
        match (mtime, read) {
            (Some(m), Ok(v)) => {
                self.quota_stale = now - m > QUOTA_STALE_MS;
                self.quota = core::quota_items(&v);
            }
            (_, r) => {
                self.quota_stale = true;
                self.last_error = Some(format!("quota read failed: {}", r.err().unwrap_or_else(|| "no mtime".into())));
            }
        }
        if let Some((until, before)) = self.wake {
            if mtime != before || now >= until {
                self.wake = None;
            }
        }
    }

    /// Forces a quota refresh: orca-dock-rs checks QUOTA_DIR/.wake every 5 s and refetches every
    /// source; the file is then re-read every 3 s for up to 45 s, until its mtime changes.
    pub fn wake_quota(&mut self, now: i64) {
        if let Err(e) = std::fs::write(self.quota_dir.join(".wake"), core::iso_utc(now)) {
            self.last_error = Some(format!("quota wake failed: {e}"));
        }
        self.wake = Some((now + WAKE_FOR_MS, self.quota_mtime()));
        self.quota_at = now;
    }
}

/// What an action sees on each call.
pub struct Ctx<'a> {
    pub deck: &'a mut Deck,
    /// Contexts of this action's visible keys, in layout order (device, row, column).
    pub keys: &'a [String],
    /// ms since the epoch.
    pub now: i64,
}

/// A key type built into the daemon, identified by the same UUID the Node plugin used.
pub trait BuiltinAction: Send {
    fn uuid(&self) -> &'static str;
    /// Refresh data and key assignment; runs before `render` on every registry tick.
    fn tick(&mut self, _ctx: &mut Ctx) {}
    /// SVG (144x144 viewBox) for one key.
    fn render(&self, ctx: &Ctx, context: &str) -> String;
    fn on_key(&mut self, _ctx: &mut Ctx, _context: &str) -> Feedback {
        Feedback::None
    }
}

fn frame(now: i64) -> u32 {
    (now.div_euclid(SPIN_MS) % 8) as u32
}

/// One main Orca agent per key; press = focus its pane.
#[derive(Default)]
pub struct AgentAction {
    slots: HashMap<String, String>,
}

impl BuiltinAction for AgentAction {
    fn uuid(&self) -> &'static str {
        "com.miguelpicado.orcadock.agent"
    }
    fn tick(&mut self, ctx: &mut Ctx) {
        ctx.deck.poll_agents(ctx.now);
        let keys: Vec<String> = ctx.deck.agents.iter().map(|a| a.key.clone()).collect();
        self.slots = core::assign(ctx.keys, &keys, &self.slots);
    }
    fn render(&self, ctx: &Ctx, context: &str) -> String {
        let a = self.slots.get(context).and_then(|k| ctx.deck.agent(k));
        let blink = a.is_some_and(|a| ctx.deck.is_blinking(&a.key, ctx.now));
        core::render_agent(a, frame(ctx.now), ctx.now, blink)
    }
    fn on_key(&mut self, ctx: &mut Ctx, context: &str) -> Feedback {
        let Some(a) = self.slots.get(context).and_then(|k| ctx.deck.agent(k)).cloned() else { return Feedback::None };
        ctx.deck.blinking.remove(&a.key);
        ctx.deck.orca.raise();
        match ctx.deck.orca.call("terminal.focus", json!({ "terminal": a.handle })) {
            Ok(_) => Feedback::None,
            Err(e) => {
                ctx.deck.last_error = Some(format!("focus failed {}: {e}", a.label));
                Feedback::Alert
            }
        }
    }
}

/// One AI account per key from quota-status.json; press = force a refresh.
#[derive(Default)]
pub struct QuotaAction {
    slots: HashMap<String, String>,
}

impl BuiltinAction for QuotaAction {
    fn uuid(&self) -> &'static str {
        "com.miguelpicado.orcadock.quota"
    }
    fn tick(&mut self, ctx: &mut Ctx) {
        ctx.deck.poll_quota(ctx.now);
        let keys: Vec<String> = ctx.deck.quota.iter().map(|q| q.key.clone()).collect();
        self.slots = core::assign(ctx.keys, &keys, &self.slots);
    }
    fn render(&self, ctx: &Ctx, context: &str) -> String {
        let q = self.slots.get(context).and_then(|k| ctx.deck.quota.iter().find(|q| &q.key == k));
        core::render_quota(q, ctx.deck.quota_stale, ctx.now)
    }
    fn on_key(&mut self, ctx: &mut Ctx, _context: &str) -> Feedback {
        ctx.deck.wake_quota(ctx.now);
        Feedback::Ok
    }
}

/// Side panel tile: HH:MM and date.
pub struct ClockAction;

impl BuiltinAction for ClockAction {
    fn uuid(&self) -> &'static str {
        "com.miguelpicado.orcadock.clock"
    }
    fn render(&self, _ctx: &Ctx, _context: &str) -> String {
        core::render_clock(crate::orca::local_time())
    }
}

/// Side panel tile: live agents working / waiting / done.
pub struct SummaryAction;

impl BuiltinAction for SummaryAction {
    fn uuid(&self) -> &'static str {
        "com.miguelpicado.orcadock.summary"
    }
    fn tick(&mut self, ctx: &mut Ctx) {
        ctx.deck.poll_agents(ctx.now);
    }
    fn render(&self, ctx: &Ctx, _context: &str) -> String {
        core::render_summary(&ctx.deck.counts)
    }
}

/// A key placed in a scene (Stream Dock `willAppear`).
#[derive(Clone, Debug)]
pub struct Key {
    pub uuid: String,
    pub context: String,
    pub device: String,
    pub row: u32,
    pub col: u32,
}

/// Built-in actions keyed by UUID, the keys they are shown on, and the last image of each key.
pub struct Registry {
    pub deck: Deck,
    actions: HashMap<&'static str, Box<dyn BuiltinAction>>,
    keys: Vec<Key>,
    shown: HashMap<String, String>,
}

impl Registry {
    /// Registry with the four Orca Deck actions.
    pub fn new(deck: Deck) -> Self {
        let list: Vec<Box<dyn BuiltinAction>> =
            vec![Box::new(AgentAction::default()), Box::new(QuotaAction::default()), Box::new(ClockAction), Box::new(SummaryAction)];
        Registry { deck, actions: list.into_iter().map(|a| (a.uuid(), a)).collect(), keys: Vec::new(), shown: HashMap::new() }
    }

    pub fn handles(&self, uuid: &str) -> bool {
        self.actions.contains_key(uuid)
    }

    /// UUIDs of the built-in actions, so a scene can be placed without touching the registry
    /// (which moves to its own thread after startup).
    pub fn uuids(&self) -> Vec<&'static str> {
        self.actions.keys().copied().collect()
    }

    /// Adds a key; false if its UUID is not built in (the host should treat it as a plugin key).
    pub fn appear(&mut self, key: Key) -> bool {
        if !self.handles(&key.uuid) {
            return false;
        }
        self.disappear(&key.context);
        self.keys.push(key);
        true
    }

    pub fn disappear(&mut self, context: &str) {
        self.keys.retain(|k| k.context != context);
        self.shown.remove(context);
    }

    /// Refreshes data and returns `(context, svg)` for every key whose image changed.
    pub fn tick(&mut self, now: i64) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let Registry { deck, actions, keys, shown } = self;
        for (uuid, action) in actions.iter_mut() {
            let mut mine: Vec<&Key> = keys.iter().filter(|k| k.uuid == *uuid).collect();
            if mine.is_empty() {
                continue;
            }
            mine.sort_by(|a, b| a.device.cmp(&b.device).then(a.row.cmp(&b.row)).then(a.col.cmp(&b.col)));
            let ctxs: Vec<String> = mine.iter().map(|k| k.context.clone()).collect();
            let mut ctx = Ctx { deck: &mut *deck, keys: &ctxs, now };
            action.tick(&mut ctx);
            for c in &ctxs {
                let img = action.render(&ctx, c);
                if shown.get(c) != Some(&img) {
                    shown.insert(c.clone(), img.clone());
                    out.push((c.clone(), img));
                }
            }
        }
        out
    }

    /// Handles a press (keyDown: Stream Dock delivers keyUp late). Call `tick` afterwards to repaint.
    pub fn key_down(&mut self, context: &str, now: i64) -> Feedback {
        let Some(uuid) = self.keys.iter().find(|k| k.context == context).map(|k| k.uuid.clone()) else { return Feedback::None };
        let ctxs: Vec<String> = self.keys.iter().filter(|k| k.uuid == uuid).map(|k| k.context.clone()).collect();
        let action = self.actions.get_mut(uuid.as_str()).expect("appear checks the uuid");
        action.on_key(&mut Ctx { deck: &mut self.deck, keys: &ctxs, now }, context)
    }
}

/// ms since the epoch, for `tick` / `key_down`.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Env vars are process-wide; cargo runs this binary's tests in one process, so this is the
    // only test that touches them.
    #[test]
    fn default_quota_dir_honours_quota_dir_then_appdata() {
        std::env::set_var("QUOTA_DIR", "quota-from-env");
        assert_eq!(Deck::default_quota_dir(), PathBuf::from("quota-from-env"));

        std::env::remove_var("QUOTA_DIR");
        std::env::set_var("APPDATA", "appdata-dir");
        assert_eq!(Deck::default_quota_dir(), PathBuf::from("appdata-dir").join("OrcaDeck").join("quota"));

        std::env::remove_var("APPDATA");
        assert_eq!(Deck::default_quota_dir(), std::env::temp_dir().join("OrcaDeck").join("quota"));
    }
}
