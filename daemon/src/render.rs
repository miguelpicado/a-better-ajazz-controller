//! Key image pipeline: any image -> resize -> rotate 270 -> JPEG <= 9000 bytes (quality 100 down by 4).
use crate::proto::{image_px, MAX_JPEG};
use image::{imageops, DynamicImage, ImageEncoder};

pub fn key_jpeg(img: &DynamicImage, key: u8) -> Vec<u8> {
    let px = image_px(key);
    let rgb = imageops::rotate270(&img.resize_exact(px, px, imageops::FilterType::Lanczos3).to_rgb8());
    let mut q: i32 = 100;
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
}
