//! Error paths of the real named-pipe client (`PipeOrca`): Orca closing mid-call, the pipe never
//! appearing, and Orca restarting with a new pipe endpoint + auth token (REPO-AUDIT.md §4.2).
//!
//! The server side is a raw Win32 named pipe created here, so no Orca process is needed and every
//! test uses a process-unique pipe name and a temp `orca-runtime.json`.
#![cfg(windows)]

use orcadeck_builtin::{Orca, PipeOrca};
use serde_json::{json, Value};
use std::ffi::{c_void, OsStr};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

const ERROR_PIPE_CONNECTED: i32 = 535;

#[link(name = "kernel32")]
extern "system" {
    fn CreateNamedPipeW(
        name: *const u16,
        open_mode: u32,
        pipe_mode: u32,
        max_instances: u32,
        out_buffer_size: u32,
        in_buffer_size: u32,
        default_timeout: u32,
        security: *mut c_void,
    ) -> *mut c_void;
    fn ConnectNamedPipe(pipe: *mut c_void, overlapped: *mut c_void) -> i32;
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("orcadeck-orca-pipe-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn pipe_name(label: &str) -> String {
    format!("orcadeck-test-{}-{label}", std::process::id())
}

fn write_runtime(path: &Path, pipe: &str, token: &str) {
    let doc = json!({
        "transports": [{ "kind": "named-pipe", "endpoint": format!(r"\\.\pipe\{pipe}") }],
        "authToken": token,
    });
    fs::write(path, doc.to_string()).unwrap();
}

/// Creates one blocking byte-mode pipe instance and waits for the client (`PipeOrca`) to open it.
fn accept(name: &str) -> File {
    let path = format!(r"\\.\pipe\{name}");
    let wide: Vec<u16> = OsStr::new(&path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // PIPE_ACCESS_DUPLEX=3, PIPE_TYPE_BYTE=0, PIPE_READMODE_BYTE=0, PIPE_WAIT=0
    let h = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            3,
            0,
            1,
            64 * 1024,
            64 * 1024,
            0,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(
        h as isize,
        -1,
        "CreateNamedPipeW({path}): {}",
        std::io::Error::last_os_error()
    );
    if unsafe { ConnectNamedPipe(h, std::ptr::null_mut()) } == 0 {
        let e = std::io::Error::last_os_error();
        // A client may connect between CreateNamedPipe and ConnectNamedPipe; that is not a failure.
        assert_eq!(
            e.raw_os_error(),
            Some(ERROR_PIPE_CONNECTED),
            "ConnectNamedPipe: {e}"
        );
    }
    unsafe { File::from_raw_handle(h) }
}

fn read_request(pipe: &mut File) -> String {
    let mut line = String::new();
    BufReader::new(pipe).read_line(&mut line).unwrap();
    line
}

#[test]
fn pipe_closed_mid_call_returns_err_without_panic() {
    let dir = tmp("closed");
    let runtime = dir.join("orca-runtime.json");
    let name = pipe_name("closed");
    write_runtime(&runtime, &name, "token");

    let server = thread::spawn(move || {
        let mut pipe = accept(&name);
        let line = read_request(&mut pipe);
        assert!(
            line.contains(r#""method":"status""#),
            "unexpected request: {line}"
        );
        // Drop without a response: the client sees EOF, not a hang.
    });

    let orca = PipeOrca::new(runtime);
    let t0 = Instant::now();
    let err = orca
        .call("status", json!({}))
        .expect_err("a pipe closed mid-call must be an error");
    let elapsed = t0.elapsed();
    server.join().unwrap();

    assert!(elapsed < Duration::from_secs(5), "call took {elapsed:?}");
    assert!(
        err.starts_with("status"),
        "error should name the method: {err}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_pipe_and_runtime_file_return_err_promptly() {
    let dir = tmp("missing");
    let runtime = dir.join("orca-runtime.json");
    write_runtime(&runtime, &pipe_name("never-created"), "token");

    let orca = PipeOrca::new(runtime.clone());
    let t0 = Instant::now();
    assert!(
        orca.call("status", json!({})).is_err(),
        "a pipe that never appears must be an error"
    );
    assert!(
        t0.elapsed() < Duration::from_secs(2),
        "open failure must not wait for TIMEOUT"
    );

    // Orca gone and its runtime file removed: the cached endpoint must be dropped and reported.
    fs::remove_file(&runtime).unwrap();
    let t0 = Instant::now();
    let err = orca.call("status", json!({})).unwrap_err();
    assert!(
        t0.elapsed() < Duration::from_secs(2),
        "missing runtime file must not wait for TIMEOUT"
    );
    assert!(
        err.contains("orca-runtime.json"),
        "error should name the runtime file: {err}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn reacquires_pipe_and_token_after_orca_restart() {
    let dir = tmp("reconnect");
    let runtime = dir.join("orca-runtime.json");
    let (pipe_a, pipe_b) = (pipe_name("restart-a"), pipe_name("restart-b"));
    write_runtime(&runtime, &pipe_a, "token-a");

    // Orca's first process answers with an application error, then goes away.
    let (seen_tx, seen_rx) = mpsc::channel();
    let (go_tx, go_rx) = mpsc::channel();
    let server_a = thread::spawn(move || {
        let mut pipe = accept(&pipe_a);
        let line = read_request(&mut pipe);
        seen_tx.send(line).unwrap();
        go_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        pipe.write_all(br#"{"ok":false,"error":{"code":"restart","message":"bye"}}"#)
            .unwrap();
        pipe.write_all(b"\n").unwrap();
    });

    // The restarted Orca listens on a new endpoint and requires the new token.
    let pipe_b_thread = pipe_b.clone();
    let server_b = thread::spawn(move || {
        let mut pipe = accept(&pipe_b_thread);
        let line = read_request(&mut pipe);
        pipe.write_all(b"{\"_keepalive\":true}\n").unwrap();
        pipe.write_all(br#"{"ok":true,"result":{"version":"9"}}"#)
            .unwrap();
        pipe.write_all(b"\n").unwrap();
        line
    });

    let orca = Arc::new(PipeOrca::new(runtime.clone()));
    let client = {
        let orca = orca.clone();
        thread::spawn(move || orca.call("status", json!({ "q": 1 })))
    };

    let first = seen_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("first pipe never got the request");
    write_runtime(&runtime, &pipe_b, "token-b");
    go_tx.send(()).unwrap();

    let result = client.join().unwrap();
    let second = server_b.join().unwrap();
    server_a.join().unwrap();

    let first: Value = serde_json::from_str(first.trim()).unwrap();
    assert_eq!(first["authToken"], "token-a");
    assert_eq!(first["method"], "status");
    let second: Value = serde_json::from_str(second.trim()).unwrap();
    assert_eq!(
        second["authToken"], "token-b",
        "the retry must re-read the runtime file"
    );
    assert_eq!(second["method"], "status");
    assert_eq!(second["params"]["q"], 1);
    assert_eq!(result.unwrap(), json!({ "version": "9" }));
    let _ = fs::remove_dir_all(&dir);
}
