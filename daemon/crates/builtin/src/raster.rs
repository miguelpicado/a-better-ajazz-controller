//! SVG -> RGBA at key size (85x85 keys, 80x80 side cells) with resvg.
use resvg::{tiny_skia, usvg};
use std::sync::{Arc, OnceLock};

/// System fonts, loaded once (scanning C:\Windows\Fonts takes tens of ms).
fn fonts() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

/// Renders `svg` scaled to `px` x `px` and returns straight (non-premultiplied) RGBA, row-major.
pub fn svg_to_rgba(svg: &str, px: u32) -> Result<Vec<u8>, String> {
    let opt = usvg::Options {
        fontdb: fonts(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    let mut pixmap = tiny_skia::Pixmap::new(px, px).ok_or("bad size")?;
    let size = tree.size();
    let t = tiny_skia::Transform::from_scale(px as f32 / size.width(), px as f32 / size.height());
    resvg::render(&tree, t, &mut pixmap.as_mut());
    Ok(pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect())
}
