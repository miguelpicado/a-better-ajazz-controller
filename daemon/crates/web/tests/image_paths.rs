//! Profile/plugin icon resolution must never escape its folder (REPO-AUDIT.md §4.2): `..`,
//! absolute and drive/UNC paths, bad scene/plugin ids and links are all refused by `Store::image`,
//! which is what the `/api/image` route uses.
use orcadeck_web::{FsStore, Server, Store};
use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::Arc,
};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nimage";
const SECRET: &[u8] = b"SECRET-OUTSIDE-THE-PLUGIN";

fn w(p: impl AsRef<Path>, b: impl AsRef<[u8]>) {
    fs::create_dir_all(p.as_ref().parent().unwrap()).unwrap();
    fs::write(p, b).unwrap();
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "orcadeck-image-paths-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Temp copy of the layout: one profile image, one plugin image, and secrets outside both.
fn fixture(name: &str) -> PathBuf {
    let t = tmp(name);
    w(t.join("profiles/S.sdProfile/Images/ok.png"), PNG);
    w(t.join("plugins/com.test.demo.sdPlugin/imgs/one.png"), PNG);
    w(
        t.join("plugins/com.test.demo.sdPlugin/manifest.json"),
        r#"{"Name":"Demo","Actions":[{"UUID":"com.test.demo.one","Name":"One"}]}"#,
    );
    // Exists outside the plugin/profile folders, so a missing check would actually read it.
    w(t.join("secret.png"), SECRET);
    w(t.join("plugins/secret.png"), SECRET);
    t
}

fn store(t: &Path) -> FsStore {
    FsStore {
        profiles: t.join("profiles"),
        plugins: t.join("plugins"),
        builtin_plugins: vec![],
        data: t.join("data"),
        legacy_config: None,
        downloads: None,
        inspector: None,
    }
}

#[test]
fn store_image_confines_scene_and_plugin_paths() {
    let t = fixture("store");
    let s = store(&t);

    // Baseline: real images still resolve, including extension probing.
    assert_eq!(
        s.image(Some("S"), None, "ok.png"),
        Some((PNG.to_vec(), "image/png"))
    );
    assert_eq!(
        s.image(None, Some("com.test.demo"), "imgs/one"),
        Some((PNG.to_vec(), "image/png"))
    );

    let absolute = t.join("secret.png").to_string_lossy().into_owned();
    // Every hostile shape must resolve to None, not to a file outside the base folder.
    for img in [
        "../../secret.png",
        r"..\..\secret.png",
        "imgs/../../secret.png",
        r"imgs\..\..\secret.png",
        "../secret.png",
        "..",
        "/Windows/win.ini",
        r"C:\Windows\win.ini",
        r"\\server\share\x.png",
        "imgs/one.png:zone",
    ] {
        assert_eq!(
            s.image(None, Some("com.test.demo"), img),
            None,
            "plugin image {img:?} escaped"
        );
        assert_eq!(
            s.image(Some("S"), None, img),
            None,
            "scene image {img:?} escaped"
        );
    }
    assert_eq!(
        s.image(None, Some("com.test.demo"), &absolute),
        None,
        "absolute plugin image escaped"
    );
    assert_eq!(
        s.image(Some("S"), None, &absolute),
        None,
        "absolute scene image escaped"
    );

    // Ids are folder names: traversal in either id is refused before touching the filesystem.
    assert_eq!(s.image(Some(r"..\..\S"), None, "ok.png"), None);
    assert_eq!(
        s.image(None, Some(r"..\..\com.test.demo"), "imgs/one.png"),
        None
    );

    // Scene image names are bare names only; a relative path is never resolved against Images/.
    assert_eq!(s.image(Some("S"), None, "imgs/one.png"), None);
    let _ = fs::remove_dir_all(&t);
}

#[cfg(windows)]
#[test]
fn symlinked_icon_cannot_escape_its_folder() {
    use std::os::windows::fs::symlink_file;

    let t = fixture("symlink");
    let link = t.join("plugins/com.test.demo.sdPlugin/link.png");
    if symlink_file(t.join("secret.png"), &link).is_err() {
        // Creating symlinks needs Developer Mode / SeCreateSymbolicLinkPrivilege; without it the
        // containment check below is already covered by the `..` and absolute-path cases.
        return;
    }
    assert_eq!(
        store(&t).image(None, Some("com.test.demo"), "link.png"),
        None,
        "a link escaping the plugin was served"
    );
    let _ = fs::remove_dir_all(&t);
}

fn get(s: &Server, path: &str) -> (String, Vec<u8>) {
    let mut c = TcpStream::connect(s.addr).unwrap();
    c.set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .unwrap();
    write!(
        c,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-OrcaDeck-Token: {}\r\n\r\n",
        s.addr.port(),
        s.token()
    )
    .unwrap();
    let mut out = vec![];
    c.read_to_end(&mut out).unwrap();
    let split = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    (
        String::from_utf8_lossy(&out[..split]).into_owned(),
        out[split + 4..].to_vec(),
    )
}

#[test]
fn http_image_route_refuses_escapes_with_404() {
    let t = fixture("http");
    let s = Server::start(Arc::new(store(&t)), 0).unwrap();

    // Baseline through the real route.
    let (head, body) = get(&s, "/api/image?uuid=com.test.demo&img=imgs/one");
    assert!(head.starts_with("HTTP/1.1 200 "), "{head}");
    assert_eq!(body, PNG);

    for (what, query) in [
        ("parent", "uuid=com.test.demo&img=..%2F..%2Fsecret.png"),
        (
            "backslash parent",
            "uuid=com.test.demo&img=..%5C..%5Csecret.png",
        ),
        (
            "drive absolute",
            "uuid=com.test.demo&img=C%3A%5CWindows%5Cwin.ini",
        ),
        (
            "root absolute",
            "uuid=com.test.demo&img=%2FWindows%2Fwin.ini",
        ),
        ("UNC", "uuid=com.test.demo&img=%5C%5Cserver%5Cshare%5Cx.png"),
        ("scene traversal", "scene=..%5C..&uuid=x&img=ok.png"),
        (
            "plugin traversal",
            "uuid=..%5C..%5Ccom.test.demo&img=imgs%2Fone.png",
        ),
    ] {
        let (head, body) = get(&s, &format!("/api/image?{query}"));
        assert!(head.starts_with("HTTP/1.1 404 "), "{what}: {head}");
        assert!(
            !body.windows(SECRET.len()).any(|w| w == SECRET),
            "{what}: leaked file contents"
        );
    }
    s.stop();
    let _ = fs::remove_dir_all(&t);
}
