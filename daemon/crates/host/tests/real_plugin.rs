//! Runs the real Orca Deck Node plugin against the host. Skipped (passes with a note) when node
//! or the plugin folder is missing. The plugin folder is copied to a temp dir first, so the
//! original is never written to (the plugin truncates its own plugin.log on start), and APPDATA /
//! QUOTA_DIR point at that temp dir so the plugin cannot reach Orca or the real quota file.
//! Override the source with ORCADOCK_PLUGIN=<path to com.miguelpicado.orcadock.sdPlugin>.

use orcadeck_host::{Config, Device, Host, Instance, Output};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::channel;
use std::time::Duration;

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

#[test]
fn orcadock_plugin_paints_and_reacts() {
    let src = std::env::var_os("ORCADOCK_PLUGIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(r"C:\Users\migue\Github\orca-dock\com.miguelpicado.orcadock.sdPlugin")
        });
    if !src.is_dir() || !src.join("manifest.json").is_file() {
        eprintln!(
            "skipped: plugin not found at {} (set ORCADOCK_PLUGIN)",
            src.display()
        );
        return;
    }
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("skipped: node not available");
        return;
    }

    let tmp = std::env::temp_dir().join(format!("orcadeck-host-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let dir = tmp.join("com.miguelpicado.orcadock.sdPlugin");
    copy_dir(&src, &dir).unwrap();
    // The spawned plugin inherits these; this test binary runs nothing else concurrently.
    std::env::set_var("APPDATA", &tmp);
    std::env::set_var("QUOTA_DIR", &tmp);

    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let cfg = Config {
        port: 0,
        devices: vec![Device {
            id: "DEV".into(),
            name: "AKP153E".into(),
            columns: 6,
            rows: 3,
            kind: 0,
        }],
        ..Config::default()
    };
    let host = Host::start(cfg, move |o: Output| {
        let _ = tx.lock().unwrap().send(o);
    })
    .unwrap();

    let plugin = orcadeck_host::load(&dir, None).unwrap();
    assert_eq!(plugin.id, "com.miguelpicado.orcadock");
    let mut clock = Instance::new(&plugin.id, "com.miguelpicado.orcadock.clock", "DEV", 5, 0);
    clock.controller = "Information".into();
    let clock = host.appear(clock);
    let quota = host.appear(Instance::new(
        &plugin.id,
        "com.miguelpicado.orcadock.quota",
        "DEV",
        0,
        0,
    ));
    host.launch(plugin).unwrap();

    let wait = |want: &dyn Fn(&Output) -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(o) if want(&o) => return o,
                Ok(o) => eprintln!("other output: {:.120}", format!("{o:?}")),
                Err(_) => break,
            }
        }
        panic!("timed out");
    };

    let img = wait(&|o| matches!(o, Output::Image { key, .. } if key.context == clock));
    let Output::Image { key, image, .. } = img else {
        unreachable!()
    };
    assert_eq!((key.column, key.row), (5, 0));
    assert!(
        image.starts_with("data:image/svg+xml"),
        "{}",
        &image[..image.len().min(40)]
    );

    // The quota key answers a press with showOk (and writes .wake into the temp QUOTA_DIR).
    // willAppear for it went out with the clock's, before the clock image came back.
    assert!(host.key(&quota, true));
    wait(&|o| matches!(o, Output::ShowOk(key) if key.context == quota));

    host.stop("com.miguelpicado.orcadock");
    let _ = std::fs::remove_dir_all(&tmp);
}
