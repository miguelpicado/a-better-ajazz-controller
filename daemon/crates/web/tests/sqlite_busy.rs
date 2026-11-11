//! A busy/locked Stream Dock `DataCache.db` (SQLITE_BUSY in the official app) must degrade the
//! legacy scene order cleanly: no panic, no blocking, and a later read still works
//! (REPO-AUDIT.md §4.2). The reader parses the file directly, so "locked" on Windows means another
//! process holds the file without sharing.
use orcadeck_web::{FsStore, Store};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MON: &str = "3I0U64IN-IT4R-RUB6-EZ4Z-VXS311JFXI05";
const ORCA: &str = "R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88";
const REC: &str = "H33G4VTZ-25Z8-PYN7-R34M-ST39SX8TRT2Q";
const ZZZ: &str = "ZZZZZZZZ-ZZZZ-ZZZZ-ZZZZ-ZZZZZZZZZZZZ";

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("orcadeck-sqlite-busy-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Legacy folder with the real fixture DB (3 AKP153E rows) plus four profiles: three in DB order and
/// one extra whose Name sorts first, so the order reveals whether the DB was read.
fn fixture(name: &str) -> PathBuf {
    let t = tmp(name);
    for (id, display) in [(MON, "Monitor"), (ORCA, "Orca"), (REC, "Rec"), (ZZZ, "AAA")] {
        let dir = t.join(format!("profiles/{id}.sdProfile"));
        fs::create_dir_all(&dir).unwrap();
        let manifest = format!(r#"{{"Name":"{display}","DeviceUUID":"AKP153E","Actions":{{}}}}"#);
        fs::write(dir.join("manifest.json"), manifest).unwrap();
    }
    fs::create_dir_all(t.join("legacy")).unwrap();
    fs::write(
        t.join("legacy/StreamDockConfig.ini"),
        "[StreamDockCurrentDevice]\nDevice=AKP153E\nAKP153E\\SerialNumber=355499441494\n\n\
         [AKP153E355499441494DeviceConfig]\nsdProfileName=".to_string() + ORCA + ".sdProfile\n",
    )
    .unwrap();
    fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataCache.db"), t.join("legacy/DataCache.db"))
        .unwrap();
    t
}

fn store(t: &Path) -> FsStore {
    FsStore {
        profiles: t.join("profiles"),
        plugins: t.join("plugins"),
        builtin_plugins: vec![],
        data: t.join("data"),
        legacy_config: Some(t.join("legacy")),
        downloads: None,
        inspector: None,
    }
}

fn ids(s: &FsStore) -> Vec<String> {
    s.scenes().unwrap().into_iter().map(|s| s.id).collect()
}

#[test]
fn locked_datacache_degrades_without_blocking_and_recovers() {
    let t = fixture("locked");
    let s = store(&t);
    let db = t.join("legacy/DataCache.db");
    // Baseline: the device order comes from DataCache.db.
    assert_eq!(ids(&s), [MON, ORCA, REC, ZZZ]);

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // No sharing at all, as an exclusive SQLite/Stream Dock handle would be.
        let lock = fs::OpenOptions::new().read(true).share_mode(0).open(&db).unwrap();
        assert!(fs::read(&db).is_err(), "the test lock must really block readers");

        let t0 = Instant::now();
        let got = ids(&s); // must return, not block on the lock or panic
        let elapsed = t0.elapsed();
        assert!(elapsed < Duration::from_secs(2), "scenes() blocked for {elapsed:?} on a busy DB");
        // No DB order: plain name order, the documented degraded result.
        assert_eq!(got, [ZZZ, MON, ORCA, REC]);

        drop(lock);
        assert_eq!(ids(&s), [MON, ORCA, REC, ZZZ], "the order must come back once the DB is readable");
    }
    let _ = fs::remove_dir_all(&t);
}

#[test]
fn truncated_datacache_is_ignored_without_panicking() {
    let t = fixture("truncated");
    let db = t.join("legacy/DataCache.db");
    let bytes = fs::read(&db).unwrap();
    // Valid 100-byte SQLite header, missing pages: the b-tree walk fails cleanly.
    fs::write(&db, &bytes[..120]).unwrap();

    let t0 = Instant::now();
    let got = ids(&store(&t));
    assert!(t0.elapsed() < Duration::from_secs(2));
    assert_eq!(got, [ZZZ, MON, ORCA, REC]);
    let _ = fs::remove_dir_all(&t);
}
