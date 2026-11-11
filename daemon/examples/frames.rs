#![allow(dead_code)]

#[path = "../src/proto.rs"]
mod proto;
#[path = "../src/render.rs"]
mod render;

use image::{GenericImage, Rgba, RgbaImage};
use orcadeck_builtin::core::{render_agent, Agent, State};
use std::fs;

fn render_label(text: &str, w: u32, h: u32, font_size: u32, color: &str) -> RgbaImage {
    let px = w.max(h).max(32);
    let text_y = (h * 7 / 10).max(font_size);
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}" viewBox="0 0 {px} {px}"><rect width="{px}" height="{px}" fill="#18181b"/><text x="10" y="{text_y}" font-family="Segoe UI, Arial, sans-serif" font-size="{font_size}" font-weight="bold" fill="{color}">{}</text></svg>"##,
        orcadeck_builtin::core::esc(text)
    );
    let rgba = orcadeck_builtin::raster::svg_to_rgba(&svg, px).unwrap_or_else(|_| vec![24; (px * px * 4) as usize]);
    let full = RgbaImage::from_raw(px, px, rgba).unwrap();
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            out.put_pixel(x, y, *full.get_pixel(x, y));
        }
    }
    out
}

fn jpeg_roundtrip(svg: &str) -> RgbaImage {
    let rgba = orcadeck_builtin::raster::svg_to_rgba(svg, 85).expect("raster svg");
    let img = RgbaImage::from_raw(85, 85, rgba).expect("raw image");
    let dyn_img = image::DynamicImage::ImageRgba8(img);
    let jpeg = render::key_jpeg(&dyn_img, 1);
    let decoded = image::load_from_memory(&jpeg).expect("decode jpeg");
    image::imageops::rotate90(&decoded.to_rgba8())
}

fn upscale_3x_nn(src: &RgbaImage) -> RgbaImage {
    let (sw, sh) = src.dimensions();
    let (dw, dh) = (sw * 3, sh * 3);
    let mut dst = RgbaImage::new(dw, dh);
    for dy in 0..dh {
        let sy = dy / 3;
        for dx in 0..dw {
            let sx = dx / 3;
            dst.put_pixel(dx, dy, *src.get_pixel(sx, sy));
        }
    }
    dst
}

struct Cell {
    col_label: String,
    svg: String,
}

struct Row {
    title: String,
    cells: Vec<Cell>,
}

fn make_agent(label: &str, color: &str, state: State, since_ms_ago: Option<i64>, workers: u32, now: i64) -> Agent {
    Agent {
        key: format!("k:{label}"),
        handle: format!("h:{label}"),
        worktree_id: "w1".into(),
        repo: label.into(),
        label: label.into(),
        color: color.into(),
        state,
        since: since_ms_ago.map(|ago| now - ago),
        workers,
    }
}

fn main() {
    let now = 1_000_000_000i64;
    let mut rows: Vec<Row> = Vec::new();

    // 1. Working at frames 0..7 (3 real colors)
    // Real color 1: Orca blue #2563eb
    let a1 = make_agent("orca", "#2563eb", State::Working, None, 0, now);
    rows.push(Row {
        title: "Working #2563eb (orca)".into(),
        cells: (0..8)
            .map(|f| Cell {
                col_label: format!("frame {f}"),
                svg: render_agent(Some(&a1), f, now, false),
            })
            .collect(),
    });

    // Real color 2: Terracotta / Anthropic #d97757
    let a2 = make_agent("anpa", "#d97757", State::Working, None, 0, now);
    rows.push(Row {
        title: "Working #d97757 (anpa)".into(),
        cells: (0..8)
            .map(|f| Cell {
                col_label: format!("frame {f}"),
                svg: render_agent(Some(&a2), f, now, false),
            })
            .collect(),
    });

    // Real color 3: Green #16a34a
    let a3 = make_agent("laya", "#16a34a", State::Working, None, 0, now);
    rows.push(Row {
        title: "Working #16a34a (laya)".into(),
        cells: (0..8)
            .map(|f| Cell {
                col_label: format!("frame {f}"),
                svg: render_agent(Some(&a3), f, now, false),
            })
            .collect(),
    });

    // 2. Question blink on/off (frames 0..3, blink=true) with since=12s and 75s ago
    let q_12s = make_agent("review", "#2563eb", State::Question, Some(12_000), 0, now);
    let q_12s_wk = make_agent("review", "#2563eb", State::Question, Some(12_000), 2, now);
    let mut q12_cells = Vec::new();
    for f in 0..4 {
        q12_cells.push(Cell {
            col_label: format!("f{f} blink=1 (w=0)"),
            svg: render_agent(Some(&q_12s), f, now, true),
        });
    }
    for f in 0..4 {
        q12_cells.push(Cell {
            col_label: format!("f{f} blink=1 (w=2)"),
            svg: render_agent(Some(&q_12s_wk), f, now, true),
        });
    }
    rows.push(Row {
        title: "Question 12s ago (blink)".into(),
        cells: q12_cells,
    });

    // 75s: 75_000 ms ago -> "1m"
    let q_75s = make_agent("review", "#8b5cf6", State::Question, Some(75_000), 0, now);
    let q_75s_wk = make_agent("review", "#8b5cf6", State::Question, Some(75_000), 1, now);
    let mut q75_cells = Vec::new();
    for f in 0..4 {
        q75_cells.push(Cell {
            col_label: format!("f{f} blink=1 (w=0)"),
            svg: render_agent(Some(&q_75s), f, now, true),
        });
    }
    for f in 0..4 {
        q75_cells.push(Cell {
            col_label: format!("f{f} blink=1 (w=1)"),
            svg: render_agent(Some(&q_75s_wk), f, now, true),
        });
    }
    rows.push(Row {
        title: "Question 75s ago (blink)".into(),
        cells: q75_cells,
    });

    // 3. Done blink on/off (frames 0..3) with since=12s and 75s
    let d_12s = make_agent("build", "#16a34a", State::Done, Some(12_000), 0, now);
    let d_75s_wk = make_agent("build", "#16a34a", State::Done, Some(75_000), 1, now);
    let mut done_cells = Vec::new();
    for f in 0..4 {
        done_cells.push(Cell {
            col_label: format!("f{f} blink=1 12s"),
            svg: render_agent(Some(&d_12s), f, now, true),
        });
    }
    for f in 0..4 {
        done_cells.push(Cell {
            col_label: format!("f{f} blink=1 75s (w=1)"),
            svg: render_agent(Some(&d_75s_wk), f, now, true),
        });
    }
    rows.push(Row {
        title: "Done blink on/off".into(),
        cells: done_cells,
    });

    // 4. Idle Question without blink, empty slot, and ring visibility tests
    let q_idle_12s = make_agent("query", "#2563eb", State::Question, Some(12_000), 0, now);
    let q_idle_75s = make_agent("query", "#2563eb", State::Question, Some(75_000), 0, now);
    let q_yellow = make_agent("alert", "#facc15", State::Question, Some(12_000), 0, now);
    let q_white = make_agent("bright", "#ffffff", State::Question, Some(12_000), 0, now);
    let q_dark = make_agent("slate", "#111827", State::Question, Some(12_000), 0, now);
    let idle_cells = vec![
        Cell {
            col_label: "idle Q blink=0 (12s)".into(),
            svg: render_agent(Some(&q_idle_12s), 0, now, false),
        },
        Cell {
            col_label: "idle Q blink=0 (75s)".into(),
            svg: render_agent(Some(&q_idle_75s), 0, now, false),
        },
        Cell {
            col_label: "empty slot (None)".into(),
            svg: render_agent(None, 0, now, false),
        },
        Cell {
            col_label: "blink on yellow #facc15".into(),
            svg: render_agent(Some(&q_yellow), 0, now, true),
        },
        Cell {
            col_label: "blink on white #ffffff".into(),
            svg: render_agent(Some(&q_white), 0, now, true),
        },
        Cell {
            col_label: "blink on dark #111827".into(),
            svg: render_agent(Some(&q_dark), 0, now, true),
        },
        Cell {
            col_label: "State::Idle".into(),
            svg: render_agent(Some(&make_agent("standby", "#6b7280", State::Idle, None, 0, now)), 0, now, false),
        },
        Cell {
            col_label: "State::Error".into(),
            svg: render_agent(Some(&make_agent("failed", "#dc2626", State::Error, None, 0, now)), 0, now, false),
        },
    ];
    rows.push(Row {
        title: "Idle, Empty, Ring Contrast".into(),
        cells: idle_cells,
    });

    // Layout configuration
    let num_cols = 8;
    let key_upscaled = 255u32; // 85 * 3
    let cell_gap = 14u32;
    let label_w = 260u32;
    let col_header_h = 32u32;
    let title_h = 44u32;

    let row_h = key_upscaled + col_header_h + cell_gap;
    let total_w = label_w + (num_cols as u32) * (key_upscaled + cell_gap) + cell_gap;
    let total_h = title_h + (rows.len() as u32) * row_h + cell_gap;

    let mut sheet = RgbaImage::from_pixel(total_w, total_h, Rgba([24, 24, 27, 255]));

    // Draw main title
    let title_img = render_label(
        "OrcaDeck Agent Key Render Test Sheet (85px -> 3x NN, JPEG round-trip)",
        total_w,
        title_h,
        18,
        "#f4f4f5",
    );
    let _ = sheet.copy_from(&title_img, 0, 0);

    for (r_idx, row) in rows.iter().enumerate() {
        let row_y = title_h + (r_idx as u32) * row_h;

        // Render row title
        let row_title_img = render_label(&row.title, label_w, key_upscaled, 15, "#e4e4e7");
        let _ = sheet.copy_from(&row_title_img, 0, row_y + col_header_h);

        for (c_idx, cell) in row.cells.iter().enumerate() {
            let col_x = label_w + (c_idx as u32) * (key_upscaled + cell_gap);

            // Column header for cell
            let header_img = render_label(&cell.col_label, key_upscaled, col_header_h, 12, "#a1a1aa");
            let _ = sheet.copy_from(&header_img, col_x, row_y);

            // Render key: SVG -> 85x85 RGBA -> JPEG roundtrip -> rotate90 -> 3x NN
            let key_85 = jpeg_roundtrip(&cell.svg);
            let key_255 = upscale_3x_nn(&key_85);
            let _ = sheet.copy_from(&key_255, col_x, row_y + col_header_h);
        }
    }

    let out_dir = std::env::temp_dir().join("orcadeck-frames");
    fs::create_dir_all(&out_dir).expect("create out_dir");
    let sheet_path = out_dir.join("sheet.png");
    sheet.save_with_format(&sheet_path, image::ImageFormat::Png).expect("save sheet.png");
    println!("Wrote sheet to {}", sheet_path.display());

    // Print consecutive frame pixel differences for Working rows
    println!("\nConsecutive frame pixel difference verification (at 85x85 key size):");
    for row in rows.iter().take(3) {
        println!("{}:", row.title);
        let keys_85: Vec<_> = row.cells.iter().map(|c| jpeg_roundtrip(&c.svg)).collect();
        for f in 0..keys_85.len() {
            let next_f = (f + 1) % keys_85.len();
            let mut diff_px = 0;
            for (p1, p2) in keys_85[f].pixels().zip(keys_85[next_f].pixels()) {
                if p1 != p2 {
                    diff_px += 1;
                }
            }
            println!("  frame {f} -> {next_f}: {diff_px} / {} pixels differ ({:.1}%)", 85 * 85, (diff_px as f64 / (85.0 * 85.0)) * 100.0);
        }
    }

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--before") {
        let before_path = out_dir.join("sheet-before.png");
        fs::copy(&sheet_path, &before_path).expect("copy sheet-before.png");
        println!("Copied to {}", before_path.display());
    }
    if args.iter().any(|a| a == "--after") {
        let after_path = out_dir.join("sheet-after.png");
        fs::copy(&sheet_path, &after_path).expect("copy sheet-after.png");
        println!("Copied to {}", after_path.display());
    }
}
