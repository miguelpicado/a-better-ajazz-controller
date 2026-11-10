//! Ports of tools/plugin-install/install.test.mjs plus zip-slip, encryption detection,
//! needs-manifest/provided, export/import and watcher cases. All fixtures are synthetic.

use orcadeck_plugman::*;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const UUID: &str = "com.example.demo";

/// Temp folder removed on drop.
struct Root(PathBuf);

impl Root {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("orca-sd-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Root(p)
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn encrypted_manifest() -> Vec<u8> {
    let mut b = b"ELGATO".to_vec();
    b.extend([1, 0]);
    b.extend([0u8; 64]);
    b
}

/// Same fixture as the Node test: ELGATO manifest, en.json with two actions, bin/plugin.js.
fn fixture(root: &Path) -> PathBuf {
    let dir = root.join(format!("{UUID}.sdPlugin"));
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("manifest.json"), encrypted_manifest()).unwrap();
    let locale = json!({
        "Name": "Demo",
        "com.example.demo.launch": { "Name": "Launch", "Tooltip": "Start it", "States": [{ "Name": "Idle" }] },
        "com.example.demo.stop": { "Name": "Stop", "States": [{ "Name": "Idle" }] },
    });
    fs::write(dir.join("en.json"), locale.to_string()).unwrap();
    fs::write(dir.join("bin/plugin.js"), r#"const actions = ["com.example.demo.launch", "com.example.demo.stop"];"#).unwrap();
    dir
}

fn action_uuids(r: &Reconstruction) -> Vec<String> {
    r.manifest.as_ref().unwrap()["Actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["UUID"].as_str().unwrap().to_string())
        .collect()
}

/// Zip `dir` (a `<uuid>.sdPlugin` folder) so entries start with its folder name.
fn zip_dir(dir: &Path, zip_path: &Path) {
    let mut zip = zip::ZipWriter::new(fs::File::create(zip_path).unwrap());
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let base = dir.parent().unwrap();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let name = p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            zip.start_file(name, opts).unwrap();
            zip.write_all(&fs::read(&p).unwrap()).unwrap();
        }
    }
    zip.finish().unwrap();
}

fn opts(root: &Path) -> InstallOptions {
    InstallOptions {
        plugins_root: root.join("plugins"),
        data_root: Some(root.join("data")),
        ..Default::default()
    }
}

#[test]
fn reconstructs_an_encrypted_manifest_from_locale_and_code_path() {
    let root = Root::new();
    let dir = fixture(&root.0);
    let r = reconstruct_manifest(&dir);
    assert!(r.encrypted);
    assert_eq!(r.confidence, Confidence::Full);
    assert_eq!(r.manifest.as_ref().unwrap()["CodePath"], "bin/plugin.js");
    assert_eq!(r.manifest.as_ref().unwrap()["Nodejs"]["Version"], "20");
    assert_eq!(action_uuids(&r), ["com.example.demo.launch", "com.example.demo.stop"]);
    let launch = &r.manifest.as_ref().unwrap()["Actions"][0];
    assert_eq!(launch["Name"], "Launch");
    assert_eq!(launch["Tooltip"], "Start it");
    assert_eq!(launch["States"], json!([{ "Name": "Idle" }]));
}

#[test]
fn falls_back_to_code_literals_when_there_is_no_locale_or_registry() {
    let root = Root::new();
    let dir = fixture(&root.0);
    fs::write(dir.join("en.json"), "{}").unwrap();
    fs::write(dir.join("bin/plugin.js"), r#"const a = "com.example.demo.launch"; const b = "com.example.other.action";"#).unwrap();
    let r = reconstruct_manifest(&dir);
    assert_eq!(r.confidence, Confidence::Full);
    let ids = action_uuids(&r);
    assert!(ids.contains(&"com.example.demo.launch".into()));
    assert!(ids.contains(&"com.example.other.action".into()));
}

#[test]
fn scans_exe_literals_in_ascii_and_utf16le() {
    let root = Root::new();
    let dir = fixture(&root.0);
    fs::remove_file(dir.join("en.json")).unwrap();
    fs::remove_dir_all(dir.join("bin")).unwrap();
    let mut exe = b"MZ\0\0com.example.demo.alpha\0com.example.demo.sdPlugin\0com.example.demo.helper.dll\0".to_vec();
    if exe.len() % 2 == 1 {
        exe.push(0);
    }
    exe.extend("com.example.demo.beta_".encode_utf16().flat_map(u16::to_le_bytes));
    fs::write(dir.join("demo.exe"), exe).unwrap();
    let r = reconstruct_manifest(&dir);
    assert_eq!(r.code_path.as_deref(), Some("demo.exe"));
    assert_eq!(action_uuids(&r), ["com.example.demo.alpha", "com.example.demo.beta"]);
    assert_eq!(r.confidence, Confidence::Full);
    assert!(r.manifest.as_ref().unwrap().get("Nodejs").is_none());
}

#[test]
fn leaves_a_plain_manifest_untouched() {
    let root = Root::new();
    let dir = fixture(&root.0);
    let plain = json!({ "UUID": UUID, "CodePath": "index.html", "Actions": [{ "UUID": "com.example.demo.launch", "Name": "Launch" }] });
    fs::write(dir.join("manifest.json"), plain.to_string()).unwrap();
    let r = reconstruct_manifest(&dir);
    assert!(!r.encrypted);
    assert_eq!(r.confidence, Confidence::Plain);
    assert_eq!(r.manifest, Some(plain));
}

#[test]
fn installs_a_zip_and_writes_the_overlay() {
    let root = Root::new();
    let dir = fixture(&root.0);
    let zip = root.0.join(format!("{UUID}.streamDeckPlugin"));
    zip_dir(&dir, &zip);
    let o = opts(&root.0);
    let r = install_package(&zip, &o).unwrap();
    assert_eq!(r.reconstruction.uuid, UUID);
    let installed = o.plugins_root.join(format!("{UUID}.sdPlugin"));
    assert!(installed.join("bin/plugin.js").exists());
    // The plugin folder is a byte copy: the encrypted manifest is untouched.
    assert_eq!(fs::read(installed.join("manifest.json")).unwrap(), encrypted_manifest());
    let overlay = root.0.join("data/manifests").join(format!("{UUID}.json"));
    assert_eq!(r.overlay.as_deref(), Some(overlay.as_path()));
    assert!(!r.needs_manifest);
    let written: Value = serde_json::from_str(&fs::read_to_string(&overlay).unwrap()).unwrap();
    assert_eq!(written["Actions"].as_array().unwrap().len(), 2);

    let e = install_package(&zip, &o).unwrap_err();
    assert!(e.to_string().contains("already exists"), "{e}");
    install_package(&zip, &InstallOptions { force: true, ..o.clone() }).unwrap();
    let names: Vec<String> = fs::read_dir(&o.plugins_root).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into()).collect();
    assert!(names.iter().any(|n| n.starts_with(&format!("{UUID}.sdPlugin.bak-"))), "{names:?}");
}

#[test]
fn rejects_zip_slip_entries() {
    for bad in ["../evil.txt", "/abs.txt", "\\abs.txt", "C:/evil.txt", "c:evil.txt", "x.sdPlugin/../../evil.txt", "a\\..\\evil.txt", "a/b.txt:stream"] {
        let root = Root::new();
        let zip_path = root.0.join("bad.streamDeckPlugin");
        let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        zip.start_file(format!("{UUID}.sdPlugin/manifest.json"), o).unwrap();
        zip.write_all(b"{}").unwrap();
        zip.start_file(bad, o).unwrap();
        zip.write_all(b"pwned").unwrap();
        zip.finish().unwrap();

        let e = install_package(&zip_path, &opts(&root.0)).unwrap_err();
        assert!(e.to_string().contains("Unsafe path"), "{bad}: {e}");
        assert!(inspect_package(&zip_path).is_err(), "{bad}");
        assert!(!root.0.join("plugins").exists(), "{bad}: nothing may be installed");
        assert!(!root.0.parent().unwrap().join("evil.txt").exists());
    }
    assert!(check_entry_name("com.x.sdPlugin/imgs/a..b.png").is_ok());
}

#[test]
fn detects_manifest_state_without_decrypting() {
    let root = Root::new();
    let dir = fixture(&root.0);
    assert_eq!(read_manifest_state(&dir), ManifestState::Encrypted { size: 72 });
    fs::write(dir.join("manifest.json"), "\u{feff}{\"UUID\":\"x\"}").unwrap();
    assert_eq!(read_manifest_state(&dir), ManifestState::Plain(json!({ "UUID": "x" })));
    fs::write(dir.join("manifest.json"), "ELGAT0 not json").unwrap();
    assert_eq!(read_manifest_state(&dir), ManifestState::Invalid { size: 15 });
    fs::remove_file(dir.join("manifest.json")).unwrap();
    assert_eq!(read_manifest_state(&dir), ManifestState::Missing);
}

#[test]
fn partial_needs_manifest_then_vendor_manifest_is_provided() {
    let root = Root::new();
    let dir = fixture(&root.0);
    fs::remove_dir_all(dir.join("bin")).unwrap(); // actions but no CodePath
    let o = opts(&root.0);
    let r = install_package(&dir, &o).unwrap();
    assert_eq!(r.reconstruction.confidence, Confidence::Partial);
    assert!(r.needs_manifest && r.overlay.is_none());
    assert!(!root.0.join("data/manifests").exists());

    let vendor = root.0.join("vendor.json");
    fs::write(&vendor, json!({ "UUID": UUID, "CodePath": "x.exe", "Actions": [{ "UUID": "com.example.demo.a" }] }).to_string()).unwrap();
    let r = install_package(&dir, &InstallOptions { force: true, manifest_path: Some(vendor.clone()), ..o.clone() }).unwrap();
    assert_eq!(r.reconstruction.confidence, Confidence::Provided);
    assert_eq!(r.reconstruction.action_count, 1);
    assert!(!r.needs_manifest && r.overlay.unwrap().exists());

    fs::write(&vendor, "{\"UUID\":\"x\"}").unwrap();
    assert!(install_package(&dir, &InstallOptions { force: true, manifest_path: Some(vendor), ..o }).is_err());
}

#[test]
fn exports_and_imports_overlays() {
    let root = Root::new();
    let dir = fixture(&root.0);
    install_package(&dir, &opts(&root.0)).unwrap();
    let out = root.0.join("overlays");
    assert_eq!(export_overlays(&root.0.join("data"), &out).unwrap(), 1);
    let other = root.0.join("other-data");
    assert_eq!(import_overlays(&out, &other).unwrap(), 1);
    assert!(other.join("manifests").join(format!("{UUID}.json")).exists());

    fs::write(out.join("broken.json"), "{\"UUID\":\"x\"}").unwrap();
    let fresh = root.0.join("fresh-data");
    assert!(import_overlays(&out, &fresh).is_err());
    assert!(!fresh.exists(), "a bad file must block the whole import");
}

#[test]
fn watcher_waits_for_a_stable_size() {
    let root = Root::new();
    let dir = fixture(&root.0);
    let inbox = root.0.join("inbox");
    fs::create_dir_all(&inbox).unwrap();
    let mut w = Watcher::new(&inbox, opts(&root.0)).unwrap();
    assert!(w.scan().is_empty());

    zip_dir(&dir, &inbox.join("demo.streamDeckPlugin"));
    fs::write(inbox.join("notes.txt"), "ignored").unwrap();
    assert!(w.poll().is_empty());
    assert!(w.poll().is_empty());
    let events = w.poll();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1.as_ref().unwrap().reconstruction.confidence, Confidence::Full);
    for _ in 0..4 {
        assert!(w.poll().is_empty(), "an unchanged package is installed once");
    }
}
