//! Runs the editor over a folder, for manual checks:
//! `cargo run --example serve -- <data-root> [--plugins DIR] [--builtin DIR] [--legacy DIR] [--downloads DIR] [--port N]`
//! `<data-root>` holds `profiles/` (written on save), `state.json` and `manifests/`. Use a copy of your profiles,
//! never Stream Dock's own folder: the `--plugins` folder is only written by the install/remove endpoints.
use std::{path::PathBuf, sync::Arc};

fn main() {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().expect("usage: serve <data-root> [--plugins DIR] [--builtin DIR] [--legacy DIR] [--port N]"));
    let mut store = orcadeck_web::FsStore {
        profiles: root.join("profiles"),
        plugins: root.join("plugins"),
        builtin_plugins: vec![],
        data: root.clone(),
        legacy_config: None,
        downloads: None,
        inspector: None,
    };
    let mut port = 0;
    while let (Some(k), Some(v)) = (args.next(), args.next()) {
        match k.as_str() {
            "--plugins" => store.plugins = v.into(),
            "--builtin" => store.builtin_plugins.push(v.into()),
            "--legacy" => store.legacy_config = Some(v.into()),
            "--downloads" => store.downloads = Some(v.into()),
            "--port" => port = v.parse().expect("port"),
            _ => panic!("unknown option {k}"),
        }
    }
    let server = orcadeck_web::Server::start(Arc::new(store), port).expect("bind");
    println!("{}", server.url());
    std::thread::park();
}
