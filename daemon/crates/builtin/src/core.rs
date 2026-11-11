//! Pure logic ported from `core.js`: which agents are "main", which key shows which agent, and
//! how every key looks (SVG text, 144x144 viewBox). No I/O here; everything is testable offline.
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Key state of an agent. Orca `waiting` = a question for you, `blocked` = error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Working,
    Question,
    Error,
    Done,
    Idle,
}

impl State {
    /// Maps an Orca agent state; unknown states have no key state.
    pub fn from_orca(s: &str) -> Option<State> {
        Some(match s {
            "working" => State::Working,
            "waiting" => State::Question,
            "blocked" => State::Error,
            "done" => State::Done,
            "idle" => State::Idle,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            State::Working => "working",
            State::Question => "question",
            State::Error => "error",
            State::Done => "done",
            State::Idle => "idle",
        }
    }
}

/// One main agent as shown on an agent key.
#[derive(Clone, Debug, PartialEq)]
pub struct Agent {
    pub key: String,
    pub handle: String,
    pub worktree_id: String,
    pub repo: String,
    pub label: String,
    pub color: String,
    pub state: State,
    /// Start of the current state, ms since the epoch.
    pub since: Option<i64>,
    /// Live orchestration workers of this agent's runs.
    pub workers: u32,
}

/// JavaScript truthiness of a JSON value (missing = undefined = false).
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// A string-ish field as text (Orca ids are strings; numbers are accepted too).
fn text(v: &Value, k: &str) -> Option<String> {
    match v.get(k)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

/// Main agents: top-level agents (no parent pane) in a root worktree (no parent worktree) whose
/// terminal is alive and is not an orchestration worker. Inputs are the raw RPC lists:
/// `worktree.ps` worktrees, `terminal.list` terminals, `orchestration.workerList` workers,
/// `repo.list` repos and `orchestration.runList` runs.
pub fn main_agents(ps: &[Value], terminals: &[Value], workers: &[Value], repos: &[Value], runs: &[Value]) -> Vec<Agent> {
    let term: HashMap<String, &Value> = terminals
        .iter()
        .map(|t| (format!("{}:{}", text(t, "tabId").unwrap_or_default(), text(t, "leafId").unwrap_or_default()), t))
        .collect();
    let worker_terms: HashSet<String> = workers
        .iter()
        .filter_map(|w| text(w, "agentTerminalHandle").or_else(|| text(w, "terminal")))
        .filter(|h| !h.is_empty())
        .collect();
    let repo_by_id: HashMap<String, &Value> = repos.iter().filter_map(|r| Some((text(r, "id")?, r))).collect();
    // Live workers per orchestrator: worker.runId -> run.coordinator_handle.
    let coord: HashMap<String, String> =
        runs.iter().filter_map(|r| Some((text(r, "id")?, text(r, "coordinator_handle")?))).collect();
    let mut busy: HashMap<String, u32> = HashMap::new();
    for w in workers {
        let h = text(w, "runId").and_then(|id| coord.get(&id).cloned()).filter(|h| !h.is_empty());
        let outcome = w.pointer("/projection/outcome").and_then(Value::as_str).unwrap_or("in_progress");
        if let (Some(h), "in_progress") = (h, outcome) {
            *busy.entry(h).or_default() += 1;
        }
    }

    let mut out = Vec::new();
    for w in ps {
        if truthy(w.get("parentWorktreeId")) || truthy(w.get("isArchived")) {
            continue;
        }
        let repo = text(w, "repoId").and_then(|id| repo_by_id.get(&id).copied());
        for a in arr(w, "agents") {
            if truthy(a.get("parentPaneKey")) {
                continue;
            }
            let key = text(a, "paneKey").unwrap_or_default();
            let Some(t) = term.get(&key) else { continue };
            let handle = text(t, "handle").unwrap_or_default();
            if worker_terms.contains(&handle) {
                continue;
            }
            let state = if t.get("connected") == Some(&Value::Bool(false)) {
                State::Error
            } else {
                a.get("state").and_then(Value::as_str).and_then(State::from_orca).unwrap_or(State::Idle)
            };
            let since = a
                .get("stateStartedAt")
                .filter(|v| !v.is_null())
                .or_else(|| a.pointer("/mainAgent/stateStartedAt"))
                .and_then(Value::as_f64)
                .map(|f| f as i64);
            let repo_name = repo.and_then(|r| text(r, "displayName")).or_else(|| text(w, "repo")).unwrap_or_else(|| "?".into());
            out.push(Agent {
                workers: busy.get(&handle).copied().unwrap_or(0),
                handle,
                worktree_id: text(w, "worktreeId").unwrap_or_default(),
                label: repo_name.clone(),
                repo: repo_name,
                color: repo.and_then(|r| text(r, "badgeColor")).unwrap_or_else(|| "#444444".into()),
                state,
                since,
                key,
            });
        }
    }
    // ponytail: approximates JS localeCompare with a case-insensitive compare; ICU collation if it matters.
    let lc = |s: &str| s.to_lowercase();
    out.sort_by(|x, y| lc(&x.repo).cmp(&lc(&y.repo)).then_with(|| lc(&x.key).cmp(&lc(&y.key))));
    // Same repo twice or more: number them so the keys can be told apart.
    let mut count: HashMap<String, u32> = HashMap::new();
    for a in &out {
        *count.entry(a.repo.clone()).or_default() += 1;
    }
    let mut seen: HashMap<String, u32> = HashMap::new();
    for a in &mut out {
        if count[&a.repo] > 1 {
            let n = seen.entry(a.repo.clone()).or_default();
            *n += 1;
            a.label = format!("{} {}", a.repo, n);
        }
    }
    out
}

/// Keeps each item on the key it already has; new items fill free keys in key order.
/// `slots` is the ordered list of key contexts, `prev` the last context -> item key map.
/// ponytail: items beyond the number of keys are not shown; add a "+N" key if that bites.
pub fn assign(slots: &[String], items: &[String], prev: &HashMap<String, String>) -> HashMap<String, String> {
    let alive: HashSet<&String> = items.iter().collect();
    let mut next = HashMap::new();
    for s in slots {
        if let Some(k) = prev.get(s).filter(|k| alive.contains(k)) {
            next.insert(s.clone(), k.clone());
        }
    }
    let taken: HashSet<String> = next.values().cloned().collect();
    let mut free = items.iter().filter(|k| !taken.contains(*k));
    for s in slots {
        if !next.contains_key(s) {
            match free.next() {
                Some(k) => next.insert(s.clone(), k.clone()),
                None => break,
            };
        }
    }
    next
}

/// XML-escapes text for SVG content and attributes.
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            _ => o.push(c),
        }
    }
    o
}

/// Black or white text, whichever reads better on `hex`.
fn ink(hex: &str) -> &'static str {
    let mut h: String = hex.replace('#', "");
    while h.len() < 6 {
        h.push('0');
    }
    let digits: String = h.chars().take(6).take_while(|c| c.is_ascii_hexdigit()).collect();
    let n = u32::from_str_radix(&digits, 16).unwrap_or(0);
    let lum = 0.299 * ((n >> 16) & 255) as f64 + 0.587 * ((n >> 8) & 255) as f64 + 0.114 * (n & 255) as f64;
    if lum > 160.0 { "#111111" } else { "#ffffff" }
}

const SANS: &str = r#"font-family="Segoe UI, Arial, sans-serif""#;
const MONO: &str = r#"font-family="Consolas, Cascadia Mono, monospace""#;

/// State icons drawn in a 24x24 box (Lucide-style strokes), placed in a dark disc.
pub fn icon(state: State, frame: u32) -> String {
    match state {
        State::Working => format!(
            r##"<circle cx="12" cy="12" r="8" fill="none" stroke="#f97316" stroke-opacity=".35" stroke-width="3"/><path d="M12 4a8 8 0 0 1 8 8" fill="none" stroke="#f97316" stroke-width="3" stroke-linecap="round" transform="rotate({} 12 12)"/>"##,
            (frame % 8) * 45
        ),
        State::Question => r##"<g fill="none" stroke="#60a5fa" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><path d="M12 17h.01"/></g>"##.into(),
        State::Done => r##"<circle cx="12" cy="12" r="10" fill="#22c55e"/><path d="M7.5 12.5l3 3 6-6.5" fill="none" stroke="#fff" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>"##.into(),
        State::Error => r##"<g fill="none" stroke="#ef4444" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/></g>"##.into(),
        State::Idle => r##"<circle cx="12" cy="12" r="5" fill="#9ca3af"/>"##.into(),
    }
}

/// Time in state: 45s, 12m, 1h05, 2d.
pub fn ago(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86399 => format!("{}h{:02}", s / 3600, (s % 3600) / 60),
        _ => format!("{}d", s / 86400),
    }
}

fn svg(body: &str) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144">{body}</svg>"#)
}

/// Agent key. `None` = empty key. Bottom row: time in the current state (left, only for Question/Done),
/// live workers badge (right). `blink` plus the "on" half of the cycle (frame / 2 even) draws a white ring.
pub fn render_agent(agent: Option<&Agent>, frame: u32, now: i64, blink: bool) -> String {
    let Some(a) = agent else {
        return svg(r##"<rect width="144" height="144" fill="#111"/><circle cx="72" cy="72" r="14" fill="none" stroke="#333" stroke-width="3" stroke-dasharray="5 6"/>"##);
    };
    let fg = ink(&a.color);
    let n = a.label.chars().count();
    let label = if n > 12 { a.label.chars().take(11).collect::<String>() + "…" } else { a.label.clone() };
    let len = label.chars().count();
    let size = if len <= 7 { 24 } else if len <= 10 { 20 } else { 17 };
    let on = blink && (frame / 2) % 2 == 0;
    let time = match a.state {
        State::Question | State::Done => a.since.filter(|&s| s != 0).map(|s| ago(now - s)).unwrap_or_default(),
        _ => String::new(),
    };
    let badge = if a.workers > 0 {
        let d = a.workers.to_string().len() as f64;
        format!(
            r##"<rect x="{}" y="110" width="{}" height="26" rx="13" fill="#fff"/><text x="{}" y="130" text-anchor="middle" {SANS} font-weight="800" font-size="19" fill="#111">⚙{}</text>"##,
            136.0 - (d * 11.0 + 26.0),
            d * 11.0 + 26.0,
            123.0 - d * 5.5,
            a.workers
        )
    } else {
        String::new()
    };
    let mut b = format!(
        r##"<rect width="144" height="144" fill="#000"/><rect x="2" y="2" width="140" height="140" rx="26" fill="{}"/><text x="72" y="30" text-anchor="middle" {SANS} font-weight="700" font-size="{size}" fill="{fg}">{}</text><circle cx="72" cy="74" r="32" fill="#000" fill-opacity=".55"/><g transform="translate(50.4 52.4) scale(1.8)">{}</g>"##,
        esc(&a.color),
        esc(&label),
        icon(a.state, frame)
    );
    if !time.is_empty() {
        b += &format!(r#"<text x="12" y="131" {SANS} font-weight="700" font-size="20" fill="{fg}">{time}</text>"#);
    }
    b += &badge;
    if on {
        b += r##"<rect x="4" y="4" width="136" height="136" rx="24" fill="none" stroke="#fff" stroke-width="7"/>"##;
    }
    svg(&b)
}

/// Local wall-clock time, as much of it as the clock tile needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub hour: u32,
    pub minute: u32,
    pub day: u32,
    /// 1..=12
    pub month: u32,
    /// 0 = Sunday
    pub weekday: u32,
}

/// Side panel tile: HH:MM and short Spanish date ("lun, 9 nov"), as es-ES `toLocale*String`.
pub fn render_clock(t: LocalTime) -> String {
    const WD: [&str; 7] = ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"];
    const MO: [&str; 12] = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic"];
    let day = format!("{}, {} {}", WD[t.weekday as usize % 7], t.day, MO[(t.month as usize + 11) % 12]);
    svg(&format!(
        r##"<rect width="144" height="144" fill="#000"/><text x="72" y="82" text-anchor="middle" {SANS} font-weight="800" font-size="50" fill="#fff">{:02}:{:02}</text><text x="72" y="118" text-anchor="middle" {SANS} font-size="22" fill="#9ca3af">{}</text>"##,
        t.hour,
        t.minute,
        esc(&day)
    ))
}

/// Live agents (main and workers) by state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub working: u32,
    pub question: u32,
    pub done: u32,
}

/// Counts every live agent (main and workers) by state.
pub fn agent_counts(ps: &[Value]) -> Counts {
    let mut c = Counts::default();
    for w in ps {
        if truthy(w.get("isArchived")) {
            continue;
        }
        for a in arr(w, "agents") {
            if a.get("hasLivePty") == Some(&Value::Bool(false)) {
                continue;
            }
            match a.get("state").and_then(Value::as_str).and_then(State::from_orca) {
                Some(State::Working) => c.working += 1,
                Some(State::Question) => c.question += 1,
                Some(State::Done) => c.done += 1,
                _ => {}
            }
        }
    }
    c
}

/// Side panel tile: working / waiting / done, one row each with its state colour.
pub fn render_summary(c: &Counts) -> String {
    let row = |y: i32, color: &str, label: &str, n: u32| {
        format!(
            r##"<circle cx="22" cy="{}" r="9" fill="{color}"/><text x="40" y="{y}" {SANS} font-size="22" fill="#d4d4d4">{label}</text><text x="134" y="{}" text-anchor="end" {SANS} font-weight="800" font-size="30" fill="#fff">{n}</text>"##,
            y - 8,
            y + 1
        )
    };
    svg(&format!(
        r##"<rect width="144" height="144" fill="#000"/>{}{}{}"##,
        row(40, "#f97316", "activos", c.working),
        row(84, "#60a5fa", "espera", c.question),
        row(128, "#22c55e", "hechos", c.done)
    ))
}

/// One quota window of an account. `left` is REMAINING %.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub name: &'static str,
    pub left: f64,
    /// Reset instant, ms since the epoch.
    pub at: Option<i64>,
    /// Window length in minutes.
    pub mins: i64,
    /// Reset label from quota-monitor, e.g. "lun 22:10".
    pub when: String,
}

/// One account from quota-status.json that has at least one number to show.
#[derive(Clone, Debug, PartialEq)]
pub struct QuotaItem {
    pub key: String,
    pub label: String,
    pub stripe: &'static str,
    /// Codex `resetsRemaining`.
    pub resets: Option<f64>,
    pub bars: Vec<Bar>,
}

const WINDOWS: [(&str, &str, i64); 3] = [("session", "5h", 300), ("weekly", "sem", 10080), ("monthly", "mes", 43200)];

/// Short key names (the key fits ~10 monospace chars next to the window tag).
fn short(id: &str) -> Option<&'static str> {
    Some(match id {
        "claude-claude-anpa" => "ANPA",
        "claude-claude-coordinacion" => "COORD",
        "agy-gemini" => "GEMINI",
        "agy-claude-gpt" => "C&GPT",
        "orca-opencodeGo" => "OCODE",
        "agy-argon" => "ARGON",
        _ => return None,
    })
}

fn stripe(provider: Option<&str>) -> &'static str {
    match provider {
        Some("anthropic") => "#d97757",
        Some("openai") => "#10a37f",
        Some("google") => "#4285f4",
        Some("opencode") => "#8b5cf6",
        Some("antigravity") => "#14b8a6",
        _ => "#9ca3af",
    }
}

/// Accounts from quota-status.json with at least one window that has a number.
pub fn quota_items(status: &Value) -> Vec<QuotaItem> {
    arr(status, "items")
        .iter()
        .map(|i| {
            let id = text(i, "id").unwrap_or_default();
            let bars = WINDOWS
                .iter()
                .filter_map(|&(k, name, mins)| {
                    let w = i.get(k)?;
                    let used = w.get("usedPercent")?.as_f64()?;
                    let at = match w.get("resetsAt") {
                        Some(Value::Number(n)) => n.as_f64().map(|f| f as i64),
                        Some(Value::String(s)) if !s.is_empty() => parse_iso_ms(s).or_else(|| s.trim().parse::<f64>().ok().map(|f| f as i64)),
                        _ => None,
                    };
                    let when = w.get("resetsAtFormatted").and_then(Value::as_str).unwrap_or("").replacen(',', "", 1);
                    let when = if when == "-" { String::new() } else { when };
                    Some(Bar { name, left: 100.0 - used, at, mins, when })
                })
                .collect();
            QuotaItem {
                label: short(&id)
                    .map(str::to_string)
                    .or_else(|| text(i, "shortName"))
                    .or_else(|| text(i, "name"))
                    .unwrap_or_else(|| id.clone()),
                key: id,
                stripe: stripe(i.get("provider").and_then(Value::as_str)),
                resets: i.get("resetsRemaining").and_then(Value::as_f64),
                bars,
            }
        })
        .filter(|i| !i.bars.is_empty())
        .collect()
}

fn left_color(p: i64) -> &'static str {
    if p <= 10 { "#ef4444" } else if p <= 30 { "#f59e0b" } else { "#22c55e" }
}

/// JS `Math.round` (halves towards +inf), clamped to 0..=100.
fn clamp(v: f64) -> i64 {
    ((v + 0.5).floor() as i64).clamp(0, 100)
}

/// Card key: colour stripe per provider, NAME + window tag, big remaining % of the main window
/// (5h when present), a second line (other windows, or the reset for a single window), a bar with
/// a white marker = time left in the window, and the reset time at the bottom.
pub fn render_quota(item: Option<&QuotaItem>, stale: bool, now: i64) -> String {
    let Some(item) = item.filter(|i| !i.bars.is_empty()) else { return render_agent(None, 0, now, false) };
    let (main, rest) = (&item.bars[0], &item.bars[1..]);
    let v = clamp(main.left);
    let tag = main.name.to_uppercase() + &item.resets.map(|r| format!(" ↻{r}")).unwrap_or_default();
    let tag_w = tag.chars().count() as f64 * 8.5 + 10.0;
    let name = item.label.to_uppercase();
    let fit = ((126.0 - tag_w) / 8.5).floor().max(1.0) as usize;
    let name_txt = if name.chars().count() > fit { name.chars().take(fit - 1).collect::<String>() + "…" } else { name };
    let reset = if main.when.is_empty() { String::new() } else { format!("↺ {}", main.when) };
    let line2 = if rest.is_empty() {
        format!(r##"<tspan fill="#9ca3af">{}</tspan>"##, esc(&reset))
    } else {
        rest.iter()
            .map(|b| format!(r#"<tspan fill="{}">{} {}%</tspan>"#, left_color(clamp(b.left)), b.name, clamp(b.left)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let line2_size = if rest.len() > 1 { 13 } else { 18 };
    let time_left = main.at.map(|at| ((at - now) as f64 / (main.mins as f64 * 60000.0)).clamp(0.0, 1.0));
    let (bar_x, bar_w, bar_y) = (16.0, 118.0, 104.0);
    let marker = time_left
        .map(|t| format!(r##"<rect x="{}" y="{}" width="4" height="18" rx="2" fill="#fff"/>"##, bar_x + bar_w * t - 2.0, bar_y - 5.0))
        .unwrap_or_default();
    let bottom = if rest.is_empty() { String::new() } else { reset };
    svg(&format!(
        r##"<rect width="144" height="144" fill="#000"/><g opacity="{}"><rect x="0" y="0" width="6" height="144" fill="{}"/><text x="16" y="24" {MONO} font-size="15" fill="#e5e7eb">{}</text><rect x="{}" y="9" width="{tag_w}" height="21" rx="4" fill="#262626"/><text x="{}" y="24" text-anchor="middle" {MONO} font-size="14" fill="#a3a3a3">{}</text><text x="14" y="68" {SANS} font-weight="800" font-size="38" fill="#fff">{v}%</text><text x="16" y="92" {MONO} font-size="{line2_size}">{line2}</text><rect x="{bar_x}" y="{bar_y}" width="{bar_w}" height="8" rx="4" fill="#262626"/><rect x="{bar_x}" y="{bar_y}" width="{}" height="8" rx="4" fill="{}"/>{marker}<text x="16" y="136" {SANS} font-size="18" fill="#9ca3af">{}</text></g>"##,
        if stale { 0.4 } else { 1.0 },
        item.stripe,
        esc(&name_txt),
        136.0 - tag_w,
        136.0 - tag_w / 2.0,
        esc(&tag),
        bar_w * v as f64 / 100.0,
        left_color(v),
        esc(&bottom)
    ))
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

/// ISO-8601 instant ("2026-11-09T21:10:00.073104+00:00", "...Z", "2026-11-09") to ms since the epoch.
/// ponytail: no offset = UTC (JS reads a date-time without offset as local); quota-monitor always writes one.
pub fn parse_iso_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    let num = |a: usize, b: usize| -> Option<i64> { s.get(a..b).filter(|t| t.bytes().all(|c| c.is_ascii_digit())).and_then(|t| t.parse().ok()) };
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    if s.as_bytes().get(4) != Some(&b'-') || s.as_bytes().get(7) != Some(&b'-') || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let mut ms = days_from_civil(y, mo, d) * 86_400_000;
    let rest = &s[10..];
    if rest.is_empty() {
        return Some(ms);
    }
    if !rest.starts_with(['T', 't', ' ']) {
        return None;
    }
    let r = &rest[1..];
    let n2 = |a: usize| -> Option<i64> { r.get(a..a + 2).filter(|t| t.bytes().all(|c| c.is_ascii_digit()))?.parse().ok() };
    let (h, mi) = (n2(0)?, n2(3)?);
    let mut i = 5;
    let mut sec_ms = 0;
    if r.as_bytes().get(5) == Some(&b':') {
        sec_ms = n2(6)? * 1000;
        i = 8;
        if r.as_bytes().get(8) == Some(&b'.') {
            let frac: String = r[9..].chars().take_while(char::is_ascii_digit).collect();
            i = 9 + frac.len();
            sec_ms += format!("{frac:0<3}")[..3].parse::<i64>().ok()?;
        }
    }
    ms += h * 3_600_000 + mi * 60_000 + sec_ms;
    let tz = &r[i..];
    match tz.as_bytes().first() {
        None | Some(b'Z') | Some(b'z') => Some(ms),
        Some(&sign @ (b'+' | b'-')) => {
            let t = tz[1..].replace(':', "");
            let (oh, om): (i64, i64) = (t.get(0..2)?.parse().ok()?, t.get(2..4).unwrap_or("00").parse().ok()?);
            let off = (oh * 60 + om) * 60_000;
            Some(if sign == b'+' { ms - off } else { ms + off })
        }
        _ => None,
    }
}

/// ms since the epoch to an ISO-8601 UTC string, like JS `toISOString()`.
pub fn iso_utc(ms: i64) -> String {
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )
}
