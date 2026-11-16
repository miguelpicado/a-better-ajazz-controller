//! Key image pipeline: any image -> resize -> rotate 270 -> JPEG <= 9000 bytes (quality 85 down by 4).
#[allow(unused_imports)]
use crate::proto::{image_px, DeviceSpec, DEFAULT_DEVICE, MAX_JPEG};
use image::{imageops, DynamicImage, ImageEncoder};

fn scale_key(img: &DynamicImage, px: u32) -> DynamicImage {
    img.resize_exact(px, px, imageops::FilterType::Lanczos3)
}

/// Scales an image to key size once, at ingest, with the filter `key_jpeg` used to apply per paint.
pub fn fit_px(img: DynamicImage, px: u32) -> DynamicImage {
    if (img.width(), img.height()) == (px, px) {
        img
    } else {
        scale_key(&img, px)
    }
}

fn start_quality() -> i32 {
    static Q: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
    *Q.get_or_init(|| {
        std::env::var("ABAC_JPEG_Q")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .map_or(85, |q: i32| q.clamp(1, 100))
    })
}

/// Editor mirror of a key: the pipeline image as-is (upright, full size), no device size limit.
pub fn mirror_jpeg(img: &DynamicImage) -> Vec<u8> {
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88)
        .write_image(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
        .expect("jpeg encode");
    out
}

pub fn key_jpeg(img: &DynamicImage, key: u8) -> Vec<u8> {
    key_jpeg_spec(DEFAULT_DEVICE, img, key)
}

pub fn key_jpeg_spec(spec: &DeviceSpec, img: &DynamicImage, key: u8) -> Vec<u8> {
    let px = spec.image_px(key);
    let scaled;
    let img = if (img.width(), img.height()) == (px, px) {
        img
    } else {
        scaled = scale_key(img, px);
        &scaled
    };
    let rgb = imageops::rotate270(&img.to_rgb8());
    // Start quality (default 85) steps down until the frame fits `max_jpeg`. Lower = smaller = less USB time:
    // at q75 a 4-key animation kept the device ~40 % busy and presses were missed. `ABAC_JPEG_Q` tunes it.
    let mut q: i32 = start_quality();
    loop {
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q.max(1) as u8)
            .write_image(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .expect("jpeg encode");
        if out.len() <= spec.max_jpeg || q <= 1 {
            return out;
        }
        q -= 4;
    }
}

fn encode_rgb(rgb: &image::RgbImage, max: usize) -> Vec<u8> {
    let mut q = start_quality();
    loop {
        let mut out = Vec::new();
        // image's encoder writes baseline, 4:4:4 (no subsampling), which the firmware's libjpeg heap needs
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q.max(1) as u8)
            .write_image(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
            .expect("jpeg encode");
        if out.len() <= max || q <= 1 {
            return out;
        }
        q -= 8;
    }
}

/// DRA key tile: `img` scaled to `tile` px square, rotated like `key_jpeg` into panel orientation.
pub fn dra_tile_jpeg(img: &DynamicImage, tile: u16) -> Vec<u8> {
    let px = tile as u32;
    let rgb = imageops::rotate270(&fit_px(img.clone(), px).to_rgb8());
    encode_rgb(&rgb, crate::proto::DRA_MAX_JPEG)
}

/// DRA key rectangle of `w` x `h` PANEL px (calibration): `img` stretched to it, rotated into panel orientation.
/// Square sizes take the same path as [`dra_tile_jpeg`].
pub fn dra_rect_jpeg(img: &DynamicImage, w: u16, h: u16) -> Vec<u8> {
    if w == h {
        return dra_tile_jpeg(img, w);
    }
    // device view is the panel rotated 90 deg: device width = panel h, device height = panel w
    let dev = img.resize_exact(h as u32, w as u32, imageops::FilterType::Triangle).to_rgb8();
    encode_rgb(&imageops::rotate270(&dev), crate::proto::DRA_MAX_JPEG)
}

/// Calibration test pattern in DEVICE orientation, `dw` x `dh` px: 1 px white border, white diagonals, the key
/// number in the middle and a red square in the top-left corner (to tell the orientation).
pub fn calib_pattern(key: u8, dw: u32, dh: u32) -> DynamicImage {
    const DIGITS: [[u8; 5]; 10] = [
        [7, 5, 5, 5, 7], [2, 6, 2, 2, 7], [7, 1, 7, 4, 7], [7, 1, 7, 1, 7], [5, 5, 7, 1, 1],
        [7, 4, 7, 1, 7], [7, 4, 7, 5, 7], [7, 1, 1, 1, 1], [7, 5, 7, 5, 7], [7, 5, 7, 1, 7],
    ];
    let (dw, dh) = (dw.max(1), dh.max(1));
    let white = image::Rgb([255u8, 255, 255]);
    let mut im = image::RgbImage::new(dw, dh);
    for x in 0..dw {
        for y in 0..dh {
            let diag = (x * dh / dw).abs_diff(y) == 0 || ((dw - 1 - x) * dh / dw).abs_diff(y) == 0;
            if diag || x == 0 || y == 0 || x == dw - 1 || y == dh - 1 {
                im.put_pixel(x, y, white);
            }
        }
    }
    let text = key.to_string();
    let units = 4 * text.len() as u32 - 1; // 3 wide + 1 gap per digit
    let sc = (dw * 6 / 10 / units).min(dh * 5 / 10 / 5).max(1);
    let (x0, y0) = ((dw.saturating_sub(units * sc)) / 2, (dh.saturating_sub(5 * sc)) / 2);
    for (i, ch) in text.bytes().enumerate() {
        for (r, bits) in DIGITS[(ch - b'0') as usize].iter().enumerate() {
            for c in 0..3u32 {
                if bits >> (2 - c) & 1 == 1 {
                    for (px, py) in (0..sc).flat_map(|a| (0..sc).map(move |b| (a, b))) {
                        let (x, y) = (x0 + (i as u32 * 4 + c) * sc + px, y0 + r as u32 * sc + py);
                        if x < dw && y < dh {
                            im.put_pixel(x, y, white);
                        }
                    }
                }
            }
        }
    }
    let m = (dw.min(dh) / 8).clamp(2, 8);
    for (x, y) in (1..=m).flat_map(|x| (1..=m).map(move |y| (x, y))) {
        if x < dw && y < dh {
            im.put_pixel(x, y, image::Rgb([255, 0, 0]));
        }
    }
    DynamicImage::ImageRgb8(im)
}

/// Side panel (keys 16..18) as ONE continuous image. Cells sit at the BAT pitch (80 px every 149 along
/// the device vertical); the gaps are filled by blending the facing cell edges. Returns the DRA pieces
/// (panel rect, jpeg), each at most `DRA_MAX_W` wide. `cells[i]` None = black. `side_px` = device-horizontal size.
pub fn dra_side_jpegs(
    cells: [Option<&DynamicImage>; 3],
    side_px: u16,
    off: (i16, i16),
) -> Vec<(crate::proto::Rect, Vec<u8>)> {
    use crate::proto::{centred_rect, DRA_MAX_W};
    const PITCH: u32 = 149;
    const CELL: u32 = 80;
    let (w, h) = (side_px as u32, 2 * PITCH + CELL); // device: w across, h down (= panel x)
    let mut dev = image::RgbImage::new(w, h);
    for (i, c) in cells.iter().enumerate() {
        if let Some(img) = c {
            let t = img.resize_exact(w, CELL, imageops::FilterType::Lanczos3).to_rgb8();
            imageops::replace(&mut dev, &t, 0, (i as u32 * PITCH) as i64);
        }
    }
    for i in 0..2u32 {
        let (top, bot) = (i * PITCH + CELL - 1, (i + 1) * PITCH);
        for y in top + 1..bot {
            let t = (y - top) as f32 / (bot - top) as f32;
            for x in 0..w {
                let (a, b) = (*dev.get_pixel(x, top), *dev.get_pixel(x, bot));
                let m = |k: usize| (a[k] as f32 * (1.0 - t) + b[k] as f32 * t) as u8;
                dev.put_pixel(x, y, image::Rgb([m(0), m(1), m(2)]));
            }
        }
    }
    let panel = DynamicImage::ImageRgb8(imageops::rotate270(&dev)); // (h wide) x (w tall)
    // BAT side cells: panel x 49.., y 12, 80 px; centre the strip on that row.
    let base = centred_rect(49 + panel.width() as i32 / 2, 12 + CELL as i32 / 2, panel.width() as u16, side_px, off);
    let mut out = Vec::new();
    let mut x0 = 0;
    while x0 < panel.width() {
        let pw = (panel.width() - x0).min(DRA_MAX_W as u32);
        let piece = panel.crop_imm(x0, 0, pw, panel.height()).to_rgb8();
        let r = crate::proto::Rect { x: base.x + x0 as u16, y: base.y, w: pw as u16, h: base.h };
        out.push((r, encode_rgb(&piece, crate::proto::DRA_MAX_JPEG)));
        x0 += pw;
    }
    out
}

/// Rasterises a `panel_strip` SVG (`dw` x `dh`, device orientation) at 2x, like the key pipeline's work size.
pub fn strip_image(svg: &str, dw: u32, dh: u32) -> Option<DynamicImage> {
    let (w, h) = (dw.clamp(1, 400) * 2, dh.clamp(1, 960) * 2);
    let rgba = abac_builtin::raster::svg_to_rgba_wh(svg, w, h).ok()?;
    image::RgbaImage::from_raw(w, h, rgba).map(DynamicImage::ImageRgba8)
}

/// BAT fallback: the three 80 px cells (at work size, 160 px) cut out of the strip, rows at the BAT pitch,
/// centred horizontally. Missing area (a small canvas) stays black.
pub fn strip_slices(strip2x: &DynamicImage) -> [DynamicImage; 3] {
    const W: u32 = 160;
    let rgb = strip2x.to_rgb8();
    let x0 = rgb.width().saturating_sub(W) / 2;
    abac_builtin::cell_rows().map(|(y0, _)| {
        let mut cell = image::RgbImage::new(W, W);
        let (y0, w) = (y0 * 2, W.min(rgb.width()));
        if y0 < rgb.height() {
            let h = W.min(rgb.height() - y0);
            imageops::replace(&mut cell, &imageops::crop_imm(&rgb, x0, y0, w, h).to_image(), 0, 0);
        }
        DynamicImage::ImageRgb8(cell)
    })
}

/// DRA draw of the whole strip: `strip2x` (device orientation, 2x) fitted to `h` px across and its own length
/// along, rotated like key images, centred on the BAT side cells (panel x 238, y 52) + `off`, clamped to the
/// panel, split into equal pieces of at most `DRA_MAX_W` px (a 378 px strip is one piece).
pub fn dra_strip_jpegs(strip2x: &DynamicImage, h: u16, off: (i16, i16)) -> Vec<(crate::proto::Rect, Vec<u8>)> {
    use crate::proto::{Rect, DRA_MAX_JPEG, DRA_MAX_W, PANEL_H, PANEL_W};
    let w = (strip2x.height() / 2).clamp(1, PANEL_W as u32);
    let h = (h as u32).clamp(1, PANEL_H as u32);
    let panel = DynamicImage::ImageRgb8(imageops::rotate270(&strip2x.resize_exact(h, w, imageops::FilterType::Lanczos3).to_rgb8()));
    let x = (238 - w as i32 / 2 + off.0 as i32).clamp(0, (PANEL_W as u32 - w) as i32) as u32;
    let y = (52 - h as i32 / 2 + off.1 as i32).clamp(0, (PANEL_H as u32 - h) as i32) as u32;
    let pw = w.div_ceil(w.div_ceil(DRA_MAX_W as u32));
    (0..w)
        .step_by(pw as usize)
        .map(|x0| {
            let cw = pw.min(w - x0);
            let piece = panel.crop_imm(x0, 0, cw, h).to_rgb8();
            (Rect { x: (x + x0) as u16, y: y as u16, w: cw as u16, h: h as u16 }, encode_rgb(&piece, DRA_MAX_JPEG))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (components, [sampling bytes]) of the first baseline SOF0 marker.
    fn sof0(j: &[u8]) -> (u16, u16, Vec<u8>) {
        let i = j.windows(2).position(|w| w == [0xFF, 0xC0]).expect("SOF0 (baseline)");
        let (h, w, n) = (u16::from_be_bytes([j[i + 5], j[i + 6]]), u16::from_be_bytes([j[i + 7], j[i + 8]]), j[i + 9] as usize);
        (w, h, (0..n).map(|c| j[i + 11 + 3 * c]).collect())
    }

    #[test]
    fn dra_tile_is_baseline_444_panel_oriented() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(200, 100, |x, y| image::Rgb([x as u8, y as u8, 7])));
        let j = dra_tile_jpeg(&img, 98);
        assert_eq!(sof0(&j), (98, 98, vec![0x11, 0x11, 0x11]));
        assert!(j.len() < crate::proto::DRA_MAX_JPEG);
    }

    #[test]
    fn dra_rect_non_square_is_panel_sized_baseline_444() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(80, 80, image::Rgb([9, 9, 9])));
        assert_eq!(sof0(&dra_rect_jpeg(&img, 60, 100)), (60, 100, vec![0x11, 0x11, 0x11]));
        assert_eq!(sof0(&dra_rect_jpeg(&img, 98, 98)), (98, 98, vec![0x11, 0x11, 0x11]));
    }

    #[test]
    fn calib_pattern_has_border_diagonals_number_and_marker() {
        let p = calib_pattern(12, 98, 60).to_rgb8();
        assert_eq!((p.width(), p.height()), (98, 60));
        let w = image::Rgb([255, 255, 255]);
        assert_eq!((*p.get_pixel(0, 30), *p.get_pixel(97, 30), *p.get_pixel(40, 0), *p.get_pixel(40, 59)), (w, w, w, w));
        assert_eq!(*p.get_pixel(2, 2), image::Rgb([255, 0, 0]));
        assert!(p.pixels().filter(|px| **px == w).count() > 300);
        // tiny sizes and 3-digit-wide cells must not panic
        for (w, h) in [(1, 1), (8, 8), (384, 8), (8, 384)] {
            calib_pattern(18, w, h);
        }
    }

    #[test]
    fn dra_side_is_one_piece_under_the_width_limit() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(80, 80, image::Rgb([200, 30, 30])));
        let v = dra_side_jpegs([Some(&img), None, Some(&img)], 80, (0, 0));
        assert_eq!(v.len(), 1);
        let (r, j) = &v[0];
        assert_eq!((r.x, r.y, r.w, r.h), (49, 12, 378, 80));
        assert_eq!(sof0(j), (378, 80, vec![0x11, 0x11, 0x11]));
        // a wider-than-limit strip would split; 3 cells at this pitch never exceed it
        assert!(r.w <= crate::proto::DRA_MAX_W);
    }

    #[test]
    fn dra_side_larger_px_grows_vertically_in_panel() {
        let v = dra_side_jpegs([None, None, None], 96, (0, 0));
        assert_eq!((v[0].0.y, v[0].0.h), (4, 96));
    }
    #[test]
    fn fits_limit_and_decodes() {
        let noisy = DynamicImage::ImageRgb8(image::RgbImage::from_fn(200, 200, |x, y| {
            image::Rgb([(x * 7 + y * 13) as u8, (x ^ y) as u8, (x * y) as u8])
        }));
        for key in [1u8, 16] {
            let j = key_jpeg(&noisy, key);
            assert!(j.len() <= MAX_JPEG, "{} bytes", j.len());
            let d = image::load_from_memory(&j).unwrap();
            assert_eq!(d.width(), image_px(key));
        }
    }

    fn noisy(px: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(image::RgbImage::from_fn(px, px, |x, y| {
            image::Rgb([(x * 7 + y * 13) as u8, (x ^ y) as u8, (x * y) as u8])
        }))
    }

    #[test]
    fn prescaled_input_encodes_the_same_bytes_as_the_original() {
        let original = noisy(200);
        for key in [1u8, 16] {
            let before = key_jpeg(&original, key);
            let after = key_jpeg(&fit_px(original.clone(), image_px(key)), key);
            assert_eq!(after, before, "key {key}");
        }
    }

    #[test]
    fn already_key_sized_images_skip_the_resize() {
        let px = image_px(1);
        // Smooth, so it fits `max_jpeg` at the start quality and the quality loop does not kick in.
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(px, px, |x, y| {
            image::Rgb([(x * 2) as u8, (y * 2) as u8, 128])
        }));
        let rotated = imageops::rotate270(&img.to_rgb8());
        let mut expected = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut expected, start_quality() as u8)
            .write_image(rotated.as_raw(), px, px, image::ExtendedColorType::Rgb8)
            .unwrap();
        assert_eq!(
            key_jpeg(&img, 1),
            expected,
            "frame is encoded as-is, with no second resize"
        );
    }

    // ---- panel_strip ----------------------------------------------------------------------

    use abac_builtin::core::LocalTime;
    use abac_builtin::strip::{render_strip, strip_dims};
    use abac_builtin::sys::{Gpu, Sample};
    use abac_builtin::StripCfg;

    fn state(cpu: f64, ram: f64, gpu: Option<f64>) -> Sample {
        Sample {
            cpu: Some(cpu),
            ram: Some((ram, (ram * 3.2e8) as u64, 32_000_000_000)),
            gpu: gpu.map(|u| Gpu { temp: 40.0 + u * 0.4, util: u, mem_used: (u * 8e7) as u64 + 900_000_000, mem_total: 8_000_000_000, clock_mhz: 1800.0 }),
            ..Default::default()
        }
    }

    const T: LocalTime = LocalTime { hour: 14, minute: 37, day: 13, month: 11, weekday: 5 };

    fn strip(t: LocalTime, s: &Sample, w: u16, h: u16, bat: bool) -> DynamicImage {
        let svg = render_strip(t, s, StripCfg { w, h, bat });
        let (dw, dh) = strip_dims(&svg).expect("strip marker");
        assert_eq!((dw, dh), (h as u32, w as u32));
        strip_image(&svg, dw, dh).expect("raster")
    }

    #[test]
    fn strip_slices_are_cell_sized_and_hold_the_cell_rows() {
        let s = state(50.0, 50.0, Some(50.0));
        let img = strip(T, &s, 378, 80, true);
        assert_eq!((img.width(), img.height()), (160, 756));
        let cells = strip_slices(&img);
        for c in &cells {
            assert_eq!((c.width(), c.height()), (160, 160));
        }
        // BAT layout puts content in every cell: none of the slices is blank
        for (i, c) in cells.iter().enumerate() {
            assert!(c.to_rgb8().pixels().any(|p| p.0.iter().any(|&v| v > 60)), "cell {i} blank");
        }
        // a small canvas must not panic and stays black where it has no rows
        let small = DynamicImage::ImageRgb8(image::RgbImage::new(100, 200));
        assert_eq!(strip_slices(&small)[2].to_rgb8().pixels().filter(|p| p.0 != [0, 0, 0]).count(), 0);
    }

    #[test]
    fn dra_strip_geometry_and_jpeg_limits() {
        let s = state(30.0, 60.0, Some(10.0));
        for (w, h, want) in [
            (378u16, 80u16, vec![(49u16, 12u16, 378u16, 80u16)]),
            (378, 95, vec![(49, 5, 378, 95)]),
            (480, 95, vec![(0, 5, 240, 95), (240, 5, 240, 95)]),
        ] {
            let v = dra_strip_jpegs(&strip(T, &s, w, h, false), h, (0, 0));
            assert_eq!(v.iter().map(|(r, _)| (r.x, r.y, r.w, r.h)).collect::<Vec<_>>(), want, "{w}x{h}");
            for (r, j) in &v {
                assert!(r.w <= crate::proto::DRA_MAX_W && j.len() < crate::proto::DRA_MAX_JPEG, "{} bytes", j.len());
                assert_eq!(sof0(j), (r.w, r.h, vec![0x11, 0x11, 0x11]), "baseline 4:4:4");
            }
        }
        // calibration offset moves it, the panel clamps it
        let img = strip(T, &s, 378, 80, false);
        assert_eq!(dra_strip_jpegs(&img, 80, (5, -3))[0].0.x, 54);
        assert_eq!(dra_strip_jpegs(&img, 80, (-500, -500))[0].0.x, 0);
        assert_eq!(dra_strip_jpegs(&img, 80, (900, 900))[0].0.y, 854 - 80);
        // a stale canvas (calibration changed `h` before the next render) is resampled, not rejected
        assert_eq!(dra_strip_jpegs(&img, 95, (0, 0))[0].0.h, 95);
    }

    /// Strip as the device shows it: DRA pieces decoded and rotated back to device orientation.
    fn device_view(pieces: &[(crate::proto::Rect, Vec<u8>)]) -> image::RgbImage {
        let w: u32 = pieces.iter().map(|(r, _)| r.w as u32).sum();
        let mut panel = image::RgbImage::new(w, pieces[0].0.h as u32);
        let mut x = 0;
        for (r, j) in pieces {
            imageops::replace(&mut panel, &image::load_from_memory(j).unwrap().to_rgb8(), x, 0);
            x += r.w as i64;
        }
        imageops::rotate90(&panel)
    }

    fn up(im: &image::RgbImage, n: u32) -> image::RgbImage {
        imageops::resize(im, im.width() * n, im.height() * n, imageops::FilterType::Nearest)
    }

    fn sheet(cols: &[image::RgbImage]) -> image::RgbImage {
        let (w, h) = (cols.iter().map(|c| c.width() + 16).sum::<u32>() + 16, cols.iter().map(|c| c.height()).max().unwrap() + 32);
        let mut out = image::RgbImage::from_pixel(w, h, image::Rgb([48, 48, 52]));
        let mut x = 16;
        for c in cols {
            imageops::replace(&mut out, c, x as i64, 16);
            x += c.width() + 16;
        }
        out
    }

    /// Offline previews. `STRIP_PREVIEW_DIR=<dir> cargo test -p abac strip_previews` writes them; otherwise a no-op.
    #[test]
    fn strip_previews() {
        let Some(dir) = std::env::var_os("STRIP_PREVIEW_DIR").map(std::path::PathBuf::from) else { return };
        std::fs::create_dir_all(&dir).unwrap();
        let wrap = LocalTime { hour: 0, minute: 0, day: 1, month: 1, weekday: 0 };
        let states: [(&str, LocalTime, Sample); 4] = [
            ("idle", T, state(8.0, 41.0, Some(3.0))),
            ("high-cpu", T, state(96.0, 88.0, Some(74.0))),
            ("gpu-nd", T, state(35.0, 55.0, None)),
            ("time-wrap", wrap, state(21.0, 47.0, Some(12.0))),
        ];
        let mut cols = Vec::new();
        let mut bat_cols = Vec::new();
        for (name, t, s) in &states {
            let v = device_view(&dra_strip_jpegs(&strip(*t, s, 378, 80, false), 80, (0, 0)));
            assert_eq!((v.width(), v.height()), (80, 378));
            up(&v, 3).save(dir.join(format!("strip-378x80-{name}.png"))).unwrap();
            cols.push(up(&v, 2));
            // BAT: three cell images (what the device gets through key_jpeg) at the BAT pitch
            let mut bat = image::RgbImage::new(80, 378);
            for (i, c) in strip_slices(&strip(*t, s, 378, 80, true)).iter().enumerate() {
                imageops::replace(&mut bat, &c.resize_exact(80, 80, imageops::FilterType::Lanczos3).to_rgb8(), 0, 149 * i as i64);
            }
            bat_cols.push(up(&bat, 2));
        }
        let idle = &states[0];
        for (w, h) in [(378u16, 95u16), (480, 95)] {
            let v = device_view(&dra_strip_jpegs(&strip(idle.1, &idle.2, w, h, false), h, (0, 0)));
            up(&v, 3).save(dir.join(format!("strip-{w}x{h}-idle.png"))).unwrap();
            cols.push(up(&v, 2));
        }
        sheet(&cols).save(dir.join("side-strip-preview.png")).unwrap();
        sheet(&bat_cols).save(dir.join("side-strip-bat-preview.png")).unwrap();
    }
}
