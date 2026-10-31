//! The optional signature image of the closing block (ADR-090): chosen
//! through the backend's open dialog, decoded and re-encoded here so no
//! metadata survives, its paper made transparent and its margins trimmed,
//! and stored beside the CV. Exports draw it only when the resolved CV shows
//! the signature.

use std::sync::Arc;

use base64::Engine as _;
use cv_api::{ApiError, ErrorCode, SignatureInfo};
use cv_core::{ClosingPart, Id, ResolvedCv};
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{ImageEncoder, RgbaImage};

use crate::photo::{PHOTO_EXTENSIONS, decode, read_image};
use crate::{App, Result, store_error};

/// The stored signature fits in this box, in pixels; never scaled up.
const MAX_WIDTH: u32 = 900;
const MAX_HEIGHT: u32 = 300;
/// Pixels at least this light count as paper and become transparent.
const PAPER: u8 = 215;
/// Transparent margin kept around the ink, in pixels.
const MARGIN: u32 = 4;

fn no_ink() -> ApiError {
    ApiError::new(
        ErrorCode::Validation,
        "No signature was found in that image. Choose a scan or photo of dark ink on light paper.",
    )
}

/// Turns a scan or photo of a signature into a PNG: light paper becomes
/// transparent, darker pixels keep their colour with an opacity that grows
/// with their darkness, the margins are trimmed and the result fits in
/// [`MAX_WIDTH`] × [`MAX_HEIGHT`]. Only the pixels are kept.
pub fn clean_signature(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut image = decode(bytes)?.to_rgba8();
    let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in image.enumerate_pixels_mut() {
        // Flatten any transparency onto white first.
        let a = u32::from(p[3]);
        let flat = |c: u8| (u32::from(c) * a + 255 * (255 - a)) / 255;
        let (r, g, b) = (flat(p[0]), flat(p[1]), flat(p[2]));
        let luma = (r * 299 + g * 587 + b * 114) / 1000;
        let alpha = if luma >= u32::from(PAPER) {
            0
        } else {
            (u32::from(PAPER) - luma) * 255 / u32::from(PAPER)
        };
        let to_u8 = |v: u32| u8::try_from(v.min(255)).unwrap_or(u8::MAX);
        *p = image::Rgba([to_u8(r), to_u8(g), to_u8(b), to_u8(alpha * 2)]);
        if p[3] > 0 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    if left > right || top > bottom {
        return Err(no_ink());
    }
    let x = left.saturating_sub(MARGIN);
    let y = top.saturating_sub(MARGIN);
    let w = (right + MARGIN + 1).min(image.width()) - x;
    let h = (bottom + MARGIN + 1).min(image.height()) - y;
    let ink = image::imageops::crop_imm(&image, x, y, w, h).to_image();
    png(&fit_within(ink))
}

fn fit_within(image: RgbaImage) -> RgbaImage {
    let (w, h) = image.dimensions();
    if w <= MAX_WIDTH && h <= MAX_HEIGHT {
        return image;
    }
    let scale = f64::min(
        f64::from(MAX_WIDTH) / f64::from(w),
        f64::from(MAX_HEIGHT) / f64::from(h),
    );
    let side = |n: u32| (f64::from(n) * scale).round().clamp(1.0, f64::from(n)) as u32;
    image::imageops::resize(&image, side(w), side(h), FilterType::Lanczos3)
}

/// A PNG of `image`. Encoding writes no metadata.
fn png(image: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|_| ApiError::new(ErrorCode::Internal, "The signature could not be encoded."))?;
    Ok(out)
}

fn info(png: &[u8]) -> SignatureInfo {
    SignatureInfo {
        data_url: format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        ),
    }
}

impl App {
    /// Asks for a JPEG or PNG of a signature and saves it as the CV's
    /// signature, replacing any earlier one.
    pub fn signature_choose(&self, cv_id: &Id) -> Result<SignatureInfo> {
        let path = self
            .dialog
            .choose_open_path(PHOTO_EXTENSIONS)
            .ok_or_else(|| ApiError::new(ErrorCode::Cancelled, "No image chosen."))?;
        let png = clean_signature(&read_image(
            &path,
            "That image is larger than 5 MB. Choose a smaller one.",
        )?)?;
        self.store.signature_put(cv_id, &png).map_err(store_error)?;
        Ok(info(&png))
    }

    /// The CV's signature, if it has one.
    pub fn signature_get(&self, cv_id: &Id) -> Result<Option<SignatureInfo>> {
        Ok(self
            .store
            .signature_get(cv_id)
            .map_err(store_error)?
            .map(|png| info(&png)))
    }

    /// Removes the CV's signature.
    pub fn signature_remove(&self, cv_id: &Id) -> Result<()> {
        self.store.signature_delete(cv_id).map_err(store_error)
    }

    /// The signature to draw for `cv`: its image when it shows one.
    pub(crate) fn signature_for(&self, cv: &ResolvedCv) -> Result<Option<Arc<[u8]>>> {
        if !cv.closing.show.contains(&ClosingPart::Signature) {
            return Ok(None);
        }
        Ok(self
            .store
            .signature_get(&cv.cv_id)
            .map_err(store_error)?
            .map(Into::into))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::jpeg::JpegEncoder;
    use image::{Rgb, RgbImage};

    /// A white page with a dark stroke at x 40..60, y 20..25.
    fn scan() -> Vec<u8> {
        let mut page = RgbImage::from_pixel(200, 100, Rgb([250, 250, 245]));
        for x in 40..60 {
            for y in 20..25 {
                page.put_pixel(x, y, Rgb([20, 20, 80]));
            }
        }
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, 95)
            .encode_image(&page)
            .expect("jpeg");
        out
    }

    fn decoded(png: &[u8]) -> RgbaImage {
        image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .expect("png")
            .to_rgba8()
    }

    #[test]
    fn paper_turns_transparent_and_margins_are_trimmed() {
        let png = clean_signature(&scan()).expect("clean");
        let out = decoded(&png);
        // About 20 × 5 of ink plus the margin on each side (JPEG blurs the
        // edges by a pixel or two).
        assert!((28..=34).contains(&out.width()), "{}", out.width());
        assert!((13..=19).contains(&out.height()), "{}", out.height());
        assert_eq!(out.get_pixel(0, 0)[3], 0, "the corner is paper");
        let centre = out.get_pixel(out.width() / 2, out.height() / 2);
        assert!(centre[3] > 200, "the ink stays opaque: {centre:?}");
    }

    #[test]
    fn large_signatures_are_scaled_into_the_box() {
        let mut page = RgbImage::from_pixel(3000, 400, Rgb([255, 255, 255]));
        for x in 10..2990 {
            page.put_pixel(x, 200, Rgb([0, 0, 0]));
        }
        let mut bytes = Vec::new();
        PngEncoder::new(&mut bytes)
            .write_image(page.as_raw(), 3000, 400, image::ExtendedColorType::Rgb8)
            .expect("png");
        let out = decoded(&clean_signature(&bytes).expect("clean"));
        assert!(out.width() <= MAX_WIDTH && out.height() <= MAX_HEIGHT);
        assert_eq!(out.width(), MAX_WIDTH);
    }

    #[test]
    fn a_blank_page_is_refused() {
        let page = RgbImage::from_pixel(50, 50, Rgb([255, 255, 255]));
        let mut bytes = Vec::new();
        PngEncoder::new(&mut bytes)
            .write_image(page.as_raw(), 50, 50, image::ExtendedColorType::Rgb8)
            .expect("png");
        let err = clean_signature(&bytes).expect_err("blank");
        assert_eq!(err.code, ErrorCode::Validation);
        assert_eq!(
            clean_signature(b"not an image").expect_err("junk").code,
            ErrorCode::UnsupportedFormat
        );
    }
}
