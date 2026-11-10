//! CLI for orcadeck-plugman, same commands as tools/plugin-install/install.mjs.

use orcadeck_plugman::*;
use std::path::PathBuf;

const USAGE: &str = "\
Usage:
  orcadeck-plugman inspect <pluginDir|package> [--json]
  orcadeck-plugman install <package> --plugins <dir> [--data <dir>] [--manifest <json>] [--force]
  orcadeck-plugman watch <inboxDir> --plugins <dir> [--data <dir>] [--once]
  orcadeck-plugman export --data <dir> --out <dir>
  orcadeck-plugman import <dir> --data <dir>";

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let (mut positional, mut opts) = (Vec::<PathBuf>::new(), InstallOptions::default());
    let (mut plugins, mut out, mut json, mut once) = (None, None, false, false);
    while let Some(arg) = args.next() {
        let mut value = || args.next().map(PathBuf::from).ok_or(format!("Missing value for {arg}"));
        match arg.as_str() {
            "--plugins" => plugins = Some(value()?),
            "--data" => opts.data_root = Some(value()?),
            "--out" => out = Some(value()?),
            "--manifest" => opts.manifest_path = Some(value()?),
            "--force" => opts.force = true,
            "--once" => once = true,
            "--json" => json = true,
            _ => positional.push(arg.into()),
        }
    }
    let need_plugins = |p: Option<PathBuf>| p.ok_or("Missing --plugins <dir>".to_string());
    let e = |e: std::io::Error| e.to_string();

    match (command.as_str(), positional.first()) {
        ("inspect", Some(path)) => {
            let r = inspect_package(path).map_err(e)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&r.to_json()).unwrap());
                return Ok(());
            }
            let kind = if r.encrypted { "encrypted" } else { "plain" };
            println!("{}: {kind} manifest, confidence={}", r.uuid, r.confidence.as_str());
            if let Some(m) = &r.manifest {
                println!("  code: {}", r.code_path.as_deref().unwrap_or("(none)"));
                let actions = m.get("Actions").and_then(|a| a.as_array()).cloned().unwrap_or_default();
                println!("  actions: {}", actions.len());
                for a in actions {
                    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    println!("    - {} ({})", s("UUID"), s("Name"));
                }
            }
        }
        ("install", Some(path)) => {
            opts.plugins_root = need_plugins(plugins)?;
            let r = install_package(path, &opts).map_err(e)?;
            println!("Installed {} -> {}", r.reconstruction.uuid, r.dest.display());
            if let Some(o) = &r.overlay {
                let rec = &r.reconstruction;
                println!("Manifest overlay -> {} ({} actions, {})", o.display(), rec.action_count, rec.confidence.as_str());
            } else if r.needs_manifest {
                println!("Encrypted manifest needs a manual overlay: re-run with --manifest <json>.");
            }
        }
        ("watch", Some(path)) => {
            opts.plugins_root = need_plugins(plugins)?;
            watch_packages(path, opts, once).map_err(e)?;
        }
        ("export", _) => {
            let (Some(data), Some(out)) = (opts.data_root, out) else {
                return Err("Missing --data <dir> or --out <dir>".into());
            };
            let n = export_overlays(&data, &out).map_err(e)?;
            println!("Exported {n} overlay(s) to {}", out.display());
        }
        ("import", Some(path)) => {
            let data = opts.data_root.ok_or("Missing --data <dir>")?;
            let n = import_overlays(path, &data).map_err(e)?;
            println!("Imported {n} overlay(s) into {}", data.display());
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}
