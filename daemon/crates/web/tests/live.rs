//! The editor tab is its own keep-alive: `/api/live` polls mark it open, and once they stop
//! `Server::wait_idle` returns so the daemon can drop the editor.
use orcadeck_web::{FsStore, Server};
use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    sync::Arc,
    time::{Duration, Instant},
};

fn start() -> Server {
    let t = std::env::temp_dir().join(format!("orcadeck-live-{}", std::process::id()));
    let _ = fs::remove_dir_all(&t);
    fs::create_dir_all(t.join("profiles")).unwrap();
    fs::create_dir_all(t.join("plugins")).unwrap();
    let store = FsStore {
        profiles: t.join("profiles"),
        plugins: t.join("plugins"),
        builtin_plugins: vec![],
        data: t.join("data"),
        legacy_config: None,
        downloads: None,
        inspector: None,
    };
    Server::start(Arc::new(store), 0).unwrap()
}

fn live(s: &Server) -> String {
    let mut c = TcpStream::connect(s.addr).unwrap();
    let req = format!(
        "GET /api/live?since=0 HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-OrcaDeck-Token: {}\r\n\r\n",
        s.addr.port(),
        s.token()
    );
    c.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    c.read_to_string(&mut out).unwrap();
    out
}

#[test]
fn wait_idle_returns_after_the_tab_stops_polling() {
    let s = start();
    assert!(live(&s).starts_with("HTTP/1.1 200"), "live answers an empty frame set");
    let t0 = Instant::now();
    s.wait_idle(Duration::from_secs(60), || false);
    let e = t0.elapsed();
    assert!(e > Duration::from_secs(4) && e < Duration::from_secs(8), "idle after ~5 s, got {e:?}");
}

#[test]
fn wait_idle_gives_up_on_a_page_that_never_shows() {
    let s = start();
    let t0 = Instant::now();
    s.wait_idle(Duration::from_secs(1), || false);
    assert!(t0.elapsed() < Duration::from_secs(3));
}
