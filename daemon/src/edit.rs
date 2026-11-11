use crate::ctl;
use orcadeck_web::{FsStore, Server};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditPaths {
    pub profiles: PathBuf,
    pub plugins: PathBuf,
    pub builtin_plugins: Vec<PathBuf>,
    pub data: PathBuf,
    pub legacy_config: Option<PathBuf>,
}

pub fn resolve_paths_with<E, F>(get_env: E, dir_exists: F) -> EditPaths
where
    E: Fn(&str) -> Option<String>,
    F: Fn(&Path) -> bool,
{
    let appdata = get_env("APPDATA")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let profiles = get_env("ORCADECK_PROFILES")
        .map(PathBuf::from)
        .unwrap_or_else(|| appdata.join("HotSpot").join("StreamDock").join("profiles"));

    let plugins = get_env("ORCADECK_PLUGINS")
        .map(PathBuf::from)
        .unwrap_or_else(|| appdata.join("HotSpot").join("StreamDock").join("plugins"));

    let data = get_env("ORCADECK_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| appdata.join("OrcaDeck"));

    let legacy_config = {
        let cand = appdata.join("HotSpot").join("StreamDock").join("config");
        if dir_exists(&cand) {
            Some(cand)
        } else {
            None
        }
    };

    let mut builtin_plugins = Vec::new();
    if let Some(extra) = get_env("ORCADECK_BUILTIN_PLUGINS") {
        for p in extra.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let path = PathBuf::from(p);
            if dir_exists(&path) {
                builtin_plugins.push(path);
            }
        }
    }
    for cand in [
        PathBuf::from(r"C:\Program Files (x86)\Stream Dock AJAZZ Global\plugins"),
        PathBuf::from(r"C:\Program Files\Stream Dock AJAZZ Global\plugins"),
    ] {
        if dir_exists(&cand) && !builtin_plugins.contains(&cand) {
            builtin_plugins.push(cand);
        }
    }

    // If com.miguelpicado.orcadock.sdPlugin is not in plugins dir, look in working dir / repo root
    if !dir_exists(&plugins.join("com.miguelpicado.orcadock.sdPlugin")) {
        for cand in [PathBuf::from("."), PathBuf::from("..")] {
            if dir_exists(&cand.join("com.miguelpicado.orcadock.sdPlugin"))
                && !builtin_plugins.contains(&cand)
            {
                builtin_plugins.push(cand);
                break;
            }
        }
    }

    EditPaths {
        profiles,
        plugins,
        builtin_plugins,
        data,
        legacy_config,
    }
}

pub fn resolve_paths() -> EditPaths {
    resolve_paths_with(|k| std::env::var(k).ok(), |p| p.is_dir())
}

static STOPPED: AtomicBool = AtomicBool::new(false);
/// Thread parked in `run`; Ctrl+C unparks it, so waiting costs no CPU (no polling).
static WAITER: std::sync::OnceLock<thread::Thread> = std::sync::OnceLock::new();

#[cfg(windows)]
mod ctrl_c {
    use super::*;

    unsafe extern "system" fn handler(ctrl_type: u32) -> i32 {
        if ctrl_type == 0 || ctrl_type == 1 || ctrl_type == 2 {
            STOPPED.store(true, Ordering::SeqCst);
            if let Some(t) = WAITER.get() {
                t.unpark();
            }
            1
        } else {
            0
        }
    }

    extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }

    pub fn set_handler() {
        unsafe {
            SetConsoleCtrlHandler(Some(handler), 1);
        }
    }
}

#[cfg(not(windows))]
mod ctrl_c {
    pub fn set_handler() {}
}

pub fn is_stopped() -> bool {
    STOPPED.load(Ordering::SeqCst)
}

/// Waits for Ctrl+C without polling: the handler unparks this thread.
fn wait_for_ctrl_c() {
    let _ = WAITER.set(thread::current());
    ctrl_c::set_handler();
    while !is_stopped() {
        thread::park();
    }
}

fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Quote the URL: `cmd` treats a bare `&` (the install token parameter) as a command separator.
        std::process::Command::new("cmd")
            .arg("/c")
            .raw_arg(format!("start \"\" \"{url}\""))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn editor_store(paths: EditPaths) -> FsStore {
    FsStore {
        profiles: paths.profiles,
        plugins: paths.plugins,
        builtin_plugins: paths.builtin_plugins,
        data: paths.data,
        legacy_config: paths.legacy_config,
        downloads: None, // %USERPROFILE%\Downloads, scanned only when the UI asks
        inspector: None, // the daemon attaches its live host when edit runs inside `run`
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    let mut port: u16 = 0;
    let mut no_open = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                port = args
                    .get(i)
                    .ok_or("se requiere un número de puerto tras --port")?
                    .parse::<u16>()
                    .map_err(|e| format!("puerto inválido: {e}"))?;
            }
            "--no-open" => {
                no_open = true;
            }
            "-h" | "--help" => {
                println!("Uso: orcadeck edit [--port <N>] [--no-open]");
                println!("Sirve el editor web en 127.0.0.1 y lo abre en el navegador.");
                return Ok(());
            }
            other => {
                return Err(format!(
                    "opción desconocida para edit: '{other}'. Uso: orcadeck edit [--port <N>] [--no-open]"
                ));
            }
        }
        i += 1;
    }

    // A live `run` owns the HID device, the plugin host and the editor: serve from inside it and
    // wait here. Its web server exists only while this connection is open (Ctrl+C closes the pipe).
    match ctl::request_edit(port) {
        Some(Ok(session)) => {
            println!(
                "OrcaDeck editor (dentro de orcadeck run): {}",
                session.url()
            );
            println!("Presiona Ctrl+C para detener el servidor.");
            if !no_open {
                if let Err(e) = open_browser(session.url()) {
                    eprintln!(
                        "aviso: no se pudo abrir el navegador ({e}); abre {} a mano",
                        session.url()
                    );
                }
            }
            wait_for_ctrl_c();
            println!("\nDeteniendo el editor...");
            drop(session);
            return Ok(());
        }
        Some(Err(e)) => return Err(format!("orcadeck run no pudo iniciar el editor: {e}")),
        None => {}
    }

    let paths = resolve_paths();
    let store = editor_store(paths);

    let server = Server::start(Arc::new(store), port)
        .map_err(|e| format!("no se pudo iniciar el servidor web: {e}"))?;

    let url = server.url();
    println!("OrcaDeck editor: {url}");
    println!("Presiona Ctrl+C para detener el servidor.");

    if !no_open {
        if let Err(e) = open_browser(&url) {
            eprintln!("aviso: no se pudo abrir el navegador ({e}); abre {url} a mano");
        }
    }

    wait_for_ctrl_c();

    println!("\nDeteniendo el editor...");
    server.stop();
    Ok(())
}

/// `orcadeck install <paquete>`: opens the editor with the package staged. Inside a live `run` the
/// package is staged there over the ctl pipe; otherwise this process serves its own editor.
pub fn run_install(raw: &str) -> Result<(), String> {
    let pkg = orcadeck_web::validate_package(raw).map_err(|e| format!("{raw}: {e}"))?;
    let path = pkg.to_string_lossy().to_string();

    if let Some(res) = ctl::request_install(&path) {
        let session = res.map_err(|e| format!("orcadeck run no pudo abrir el editor: {e}"))?;
        println!("OrcaDeck: instala el plugin en el navegador (dentro de orcadeck run).");
        println!("{}", session.url());
        println!("Presiona Ctrl+C para cerrar el editor.");
        if let Err(e) = open_browser(session.url()) {
            eprintln!(
                "aviso: no se pudo abrir el navegador ({e}); abre {} a mano",
                session.url()
            );
        }
        wait_for_ctrl_c();
        println!("\nDeteniendo el editor...");
        drop(session);
        return Ok(());
    }

    let token = orcadeck_web::stage_package(&path)
        .map_err(|e| format!("no se pudo preparar el paquete: {e}"))?;
    let server = Server::start(Arc::new(editor_store(resolve_paths())), 0)
        .map_err(|e| format!("no se pudo iniciar el servidor web: {e}"))?;
    let url = format!("{}&install={token}", server.url());
    println!("OrcaDeck: instala el plugin en el navegador.");
    println!("{url}");
    println!("Presiona Ctrl+C para cerrar el editor.");
    if let Err(e) = open_browser(&url) {
        eprintln!("aviso: no se pudo abrir el navegador ({e}); abre {url} a mano");
    }
    wait_for_ctrl_c();
    println!("\nDeteniendo el editor...");
    server.stop();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_paths_defaults() {
        let env_map = |k: &str| match k {
            "APPDATA" => Some(r"C:\Users\test\AppData\Roaming".into()),
            _ => None,
        };
        let exists =
            |p: &Path| p == Path::new(r"C:\Users\test\AppData\Roaming\HotSpot\StreamDock\config");

        let paths = resolve_paths_with(env_map, exists);
        assert_eq!(
            paths.profiles,
            PathBuf::from(r"C:\Users\test\AppData\Roaming\HotSpot\StreamDock\profiles")
        );
        assert_eq!(
            paths.plugins,
            PathBuf::from(r"C:\Users\test\AppData\Roaming\HotSpot\StreamDock\plugins")
        );
        assert_eq!(
            paths.data,
            PathBuf::from(r"C:\Users\test\AppData\Roaming\OrcaDeck")
        );
        assert_eq!(
            paths.legacy_config,
            Some(PathBuf::from(
                r"C:\Users\test\AppData\Roaming\HotSpot\StreamDock\config"
            ))
        );
    }

    #[test]
    fn test_resolve_paths_overrides() {
        let env_map = |k: &str| match k {
            "APPDATA" => Some(r"C:\Users\test\AppData\Roaming".into()),
            "ORCADECK_PROFILES" => Some(r"D:\custom\profiles".into()),
            "ORCADECK_PLUGINS" => Some(r"D:\custom\plugins".into()),
            "ORCADECK_DATA" => Some(r"D:\custom\data".into()),
            "ORCADECK_BUILTIN_PLUGINS" => Some(r"D:\custom\builtin1;D:\custom\builtin2".into()),
            _ => None,
        };
        let exists = |p: &Path| {
            p == Path::new(r"D:\custom\builtin1") || p == Path::new(r"D:\custom\builtin2")
        };

        let paths = resolve_paths_with(env_map, exists);
        assert_eq!(paths.profiles, PathBuf::from(r"D:\custom\profiles"));
        assert_eq!(paths.plugins, PathBuf::from(r"D:\custom\plugins"));
        assert_eq!(paths.data, PathBuf::from(r"D:\custom\data"));
        assert_eq!(
            paths.builtin_plugins,
            vec![
                PathBuf::from(r"D:\custom\builtin1"),
                PathBuf::from(r"D:\custom\builtin2"),
            ]
        );
        assert_eq!(paths.legacy_config, None);
    }
}
