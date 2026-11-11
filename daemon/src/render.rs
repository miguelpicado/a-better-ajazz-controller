//! Key image pipeline: any image -> resize -> rotate 270 -> JPEG <= 9000 bytes (quality 75 down by 4).
use crate::proto::{image_px, MAX_JPEG};
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

pub fn key_jpeg(img: &DynamicImage, key: u8) -> Vec<u8> {
    let px = image_px(key);
    let scaled;
    let img = if (img.width(), img.height()) == (px, px) {
        img
    } else {
        scaled = scale_key(img, px);
        &scaled
    };
    let rgb = imageops::rotate270(&img.to_rgb8());
    // q75, not 100: at ~7 KB per key a batch of animated keys kept the device busy ~40 % of the time and it
    // missed presses; flat UI graphics look fine at 75 and are ~3x smaller.
    let mut q: i32 = 75;
    loop {
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q.max(1) as u8)
            .write_image(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
            .expect("jpeg encode");
        if out.len() <= MAX_JPEG || q <= 1 {
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
        let img = noisy(px);
        let rotated = imageops::rotate270(&img.to_rgb8());
        let mut expected = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut expected, 75)
            .write_image(rotated.as_raw(), px, px, image::ExtendedColorType::Rgb8)
            .unwrap();
        assert_eq!(key_jpeg(&img, 1), expected, "frame is encoded as-is, with no second resize");
    }
}
