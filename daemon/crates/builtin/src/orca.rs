//! Orca access: the runtime RPC over its named pipe (described by %APPDATA%\orca\orca-runtime.json)
//! and bringing Orca's window to the front. Behind the `Orca` trait so tests use a mock.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

pub trait Orca: Send {
    /// One runtime RPC call; `Ok(result)` or `Err(message)`.
    fn call(&self, method: &str, params: Value) -> Result<Value, String>;
    /// Brings Orca's main window to the front (restore, switch virtual desktop).
    fn raise(&self) {}
}

/// Real client: newline-delimited JSON frames over the named pipe (~2-25 ms per call).
pub struct PipeOrca {
    runtime_file: PathBuf,
    /// (pipe endpoint, auth token), cached until a call fails.
    meta: Mutex<Option<(String, String)>>,
}

const TIMEOUT: Duration = Duration::from_secs(5);

impl PipeOrca {
    pub fn new(runtime_file: PathBuf) -> Self {
        PipeOrca {
            runtime_file,
            meta: Mutex::new(None),
        }
    }

    /// `%APPDATA%\orca\orca-runtime.json`.
    pub fn from_env() -> Self {
        Self::new(
            PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
                .join("orca")
                .join("orca-runtime.json"),
        )
    }

    fn meta(&self) -> Result<(String, String), String> {
        let mut m = self.meta.lock().unwrap();
        if m.is_none() {
            let raw = std::fs::read_to_string(&self.runtime_file)
                .map_err(|e| format!("{}: {e}", self.runtime_file.display()))?;
            let v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            let pipe = v["transports"]
                .as_array()
                .and_then(|t| t.iter().find(|t| t["kind"] == "named-pipe"))
                .and_then(|t| t["endpoint"].as_str())
                .ok_or("no named pipe in orca-runtime.json")?;
            *m = Some((
                pipe.to_string(),
                v["authToken"].as_str().unwrap_or_default().to_string(),
            ));
        }
        Ok(m.clone().unwrap())
    }

    fn call_once(&self, method: &str, params: &Value) -> Result<Value, String> {
        static ID: AtomicU64 = AtomicU64::new(0);
        let (pipe, token) = self.meta()?;
        let id = format!(
            "orcadeck-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        );
        let line = json!({ "id": id, "authToken": token, "method": method, "params": params })
            .to_string()
            + "\n";
        let m = method.to_string();
        let (tx, rx) = mpsc::channel();
        // ponytail: std pipes have no read timeout, so the call runs on a throwaway thread and a hung
        // pipe leaks it until Orca closes the pipe; overlapped I/O if that ever shows up.
        std::thread::spawn(move || {
            let r = (|| {
                let mut f = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&pipe)
                    .map_err(|e| format!("{m}: {e}"))?;
                f.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
                for l in BufReader::new(f).lines() {
                    let fr: Value = serde_json::from_str(&l.map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                    if truthy(&fr["_keepalive"]) {
                        continue;
                    }
                    return if fr["ok"] == true {
                        Ok(fr["result"].clone())
                    } else {
                        Err(format!(
                            "{m}: {} {}",
                            fr["error"]["code"],
                            fr["error"]["message"].as_str().unwrap_or("")
                        ))
                    };
                }
                Err(format!("{m}: pipe closed"))
            })();
            let _ = tx.send(r);
        });
        rx.recv_timeout(TIMEOUT)
            .map_err(|_| format!("{method} timeout"))?
    }
}

fn truthy(v: &Value) -> bool {
    !matches!(v, Value::Null | Value::Bool(false))
}

impl Orca for PipeOrca {
    /// One retry with a re-read runtime file (Orca restarted: new pipe and token).
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.call_once(method, &params).or_else(|_| {
            *self.meta.lock().unwrap() = None;
            self.call_once(method, &params)
        })
    }

    fn raise(&self) {
        #[cfg(windows)]
        win::raise_orca();
    }
}

/// Local wall-clock time for the clock tile.
#[cfg(windows)]
pub fn local_time() -> crate::core::LocalTime {
    win::local_time()
}

#[cfg(windows)]
mod win {
    //! Win32 port of raise.ps1 (no PowerShell helper) plus GetLocalTime.
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicIsize, Ordering};

    type Hwnd = isize;

    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        ms: u16,
    }

    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(cb: extern "system" fn(Hwnd, isize) -> i32, p: isize) -> i32;
        fn GetWindowThreadProcessId(h: Hwnd, pid: *mut u32) -> u32;
        fn IsWindowVisible(h: Hwnd) -> i32;
        fn IsWindow(h: Hwnd) -> i32;
        fn GetWindowTextLengthW(h: Hwnd) -> i32;
        fn GetWindow(h: Hwnd, cmd: u32) -> Hwnd;
        fn IsIconic(h: Hwnd) -> i32;
        fn ShowWindow(h: Hwnd, cmd: i32) -> i32;
        fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
        fn BringWindowToTop(h: Hwnd) -> i32;
        fn SetForegroundWindow(h: Hwnd) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn QueryFullProcessImageNameW(
            p: *mut c_void,
            flags: u32,
            buf: *mut u16,
            len: *mut u32,
        ) -> i32;
        fn CloseHandle(p: *mut c_void) -> i32;
        fn GetLocalTime(t: *mut SystemTime);
    }

    pub fn local_time() -> crate::core::LocalTime {
        let mut t = SystemTime::default();
        unsafe { GetLocalTime(&mut t) };
        crate::core::LocalTime {
            hour: t.hour.into(),
            minute: t.minute.into(),
            day: t.day.into(),
            month: t.month.into(),
            weekday: t.day_of_week.into(),
        }
    }

    fn is_orca(pid: u32) -> bool {
        unsafe {
            let p = OpenProcess(0x1000, 0, pid); // PROCESS_QUERY_LIMITED_INFORMATION
            if p.is_null() {
                return false;
            }
            let mut buf = [0u16; 520];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(p, 0, buf.as_mut_ptr(), &mut len) != 0;
            CloseHandle(p);
            ok && String::from_utf16_lossy(&buf[..len as usize])
                .to_lowercase()
                .ends_with("\\orca.exe")
        }
    }

    /// Top-level, visible, titled, unowned window of orca.exe (works across virtual desktops).
    extern "system" fn find(h: Hwnd, out: isize) -> i32 {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(h, &mut pid);
            if IsWindowVisible(h) != 0
                && GetWindowTextLengthW(h) > 0
                && GetWindow(h, 4) == 0
                && is_orca(pid)
            {
                *(out as *mut Hwnd) = h;
                return 0;
            }
        }
        1
    }

    pub fn raise_orca() {
        static CACHED: AtomicIsize = AtomicIsize::new(0);
        unsafe {
            let mut h = CACHED.load(Ordering::Relaxed);
            if h == 0 || IsWindow(h) == 0 {
                h = 0;
                EnumWindows(find, &mut h as *mut Hwnd as isize);
                CACHED.store(h, Ordering::Relaxed);
            }
            if h == 0 {
                return;
            }
            if IsIconic(h) != 0 {
                ShowWindow(h, 9); // SW_RESTORE
            }
            keybd_event(0x12, 0, 0, 0); // Alt down/up: lifts Windows' foreground lock
            keybd_event(0x12, 0, 2, 0);
            BringWindowToTop(h);
            SetForegroundWindow(h);
        }
    }
}
