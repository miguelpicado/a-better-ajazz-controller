//! HTTP/1.1 size limits (REPO-AUDIT.md §4.2): an oversized or lying `Content-Length` and an
//! oversized header must be refused quickly, without reading the declared body.
use orcadeck_web::{FsStore, Server};
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_BODY: usize = 4 * 1024 * 1024;
const MAX_HEAD: usize = 16 * 1024;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "orcadeck-http-limits-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("profiles")).unwrap();
    fs::create_dir_all(d.join("plugins")).unwrap();
    d
}

fn start(t: &Path) -> Server {
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

/// Sends `req` and reads the reply; fails the test if the server takes longer than the client's
/// 3 s read timeout to close (e.g. because it is still waiting for a body it must reject).
fn send(addr: SocketAddr, req: &[u8]) -> (String, Vec<u8>, Duration) {
    let mut c = TcpStream::connect(addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let t0 = Instant::now();
    c.write_all(req).unwrap();
    let mut out = vec![];
    c.read_to_end(&mut out).unwrap();
    let elapsed = t0.elapsed();
    let split = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    (
        String::from_utf8_lossy(&out[..split]).into_owned(),
        out[split + 4..].to_vec(),
        elapsed,
    )
}

fn assert_status(head: &str, status: u16) {
    assert!(
        head.starts_with(&format!("HTTP/1.1 {status} ")),
        "expected {status}, got {head}"
    );
    assert!(
        head.to_ascii_lowercase().contains("connection: close"),
        "connection not closed: {head}"
    );
}

#[test]
fn body_over_limit_is_refused_before_it_is_read() {
    let t = tmp("body");
    let s = start(&t);
    let req = format!(
        "POST /api/scenes HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n",
        s.addr.port(),
        MAX_BODY + 1
    );
    // Deliberately send no body: the 413 must arrive without the server reading MAX_BODY bytes.
    let (head, body, elapsed) = send(s.addr, req.as_bytes());
    assert_status(&head, 413);
    assert!(
        elapsed < Duration::from_secs(2),
        "waited for a body it must not read: {elapsed:?}"
    );
    assert!(!body.is_empty(), "413 should carry a JSON error body");
    s.stop();
    let _ = fs::remove_dir_all(&t);
}

#[test]
fn lying_or_malformed_content_length_is_rejected() {
    let t = tmp("lengths");
    let s = start(&t);
    let request = |len: &str| {
        format!(
            "POST /api/scenes HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\n\
             Content-Length: {len}\r\n\r\n",
            s.addr.port()
        )
    };
    // usize::MAX is above the cap, not an allocation; an overflowing or negative value is malformed.
    for (len, status) in [
        ("18446744073709551615", 413),
        ("99999999999999999999999", 400),
        ("-1", 400),
    ] {
        let (head, _, elapsed) = send(s.addr, request(len).as_bytes());
        assert_status(&head, status);
        assert!(
            elapsed < Duration::from_secs(2),
            "Content-Length {len} took {elapsed:?}"
        );
    }
    // Only Content-Length bodies are accepted (no chunked decoding in this server).
    let chunked = format!(
        "POST /api/scenes HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\n\
         Transfer-Encoding: chunked\r\n\r\n",
        s.addr.port()
    );
    assert_status(&send(s.addr, chunked.as_bytes()).0, 411);
    s.stop();
    let _ = fs::remove_dir_all(&t);
}

#[test]
fn header_over_limit_is_refused() {
    let t = tmp("head");
    let s = start(&t);
    let pad = "a".repeat(MAX_HEAD + 1);
    let req = format!(
        "GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-Pad: {pad}\r\n\r\n",
        s.addr.port()
    );
    let (head, _, elapsed) = send(s.addr, req.as_bytes());
    assert_status(&head, 431);
    assert!(
        elapsed < Duration::from_secs(2),
        "oversized header took {elapsed:?}"
    );
    s.stop();
    let _ = fs::remove_dir_all(&t);
}
