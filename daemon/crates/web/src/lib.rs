//! OrcaDeck local web editor: an HTTP server on 127.0.0.1 that serves an embedded SPA and a JSON API to edit
//! scenes, cells and actions and to inspect plugins. The daemon starts it on demand and drops it when done.
//!
//! ```no_run
//! # use std::sync::Arc;
//! let store = Arc::new(orcadeck_web::FsStore {
//!     profiles: "data/profiles".into(), plugins: "data/plugins".into(), builtin_plugins: vec![],
//!     data: "data".into(), legacy_config: None,
//! });
//! let server = orcadeck_web::Server::start(store, 0).unwrap();
//! println!("open {}", server.url()); // token in the URL; the page swaps it for a cookie
//! ```
mod api;
mod http;
mod sqlite;
pub mod store;

pub use api::validate_scene;
pub use store::{Error, FsStore, Plugin, SceneInfo, Store};

use std::{
    io,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("OS random generator");
    b
}

/// Stream Dock-style id: base36 `8-4-4-4-12`, uppercase for scenes, lowercase for `ActionID`.
pub fn new_id(upper: bool) -> String {
    let abc: &[u8] = if upper { b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ" } else { b"0123456789abcdefghijklmnopqrstuvwxyz" };
    let mut s = String::with_capacity(36);
    for (i, b) in random::<32>().iter().enumerate() {
        if matches!(i, 8 | 12 | 16 | 20) {
            s.push('-');
        }
        s.push(abc[*b as usize % 36] as char);
    }
    s
}

pub struct Server {
    pub addr: SocketAddr,
    token: String,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Server {
    /// Binds `127.0.0.1:port` (0 = any free port) with a fresh random session token. Each connection is one
    /// request on a short-lived thread; the accept loop blocks, so the idle server uses no CPU.
    pub fn start(store: Arc<dyn Store>, port: u16) -> io::Result<Server> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let addr = listener.local_addr()?;
        let token: String = random::<32>().iter().map(|b| format!("{b:02x}")).collect();
        let ctx = Arc::new(api::Ctx { store, token: token.clone(), port: addr.port(), lock: Mutex::new(()) });
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = thread::spawn(move || {
            for conn in listener.incoming() {
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(conn) = conn else { continue };
                let ctx = ctx.clone();
                thread::spawn(move || serve(&ctx, conn));
            }
        });
        Ok(Server { addr, token, stop, thread: Some(thread) })
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// URL to open in the browser.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/?token={}", self.addr.port(), self.token)
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr); // wake the blocking accept
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn serve(ctx: &api::Ctx, conn: TcpStream) {
    let _ = conn.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = conn.set_write_timeout(Some(Duration::from_secs(10)));
    let resp = match http::read_request(&conn) {
        Ok(req) => api::handle(ctx, req),
        Err(code) => http::Response::error(code, "bad request"),
    };
    let _ = http::write_response(&conn, &resp);
}

#[cfg(test)]
mod tests {
    #[test]
    fn ids_look_like_stream_dock() {
        let a = super::new_id(true);
        assert_eq!(a.len(), 36);
        assert_eq!(a.split('-').map(str::len).collect::<Vec<_>>(), [8, 4, 4, 4, 12]);
        assert!(a.bytes().all(|b| b == b'-' || b.is_ascii_digit() || b.is_ascii_uppercase()));
        assert_ne!(a, super::new_id(true));
        assert!(super::new_id(false).bytes().all(|b| !b.is_ascii_uppercase()));
    }
}
