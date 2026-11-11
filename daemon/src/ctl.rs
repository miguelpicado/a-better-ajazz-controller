//! Local control channel between `orcadeck run` and `orcadeck edit`.
//!
//! `run` owns the HID device, the plugin host and, on demand, the web editor. `edit` is a client:
//! it asks the live `run` to start the editor inside its own process, receives the URL and holds
//! the connection open. Ctrl+C closes the pipe and `run` drops the web server, so the editor is
//! never resident.
//!
//! Transport is the Windows named pipe `\\.\pipe\orcadeck-ctl` (byte mode, one instance, blocking
//! I/O) through raw `kernel32` calls, like the Ctrl+C handler in `edit.rs`: no crates, and the
//! server thread sleeps in `ConnectNamedPipe`/`ReadFile`, so an idle daemon costs no CPU.
//!
//! Protocol is one ASCII line each way. `EDIT <port>` (`0` = any free port) answers `OK <url>` or
//! `ERR <message>`; `INSTALL <path>` stages a plugin package in the daemon and answers exactly like
//! `EDIT 0`, with `&install=<one-use-token>` appended to the URL. The session lasts until the client
//! disconnects.

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::thread;

    const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;
    const PIPE_TYPE_BYTE: u32 = 0x0000_0000;
    const PIPE_READMODE_BYTE: u32 = 0x0000_0000;
    const PIPE_WAIT: u32 = 0x0000_0000;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const OPEN_EXISTING: u32 = 3;
    const ERROR_PIPE_BUSY: u32 = 231;
    const ERROR_PIPE_CONNECTED: u32 = 535;
    const ERROR_NO_DATA: u32 = 232;
    /// How long a client waits for a busy pipe (another `edit` is attached) before giving up.
    const BUSY_WAIT_MS: u32 = 500;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateNamedPipeW(name: *const u16, open_mode: u32, pipe_mode: u32, max_instances: u32,
            out_size: u32, in_size: u32, timeout: u32, sa: *mut c_void) -> *mut c_void;
        fn ConnectNamedPipe(h: *mut c_void, overlapped: *mut c_void) -> i32;
        fn DisconnectNamedPipe(h: *mut c_void) -> i32;
        fn CreateFileW(name: *const u16, access: u32, share: u32, sa: *mut c_void,
            disposition: u32, flags: u32, template: *mut c_void) -> *mut c_void;
        fn WaitNamedPipeW(name: *const u16, timeout: u32) -> i32;
        fn ReadFile(h: *mut c_void, buf: *mut u8, n: u32, read: *mut u32, overlapped: *mut c_void) -> i32;
        fn WriteFile(h: *mut c_void, buf: *const u8, n: u32, written: *mut u32, overlapped: *mut c_void) -> i32;
        fn CloseHandle(h: *mut c_void) -> i32;
        fn GetLastError() -> u32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// `HANDLE` kept as `isize` so the types that hold it are `Send` without unsafe impls.
    #[derive(Clone, Copy)]
    struct Handle(isize);

    impl Handle {
        fn raw(self) -> *mut c_void {
            self.0 as *mut c_void
        }
        fn valid(self) -> bool {
            self.0 != 0 && self.0 != -1
        }
    }

    /// One end of a request. Closes the handle on drop when it owns it (client side).
    pub struct Conn {
        h: Handle,
        owner: bool,
        /// Server end: it understands the one-shot `INSTALL` request and annotates the editor URL.
        server: bool,
        /// Line already produced by the server end (e.g. the `EDIT` an `INSTALL` becomes).
        pending: Option<String>,
        /// Single-use package token to append to the `OK <url>` reply.
        staged: Option<String>,
    }

    impl Conn {
        /// One request line, without the newline. `None` on EOF or a broken pipe.
        pub fn read_line(&mut self) -> Option<String> {
            if let Some(line) = self.pending.take() {
                return Some(line);
            }
            let line = self.read_raw_line()?;
            if !self.server {
                return Some(line);
            }
            // `INSTALL <path>` from `orcadeck install`: stage the package here, in the process that
            // owns the web server, and let `serve_edit` start the editor as if it had asked for it.
            let Some(path) = super::parse_install(&line) else { return Some(line) };
            match orcadeck_web::stage_package(&path) {
                Ok(token) => {
                    self.pending = Some("EDIT 0".into());
                    self.staged = Some(token);
                    self.read_line()
                }
                Err(e) => Some(format!("INSTALL-ERR {e}")),
            }
        }

        fn read_raw_line(&mut self) -> Option<String> {
            let mut line = Vec::new();
            let mut buf = [0u8; 64];
            loop {
                let mut n = 0u32;
                let ok = unsafe { ReadFile(self.h.raw(), buf.as_mut_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut()) };
                if ok == 0 || n == 0 {
                    return None;
                }
                let read = &buf[..n as usize];
                if let Some(i) = read.iter().position(|b| *b == b'\n') {
                    line.extend_from_slice(&read[..i]);
                    return String::from_utf8(line).ok();
                }
                line.extend_from_slice(read);
                if line.len() > 4096 {
                    return None;
                }
            }
        }

        pub fn write_line(&mut self, line: &str) {
            // Only the editor URL carries the staged token; the client opens it in the browser.
            let out = match (self.staged.take(), line.strip_prefix("OK ")) {
                (Some(token), Some(url)) => format!("OK {url}&install={token}"),
                _ => line.to_string(),
            };
            let mut bytes = out.into_bytes();
            bytes.push(b'\n');
            let mut off = 0;
            while off < bytes.len() {
                let mut n = 0u32;
                let ok = unsafe {
                    WriteFile(self.h.raw(), bytes[off..].as_ptr(), (bytes.len() - off) as u32, &mut n, std::ptr::null_mut())
                };
                if ok == 0 || n == 0 {
                    return;
                }
                off += n as usize;
            }
        }

        /// Blocks until the peer closes. Used by the server to keep the editor alive for exactly as
        /// long as the `edit` process.
        pub fn wait_close(&mut self) {
            let mut buf = [0u8; 64];
            loop {
                let mut n = 0u32;
                let ok = unsafe { ReadFile(self.h.raw(), buf.as_mut_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut()) };
                if ok == 0 || n == 0 {
                    return;
                }
            }
        }
    }

    impl Drop for Conn {
        fn drop(&mut self) {
            if self.owner {
                unsafe { CloseHandle(self.h.raw()) };
            }
        }
    }

    /// Pipe instance a client can already connect to. It survives `accept`, so there is no window
    /// between connections in which `edit` would fall back to standalone.
    pub struct Listener {
        h: Handle,
    }

    impl Listener {
        /// `None` when the OS refuses the name (e.g. max instances reached).
        pub fn new(name: &str) -> Option<Listener> {
            let w = wide(name);
            let h = unsafe {
                CreateNamedPipeW(
                    w.as_ptr(),
                    PIPE_ACCESS_DUPLEX,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                    1,
                    8192,
                    8192,
                    0,
                    std::ptr::null_mut(),
                )
            };
            let h = Handle(h as isize);
            h.valid().then_some(Listener { h })
        }

        /// Waits for one client and runs `handler` for its whole connection. False on a fatal pipe
        /// error (the caller stops serving).
        pub fn accept(&self, handler: impl FnOnce(&mut Conn)) -> bool {
            let h = self.h.raw();
            if unsafe { ConnectNamedPipe(h, std::ptr::null_mut()) } == 0 {
                match unsafe { GetLastError() } {
                    // The client connected before the call, or connected and left already.
                    ERROR_PIPE_CONNECTED => {}
                    ERROR_NO_DATA => {
                        unsafe { DisconnectNamedPipe(h) };
                        return true;
                    }
                    _ => return false,
                }
            }
            handler(&mut Conn { h: self.h, owner: false, server: true, pending: None, staged: None });
            unsafe { DisconnectNamedPipe(h) };
            true
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.h.raw()) };
        }
    }

    /// An edit session served by a live `run`. Dropping it closes the pipe, which is what stops the
    /// embedded web server.
    pub struct Session {
        url: String,
        _conn: Conn,
    }

    impl Session {
        pub fn url(&self) -> &str {
            &self.url
        }
    }

    /// Connects to `name` and sends one request line. `None`: no live `run` (standalone fallback).
    pub fn request_line(name: &str, request: &str) -> Option<Result<Session, String>> {
        let w = wide(name);
        let open = || unsafe {
            CreateFileW(
                w.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        let mut h = Handle(open() as isize);
        if !h.valid() {
            if unsafe { GetLastError() } != ERROR_PIPE_BUSY {
                return None; // no live run (or no permission): standalone editor
            }
            // Another edit owns the single instance: wait briefly, then refuse rather than start a
            // second editor writing the same files.
            unsafe { WaitNamedPipeW(w.as_ptr(), BUSY_WAIT_MS) };
            h = Handle(open() as isize);
            if !h.valid() {
                return if unsafe { GetLastError() } == ERROR_PIPE_BUSY {
                    Some(Err("otro 'orcadeck edit' ya está conectado a run".into()))
                } else {
                    None
                };
            }
        }
        let mut conn = Conn { h, owner: true, server: false, pending: None, staged: None };
        conn.write_line(request);
        let reply = conn.read_line();
        match reply {
            Some(line) => match line.strip_prefix("OK ") {
                // The URL is opened in the browser: never trust the pipe for anything but loopback.
                Some(url) if url.starts_with("http://127.0.0.1:") => Some(Ok(Session { url: url.to_string(), _conn: conn })),
                Some(url) => Some(Err(format!("respuesta inesperada del daemon: {url:?}"))),
                None => Some(Err(line.strip_prefix("ERR ").unwrap_or(&line).to_string())),
            },
            None => Some(Err("run cerró el canal sin responder".into())),
        }
    }

    /// Asks for the editor on `port` (0 = any free port).
    pub fn request(name: &str, port: u16) -> Option<Result<Session, String>> {
        request_line(name, &format!("EDIT {port}"))
    }

    /// Serves `edit` requests forever on `name`, one at a time. Blocking: no polling.
    pub fn serve(name: String, handler: impl Fn(&mut Conn)) {
        let Some(listener) = Listener::new(&name) else {
            return eprintln!("ctl: no se pudo crear el pipe {name}");
        };
        while listener.accept(|c| handler(c)) {}
        eprintln!("ctl: pipe {name} cerrado");
    }

    /// Runs `serve` on its own thread. The daemon keeps its name "ctl" and a small stack: it only
    /// ever handles one short line at a time.
    pub fn spawn(name: &str, handler: impl Fn(&mut Conn) + Send + 'static) {
        let name = name.to_string();
        if let Err(e) = thread::Builder::new().name("ctl".into()).stack_size(128 * 1024).spawn(move || serve(name, handler)) {
            eprintln!("ctl: {e}");
        }
    }
}

#[cfg(not(windows))]
mod imp {
    /// Non-Windows stub: there is no daemon to attach to, so `edit` always runs standalone.
    pub struct Conn;
    impl Conn {
        pub fn read_line(&mut self) -> Option<String> {
            None
        }
        pub fn write_line(&mut self, _line: &str) {}
        pub fn wait_close(&mut self) {}
    }
    pub struct Session;
    impl Session {
        pub fn url(&self) -> &str {
            ""
        }
    }
    pub fn request(_name: &str, _port: u16) -> Option<Result<Session, String>> {
        None
    }
    pub fn request_line(_name: &str, _request: &str) -> Option<Result<Session, String>> {
        None
    }
    pub fn serve(_name: String, _handler: impl Fn(&mut Conn)) {}
    pub fn spawn(_name: &str, _handler: impl Fn(&mut Conn) + Send + 'static) {}
}

pub use imp::{spawn, Conn, Session};
#[cfg(all(test, windows))]
pub(crate) use imp::Listener;

/// Pipe the daemon listens on; `run` owns it, `edit` connects.
pub const NAME: &str = r"\\.\pipe\orcadeck-ctl";

/// Test hook: the same request on an arbitrary pipe name, so tests never touch the real one.
#[cfg(all(test, windows))]
pub(crate) fn request_at(name: &str, port: u16) -> Option<Result<Session, String>> {
    imp::request(name, port)
}

/// Parses the install request. The rest of the line is the path, spaces included.
pub fn parse_install(line: &str) -> Option<String> {
    line.strip_prefix("INSTALL ").map(str::trim).filter(|p| !p.is_empty()).map(String::from)
}

/// Attaches to a live `run` and asks it for the editor with `path` staged for installation.
/// `None` means "no daemon": the caller starts the standalone editor.
pub fn request_install(path: &str) -> Option<Result<Session, String>> {
    imp::request_line(NAME, &format!("INSTALL {path}"))
}

/// Parses the editor request. `EDIT` alone means any free port.
pub fn parse_edit(line: &str) -> Option<u16> {
    let mut w = line.split_whitespace();
    if w.next()? != "EDIT" {
        return None;
    }
    match w.next() {
        None => Some(0),
        Some(port) => port.parse().ok(),
    }
}

/// Attaches to a live `run` and asks it for the editor. `None` means "no daemon": the caller starts
/// the standalone editor.
pub fn request_edit(port: u16) -> Option<Result<Session, String>> {
    imp::request(NAME, port)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn test_pipe(tag: &str) -> String {
        format!(r"\\.\pipe\orcadeck-ctl-test-{tag}-{}", std::process::id())
    }

    #[test]
    fn the_editor_protocol_is_one_line() {
        assert_eq!(parse_edit("EDIT 0"), Some(0));
        assert_eq!(parse_edit("EDIT 8123"), Some(8123));
        assert_eq!(parse_edit("EDIT"), Some(0));
        assert_eq!(parse_edit("EDIT abc"), None);
        assert_eq!(parse_edit("PING 1"), None);
        assert_eq!(parse_edit(""), None);
    }

    #[test]
    fn a_request_gets_its_reply_over_the_pipe() {
        let name = test_pipe("roundtrip");
        let listener = imp::Listener::new(&name).expect("pipe instance");
        let server = std::thread::spawn(move || {
            let mut seen = String::new();
            assert!(listener.accept(|c| {
                seen = c.read_line().expect("request line");
                c.write_line("OK http://127.0.0.1:9/?token=t");
                c.wait_close();
            }));
            seen
        });

        let session = imp::request(&name, 8123).expect("daemon present").expect("url");
        assert_eq!(session.url(), "http://127.0.0.1:9/?token=t");
        drop(session); // what Ctrl+C does: the daemon sees EOF and drops its server

        assert_eq!(server.join().unwrap(), "EDIT 8123");
    }

    #[test]
    fn the_install_protocol_keeps_the_path_after_the_verb() {
        assert_eq!(parse_install(r"INSTALL C:\Users\me\a.streamDeckPlugin").as_deref(), Some(r"C:\Users\me\a.streamDeckPlugin"));
        assert_eq!(parse_install(r"INSTALL C:\mis plugins\b.zip").as_deref(), Some(r"C:\mis plugins\b.zip"));
        assert_eq!(parse_install("INSTALL"), None);
        assert_eq!(parse_install("INSTALL   "), None);
        assert_eq!(parse_install("EDIT 0"), None);
    }

    #[test]
    fn an_install_request_stages_the_file_and_puts_a_token_in_the_url() {
        let name = test_pipe("install");
        let listener = imp::Listener::new(&name).expect("pipe instance");
        let pkg = std::env::temp_dir().join(format!("orcadeck-ctl-test-{}.streamDeckPlugin", std::process::id()));
        std::fs::write(&pkg, b"package body").unwrap();
        let path = pkg.to_string_lossy().to_string();
        let server = std::thread::spawn(move || {
            let mut seen = String::new();
            assert!(listener.accept(|c| {
                seen = c.read_line().expect("request line");
                c.write_line("OK http://127.0.0.1:9/?token=t");
                c.wait_close();
            }));
            seen
        });

        let request = format!("INSTALL {path}");
        let session = imp::request_line(&name, &request).expect("daemon present").expect("url");
        let url = session.url();
        let token = url.strip_prefix("http://127.0.0.1:9/?token=t&install=").expect("staged token in the URL");
        assert_eq!(token.len(), 32);
        assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
        drop(session);

        assert_eq!(server.join().unwrap(), "EDIT 0", "serve_edit sees the editor request it knows");
        let _ = std::fs::remove_file(&pkg);
    }

    #[test]
    fn no_daemon_means_standalone() {
        let name = test_pipe("absent");
        assert!(imp::request(&name, 0).is_none(), "no pipe: edit falls back to its own server");
    }

    #[test]
    fn a_second_editor_is_refused_instead_of_opening_another_server() {
        let name = test_pipe("busy");
        let listener = imp::Listener::new(&name).expect("pipe instance");
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            assert!(listener.accept(|c| {
                c.read_line().expect("first request");
                c.write_line("OK http://127.0.0.1:9/?token=t");
                let _ = release_rx.recv(); // keep the only instance connected
                c.wait_close();
            }));
        });

        let first = imp::request(&name, 0).expect("daemon present").expect("url");
        let second = imp::request(&name, 0).expect("daemon present");
        assert!(second.is_err(), "two editors must not write the same files");

        drop(first);
        let _ = release_tx.send(());
        server.join().expect("server finishes");
    }
}
