//! Every assertion of the Node plugin's test.js, ported, plus registry (mocked Orca) and raster tests.
use orcadeck_builtin::core::*;
use orcadeck_builtin::{frame, raster, Deck, Feedback, Key, Orca, Registry, POLL_MS, QUOTA_MS, SPIN_MS, WORKERS_MS};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn fixture() -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let ps = json!([
        { "repoId": "r1", "repo": "laya", "parentWorktreeId": null, "agents": [
            { "paneKey": "t1:l1", "parentPaneKey": null, "state": "working" },
            { "paneKey": "t1:l2", "parentPaneKey": "t1:l1", "state": "working" },
            { "paneKey": "t1:l3", "parentPaneKey": null, "state": "waiting" } ] },
        { "repoId": "r2", "repo": "anpa", "parentWorktreeId": null, "agents": [
            { "paneKey": "t2:l1", "parentPaneKey": null, "state": "blocked" },
            { "paneKey": "t2:l9", "parentPaneKey": null, "state": "done" },
            { "paneKey": "t2:l2", "parentPaneKey": null, "state": "done" } ] },
        { "repoId": "r2", "repo": "anpa", "parentWorktreeId": "x", "agents": [
            { "paneKey": "t3:l1", "parentPaneKey": null, "state": "done" } ] }
    ]);
    let terms = json!([
        { "tabId": "t1", "leafId": "l1", "handle": "h1" }, { "tabId": "t1", "leafId": "l2", "handle": "h2" },
        { "tabId": "t1", "leafId": "l3", "handle": "h3" }, { "tabId": "t2", "leafId": "l1", "handle": "h4" },
        { "tabId": "t2", "leafId": "l2", "handle": "h5" }, { "tabId": "t3", "leafId": "l1", "handle": "h6" }
    ]);
    let repos = json!([{ "id": "r1", "displayName": "laya", "badgeColor": "#ff0000" }, { "id": "r2", "displayName": "anpa", "badgeColor": "#00ff00" }]);
    let a = |v: Value| v.as_array().unwrap().clone();
    (a(ps), a(terms), a(repos))
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn sorted(m: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut v: Vec<_> = m.clone().into_iter().collect();
    v.sort();
    v
}

#[test]
fn test_js_ok() {
    let (mut ps, terms, repos) = fixture();
    let a = main_agents(&ps, &terms, &[json!({ "terminal": "h5" })], &repos, &[]);
    let got: Vec<_> = a.iter().map(|x| (x.handle.as_str(), x.label.as_str(), x.state.as_str(), x.color.as_str())).collect();
    assert_eq!(got, [("h4", "anpa", "error", "#00ff00"), ("h1", "laya 1", "working", "#ff0000"), ("h3", "laya 2", "question", "#ff0000")]);

    // New agents fill free keys in order; existing ones keep their key.
    let keys: Vec<String> = a.iter().map(|x| x.key.clone()).collect();
    let slots = s(&["k1", "k2"]);
    let m = assign(&slots, &keys, &HashMap::from([("k2".into(), "t1:l3".into())]));
    assert_eq!(sorted(&m), [("k1".into(), "t2:l1".into()), ("k2".into(), "t1:l3".into())]);
    // Agent gone -> key freed and refilled.
    let keys2: Vec<String> = keys.iter().filter(|k| *k != "t2:l1").cloned().collect();
    let m = assign(&slots, &keys2, &m);
    assert_eq!(sorted(&m), [("k1".into(), "t1:l1".into()), ("k2".into(), "t1:l3".into())]);

    assert!(render_agent(Some(&a[0]), 0, 0, false).starts_with("<svg"));
    assert!(render_agent(None, 0, 0, false).starts_with("<svg"));

    // Workers badge: live workers of a run counted on its coordinator's key; worker panes use agentTerminalHandle.
    ps[0]["agents"][0]["stateStartedAt"] = json!(1000);
    let runs = [json!({ "id": "run1", "coordinator_handle": "h1" })];
    let wk = [
        json!({ "runId": "run1", "agentTerminalHandle": "h5", "projection": { "outcome": "in_progress" } }),
        json!({ "runId": "run1", "agentTerminalHandle": "h9", "projection": { "outcome": "succeeded" } }),
    ];
    let b = main_agents(&ps, &terms, &wk, &repos, &runs);
    let got: Vec<_> = b.iter().map(|x| (x.handle.as_str(), x.workers)).collect();
    assert_eq!(got, [("h4", 0), ("h1", 1), ("h3", 0)]);
    assert_eq!(b[1].since, Some(1000));
    let img = render_agent(Some(&b[1]), 0, 1000 + 13 * 60000, true);
    assert!(!img.contains(">13m<"), "Working agent does not show elapsed time");
    assert!(img.contains("⚙1"));
    assert!(img.contains(r#"stroke-width="7""#), "blink ring on frame 0");
    assert!(!render_agent(Some(&b[1]), 2, 0, true).contains(r#"stroke-width="7""#), "off half");

    assert_eq!([ago(45_000), ago(12 * 60_000), ago(65 * 60_000), ago(50 * 3_600_000)], ["45s", "12m", "1h05", "2d"]);
    assert_eq!(agent_counts(&ps), Counts { working: 2, question: 1, done: 3 });
    let sum = render_summary(&Counts { working: 2, question: 1, done: 3 });
    let (i2, i1, i3) = (sum.find(">2<").unwrap(), sum.find(">1<").unwrap(), sum.find(">3<").unwrap());
    assert!(i2 < i1 && i1 < i3);
    // 2026-11-09 (a Monday) 09:05, as new Date(2026, 10, 9, 9, 5).
    let clock = render_clock(LocalTime { hour: 9, minute: 5, day: 9, month: 11, weekday: 1 });
    assert!(clock.contains("09:05"));
    assert!(clock.contains("lun, 9 nov"), "es-ES short date");
}

#[test]
fn test_js_ok_quota() {
    // Accounts with no numbers are dropped; missing windows are skipped.
    let q = quota_items(&json!({ "items": [
        { "id": "c", "shortName": "Claude", "session": { "usedPercent": 13 }, "weekly": { "usedPercent": 50 } },
        { "id": "o", "shortName": "OpenCodeGo", "session": { "usedPercent": 0 }, "weekly": { "usedPercent": 0 }, "monthly": { "usedPercent": 71 } },
        { "id": "x", "shortName": "cursor", "session": { "usedPercent": null }, "weekly": { "usedPercent": null } }
    ] }));
    let got: Vec<_> = q.iter().map(|i| (i.key.as_str(), i.bars.iter().map(|b| b.name).collect::<Vec<_>>().join("/"))).collect();
    assert_eq!(got, [("c", "5h/sem".to_string()), ("o", "5h/sem/mes".to_string())]);
    let svg = render_quota(Some(&q[1]), true, 0);
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains(r#"opacity="0.4""#), "stale dims the card");
}

#[test]
fn quota_card_details() {
    let now = parse_iso_ms("2026-11-09T19:10:00Z").unwrap();
    let q = quota_items(&json!({ "items": [{ "id": "claude-claude-anpa", "provider": "anthropic", "resetsRemaining": 2,
        "session": { "usedPercent": 11, "resetsAt": "2026-11-09T21:10:00.073104+00:00", "resetsAtFormatted": "lun, 22:10" },
        "weekly": { "usedPercent": 95, "resetsAtFormatted": "-" } }] }));
    let i = &q[0];
    assert_eq!((i.label.as_str(), i.stripe, i.bars[0].when.as_str(), i.bars[1].when.as_str()), ("ANPA", "#d97757", "lun 22:10", ""));
    assert_eq!(i.bars[0].at, Some(now + 2 * 3_600_000 + 73));
    let svg = render_quota(Some(i), false, now);
    assert!(svg.contains(">89%<") && svg.contains("5H ↻2") && svg.contains("sem 5%") && svg.contains("↺ lun 22:10"));
    // 2 h left of a 5 h window -> marker at 16 + 118 * 0.4 - 2 (plus the 73 ms).
    assert!(svg.contains(r#"<rect x="61.20"#), "{svg}");
}

#[test]
fn iso_round_trip() {
    for ms in [0, 1_762_715_400_123, 951_782_400_000 /* 2000-02-29 */] {
        assert_eq!(parse_iso_ms(&iso_utc(ms)), Some(ms));
    }
    assert_eq!(parse_iso_ms("2026-11-09T22:10:00+01:00"), parse_iso_ms("2026-11-09T21:10:00Z"));
    assert_eq!(parse_iso_ms("nope"), None);
}

/// Answers from a method -> result table and records every call.
struct Mock {
    replies: Arc<Mutex<HashMap<String, Value>>>,
    calls: Arc<Mutex<Vec<String>>>,
}

impl Orca for Mock {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.calls.lock().unwrap().push(format!("{method} {params}"));
        self.replies.lock().unwrap().get(method).cloned().ok_or_else(|| format!("{method}: unknown"))
    }
    fn raise(&self) {
        self.calls.lock().unwrap().push("raise".into());
    }
}

fn key(uuid: &str, ctx: &str, col: u32) -> Key {
    Key { uuid: format!("com.miguelpicado.orcadock.{uuid}"), context: ctx.into(), device: "d".into(), row: 0, col }
}

#[test]
fn registry_with_mocked_orca() {
    let (ps, terms, repos) = fixture();
    let replies = Arc::new(Mutex::new(HashMap::from([
        ("repo.list".to_string(), json!({ "repos": repos })),
        ("worktree.ps".to_string(), json!({ "worktrees": ps })),
        ("terminal.list".to_string(), json!({ "terminals": terms })),
        ("orchestration.workerList".to_string(), json!({ "workers": [{ "terminal": "h5" }] })),
        ("orchestration.runList".to_string(), json!({ "runs": [] })),
        ("terminal.focus".to_string(), json!({})),
    ])));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let dir = std::env::temp_dir().join(format!("orcadeck-builtin-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("quota-status.json"), r#"{"items":[{"id":"c","shortName":"Claude","session":{"usedPercent":40}}]}"#).unwrap();

    let mut reg = Registry::new(Deck::new(Box::new(Mock { replies: replies.clone(), calls: calls.clone() }), dir.clone()));
    assert!(!reg.appear(key("nope", "x", 0)), "unknown UUIDs stay with the plugin host");
    for (k, c, col) in [("agent", "a2", 1), ("agent", "a1", 0), ("summary", "s", 5), ("quota", "q", 2)] {
        assert!(reg.appear(key(k, c, col)));
    }
    let t0 = 1_000_000;
    let paints: HashMap<_, _> = reg.tick(t0).into_iter().collect();
    assert_eq!(paints.len(), 4);
    assert!(paints["a1"].contains(">anpa<"), "first key in layout order gets the first agent");
    assert!(paints["a2"].contains(">laya 1<"));
    assert!(paints["s"].contains(">activos<"));
    assert!(paints["q"].contains(">60%<"));
    assert!(reg.tick(t0 + 10).is_empty(), "unchanged keys are not resent");

    // laya 1 turns done -> its key blinks (ring on the "on" frames) until pressed.
    replies.lock().unwrap().get_mut("worktree.ps").unwrap()["worktrees"][0]["agents"][0]["state"] = json!("done");
    let t1 = t0 + 2000; // frame 0: "on" half of the blink, and now - t0 == POLL_MS (2000)
    let paints: HashMap<_, _> = reg.tick(t1).into_iter().collect();
    assert!(paints["a2"].contains(r#"stroke-width="7""#));
    assert!(reg.deck.is_blinking("t1:l1", t1));

    calls.lock().unwrap().clear();
    assert_eq!(reg.key_down("a2", t1), Feedback::None);
    assert_eq!(*calls.lock().unwrap(), ["raise", r#"terminal.focus {"terminal":"h1"}"#]);
    assert!(!reg.deck.is_blinking("t1:l1", t1));
    replies.lock().unwrap().remove("terminal.focus");
    assert_eq!(reg.key_down("a1", t1), Feedback::Alert, "failed focus shows an alert");

    // Quota press writes .wake and answers Ok.
    assert_eq!(reg.key_down("q", t1), Feedback::Ok);
    assert!(std::fs::read_to_string(dir.join(".wake")).unwrap().ends_with('Z'));
    let _ = std::fs::remove_dir_all(&dir);
}

fn px_at(rgba: &[u8], px: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * px + x) * 4) as usize;
    rgba[i..i + 4].try_into().unwrap()
}

#[test]
fn raster_agent_key_85_and_side_cell_80() {
    let (ps, terms, repos) = fixture();
    let agents = main_agents(&ps, &terms, &[], &repos, &[]);
    let a = agents.iter().find(|a| a.color == "#ff0000").unwrap(); // laya, red
    for px in [85u32, 80] {
        let rgba = raster::svg_to_rgba(&render_agent(Some(a), 0, 0, false), px).unwrap();
        assert_eq!(rgba.len(), (px * px * 4) as usize);
        assert_eq!(px_at(&rgba, px, px * 15 / 144, px * 72 / 144), [255, 0, 0, 255], "repo colour");
        assert_eq!(px_at(&rgba, px, 0, 0), [0, 0, 0, 255], "black corner");
        // Label text is white on red: fonts were found and text was drawn.
        let label_px = (px * 12 / 144..px * 32 / 144)
            .flat_map(|y| (0..px).map(move |x| (x, y)))
            .filter(|&(x, y)| px_at(&rgba, px, x, y)[1] > 200)
            .count();
        assert!(label_px > 10, "{px}px: {label_px} label pixels");
    }
    let clock = raster::svg_to_rgba(&render_clock(LocalTime { hour: 9, minute: 5, day: 9, month: 11, weekday: 1 }), 80).unwrap();
    assert!(clock.chunks(4).filter(|p| p[0] > 200).count() > 50, "clock digits drawn");
}

#[test]
fn cadence_constants_match_perf_audit_and_owner_requirements() {
    assert_eq!(SPIN_MS, 250, "SPIN_MS must be 250 ms (4 fps, one revolution in 2 s)");
    assert_eq!(POLL_MS, 2000, "POLL_MS must be 2000 ms (2 s)");
    assert_eq!(WORKERS_MS, 30_000, "WORKERS_MS must be 30_000 ms (30 s)");
    assert_eq!(QUOTA_MS, 60_000, "QUOTA_MS must be 60_000 ms (60 s)");
}

#[test]
fn working_spinner_rotation_and_frame_cadence() {
    // Frame advances every 250 ms (SPIN_MS) and wraps modulo 8 (one revolution in 2000 ms).
    assert_eq!(frame(0), 0);
    assert_eq!(frame(100), 0);
    assert_eq!(frame(249), 0);
    assert_eq!(frame(250), 1);
    assert_eq!(frame(499), 1);
    assert_eq!(frame(500), 2);
    assert_eq!(frame(749), 2);
    assert_eq!(frame(750), 3);
    assert_eq!(frame(1000), 4);
    assert_eq!(frame(1250), 5);
    assert_eq!(frame(1500), 6);
    assert_eq!(frame(1750), 7);
    assert_eq!(frame(1999), 7);
    assert_eq!(frame(2000), 0);
    assert_eq!(frame(2250), 1);

    // Consecutive frames 0..7 render different SVGs (both in core::icon and render_agent).
    let working = Agent {
        key: "w1".into(),
        handle: "h1".into(),
        worktree_id: "wt1".into(),
        repo: "test".into(),
        label: "test".into(),
        color: "#f97316".into(),
        state: State::Working,
        since: Some(1000),
        workers: 0,
    };

    for f in 0..8 {
        let next_f = (f + 1) % 8;
        let icon_curr = icon(State::Working, f);
        let icon_next = icon(State::Working, next_f);
        assert_ne!(icon_curr, icon_next, "icon must change between frame {f} and {next_f}");
        assert!(icon_curr.contains(&format!("rotate({} 12 12)", f * 45)));

        let svg_curr = render_agent(Some(&working), f, 5000, false);
        let svg_next = render_agent(Some(&working), next_f, 5000, false);
        assert_ne!(svg_curr, svg_next, "rendered agent SVG must differ between frame {f} and {next_f}");
    }
}

#[test]
fn question_done_white_ring_alternation() {
    for state in [State::Question, State::Done] {
        let agent = Agent {
            key: "a".into(),
            handle: "h".into(),
            worktree_id: "wt".into(),
            repo: "repo".into(),
            label: "repo".into(),
            color: "#00ff00".into(),
            state,
            since: Some(1000),
            workers: 0,
        };

        // When blink is true:
        // Frames 0 and 1: ON (frame / 2 % 2 == 0)
        // Frames 2 and 3: OFF (frame / 2 % 2 == 1)
        // Frames 4 and 5: ON
        // Frames 6 and 7: OFF
        for f in [0u32, 1, 4, 5] {
            let svg = render_agent(Some(&agent), f, 2000, true);
            assert!(svg.contains(r#"stroke-width="7""#), "state {state:?} frame {f} must have white ring");
        }
        for f in [2u32, 3, 6, 7] {
            let svg = render_agent(Some(&agent), f, 2000, true);
            assert!(!svg.contains(r#"stroke-width="7""#), "state {state:?} frame {f} must NOT have white ring");
        }

        // When blink is false: white ring never appears regardless of frame.
        for f in 0..8 {
            let svg = render_agent(Some(&agent), f, 2000, false);
            assert!(!svg.contains(r#"stroke-width="7""#), "state {state:?} frame {f} with blink=false must NOT have white ring");
        }
    }

    // Blinking window durations.
    assert_eq!(orcadeck_builtin::blink_ms(State::Question), Some(60_000));
    assert_eq!(orcadeck_builtin::blink_ms(State::Done), Some(20_000));
    assert_eq!(orcadeck_builtin::blink_ms(State::Working), None);
    assert_eq!(orcadeck_builtin::blink_ms(State::Idle), None);
    assert_eq!(orcadeck_builtin::blink_ms(State::Error), None);
}

#[test]
fn elapsed_time_shown_only_for_question_and_done() {
    let make = |state: State| Agent {
        key: "a".into(),
        handle: "h".into(),
        worktree_id: "wt".into(),
        repo: "repo".into(),
        label: "repo".into(),
        color: "#444444".into(),
        state,
        since: Some(10_000),
        workers: 0,
    };

    // Working shows NO elapsed time text at all (spinner stays clean).
    let working = make(State::Working);
    for now in [10_000, 10_500, 11_000, 20_000, 70_000] {
        let svg = render_agent(Some(&working), 0, now, false);
        assert!(!svg.contains("y=\"131\""), "Working agent must not render elapsed time text");
        assert!(!svg.contains(">0s<") && !svg.contains(">1s<") && !svg.contains(">10s<") && !svg.contains(">1m<"));
    }

    // Idle and Error show no elapsed time.
    for state in [State::Idle, State::Error] {
        let svg = render_agent(Some(&make(state)), 0, 20_000, false);
        assert!(!svg.contains("y=\"131\""), "{state:?} agent must not render elapsed time text");
    }

    // Question shows elapsed time formatted every second under 60 s, then m/h/d.
    let question = make(State::Question);
    let s0 = render_agent(Some(&question), 0, 10_000, false);
    assert!(s0.contains(">0s<"), "Question at start shows 0s");

    let s0_half = render_agent(Some(&question), 0, 10_500, false);
    assert!(s0_half.contains(">0s<"), "Question within first second still shows 0s");

    let s1 = render_agent(Some(&question), 0, 11_000, false);
    assert!(s1.contains(">1s<"), "Question after 1 second shows 1s");

    let s2 = render_agent(Some(&question), 0, 12_000, false);
    assert!(s2.contains(">2s<"), "Question after 2 seconds shows 2s");

    let s59 = render_agent(Some(&question), 0, 69_999, false);
    assert!(s59.contains(">59s<"), "Question at 59s shows 59s");

    let s60 = render_agent(Some(&question), 0, 70_000, false);
    assert!(s60.contains(">1m<"), "Question at 60s shows 1m");

    // Done also shows elapsed time.
    let done = make(State::Done);
    assert!(render_agent(Some(&done), 0, 10_000, false).contains(">0s<"));
    assert!(render_agent(Some(&done), 0, 15_000, false).contains(">5s<"));
}

#[test]
fn registry_tick_repaints_only_changed_keys() {
    let ps = json!([
        { "repoId": "r1", "repo": "r1", "parentWorktreeId": null, "agents": [
            { "paneKey": "k:a_work", "parentPaneKey": null, "state": "working" },
            { "paneKey": "k:b_q", "parentPaneKey": null, "state": "waiting", "stateStartedAt": 1_000_000 },
            { "paneKey": "k:c_idle", "parentPaneKey": null, "state": "idle" }
        ]}
    ]);
    let terms = json!([
        { "tabId": "k", "leafId": "a_work", "handle": "h_work" },
        { "tabId": "k", "leafId": "b_q", "handle": "h_q" },
        { "tabId": "k", "leafId": "c_idle", "handle": "h_idle" }
    ]);
    let repos = json!([{ "id": "r1", "displayName": "repo1", "badgeColor": "#333333" }]);
    let replies = Arc::new(Mutex::new(HashMap::from([
        ("repo.list".to_string(), json!({ "repos": repos })),
        ("worktree.ps".to_string(), json!({ "worktrees": ps })),
        ("terminal.list".to_string(), json!({ "terminals": terms })),
        ("orchestration.workerList".to_string(), json!({ "workers": [] })),
        ("orchestration.runList".to_string(), json!({ "runs": [] })),
    ])));

    let dir = std::env::temp_dir().join(format!("orcadeck-tick-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let mut reg = Registry::new(Deck::new(Box::new(Mock { replies, calls: Arc::new(Mutex::new(Vec::new())) }), dir.clone()));
    assert!(reg.appear(key("agent", "c_work", 0)));
    assert!(reg.appear(key("agent", "c_q", 1)));
    assert!(reg.appear(key("agent", "c_idle", 2)));

    let t0 = 1_000_000;
    // Tick 0: initial appearance, all 3 keys are returned.
    let p0: HashMap<_, _> = reg.tick(t0).into_iter().collect();
    assert_eq!(p0.len(), 3);
    assert!(p0.contains_key("c_work"));
    assert!(p0.contains_key("c_q"));
    assert!(p0.contains_key("c_idle"));

    // Set blinking on question agent (simulating the state transition into waiting).
    reg.deck.blinking.insert("k:b_q".into(), t0 + 60_000);
    // Refresh shown entry for c_q with blinking on frame 0 so tick 1 compares against the blinking ring on.
    reg.tick(t0);

    // Tick 1 (t0 + 250):
    // - c_work rotates spinner to frame 1: MUST repaint.
    // - c_idle is unchanged: MUST NOT repaint.
    // - c_q has frame 1 (same on-half, same second 0s): unchanged, MUST NOT repaint.
    let p1: HashMap<_, _> = reg.tick(t0 + 250).into_iter().collect();
    assert_eq!(p1.len(), 1, "only working agent repaints at 250 ms: {p1:?}");
    assert!(p1.contains_key("c_work"));
    assert!(!p1.contains_key("c_idle"), "idle key must never repaint if state has not changed");
    assert!(!p1.contains_key("c_q"), "question key does not repaint when ring & timer did not change");

    // Tick 2 (t0 + 500):
    // - c_work rotates spinner to frame 2: MUST repaint.
    // - c_idle is unchanged: MUST NOT repaint.
    // - c_q frame 2 switches ring from ON to OFF: MUST repaint.
    let p2: HashMap<_, _> = reg.tick(t0 + 500).into_iter().collect();
    assert_eq!(p2.len(), 2, "working and question ring toggle repaint: {p2:?}");
    assert!(p2.contains_key("c_work"));
    assert!(p2.contains_key("c_q"));
    assert!(!p2.contains_key("c_idle"), "idle key must never repaint");

    // Tick 3 (t0 + 750):
    // - c_work rotates spinner to frame 3: MUST repaint.
    // - c_idle is unchanged: MUST NOT repaint.
    // - c_q frame 3 is still OFF and elapsed time is still 0s: MUST NOT repaint.
    let p3: HashMap<_, _> = reg.tick(t0 + 750).into_iter().collect();
    assert_eq!(p3.len(), 1);
    assert!(p3.contains_key("c_work"));
    assert!(!p3.contains_key("c_idle"));
    assert!(!p3.contains_key("c_q"));

    // Tick 4 (t0 + 1000):
    // - c_work rotates spinner to frame 4: MUST repaint.
    // - c_idle is unchanged: MUST NOT repaint.
    // - c_q ring turns back ON and elapsed time ticks to 1s: MUST repaint.
    let p4: HashMap<_, _> = reg.tick(t0 + 1000).into_iter().collect();
    assert_eq!(p4.len(), 2);
    assert!(p4.contains_key("c_work"));
    assert!(p4.contains_key("c_q"));
    assert!(!p4.contains_key("c_idle"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn registry_next_timeout_cadence() {
    let ps_working = json!([
        { "repoId": "r1", "repo": "r1", "parentWorktreeId": null, "agents": [
            { "paneKey": "k:a_work", "parentPaneKey": null, "state": "working" }
        ]}
    ]);
    let ps_timer = json!([
        { "repoId": "r1", "repo": "r1", "parentWorktreeId": null, "agents": [
            { "paneKey": "k:b_q", "parentPaneKey": null, "state": "waiting", "stateStartedAt": 1_000_000 }
        ]}
    ]);
    let ps_idle = json!([
        { "repoId": "r1", "repo": "r1", "parentWorktreeId": null, "agents": [
            { "paneKey": "k:c_idle", "parentPaneKey": null, "state": "idle" }
        ]}
    ]);
    let terms = json!([
        { "tabId": "k", "leafId": "a_work", "handle": "h_work" },
        { "tabId": "k", "leafId": "b_q", "handle": "h_q" },
        { "tabId": "k", "leafId": "c_idle", "handle": "h_idle" }
    ]);
    let repos = json!([{ "id": "r1", "displayName": "repo1", "badgeColor": "#333333" }]);
    let replies = Arc::new(Mutex::new(HashMap::from([
        ("repo.list".to_string(), json!({ "repos": repos })),
        ("worktree.ps".to_string(), json!({ "worktrees": ps_working })),
        ("terminal.list".to_string(), json!({ "terminals": terms })),
        ("orchestration.workerList".to_string(), json!({ "workers": [] })),
        ("orchestration.runList".to_string(), json!({ "runs": [] })),
    ])));

    let dir = std::env::temp_dir().join(format!("orcadeck-timeout-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let mut reg = Registry::new(Deck::new(Box::new(Mock { replies: replies.clone(), calls: Arc::new(Mutex::new(Vec::new())) }), dir.clone()));
    assert!(reg.appear(key("agent", "c1", 0)));

    let t0 = 1_000_000;
    // 1. Working agent visible: cadence is SPIN_MS (250 ms)
    reg.tick(t0);
    assert_eq!(reg.next_timeout(t0), std::time::Duration::from_millis(SPIN_MS as u64));

    // 2. Question / timer agent visible (without working or blinking): cadence is 1000 ms
    replies.lock().unwrap().insert("worktree.ps".to_string(), json!({ "worktrees": ps_timer }));
    reg.tick(t0 + 2000);
    assert_eq!(reg.next_timeout(t0 + 2000), std::time::Duration::from_millis(1000));

    // 3. Idle agent only (no working, no blinking, no timer): cadence is slow (POLL_MS = 2000 ms)
    replies.lock().unwrap().insert("worktree.ps".to_string(), json!({ "worktrees": ps_idle }));
    reg.tick(t0 + 4000);
    assert_eq!(reg.next_timeout(t0 + 4000), std::time::Duration::from_millis(POLL_MS as u64));

    let _ = std::fs::remove_dir_all(&dir);
}
