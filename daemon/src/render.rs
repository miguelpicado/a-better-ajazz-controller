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
        std::env::var("ORCADECK_JPEG_Q")
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
    // at q75 a 4-key animation kept the device ~40 % busy and presses were missed. `ORCADECK_JPEG_Q` tunes it.
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
