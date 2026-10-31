//! The optional profile photo (ADR-083): chosen through the backend's open
//! dialog, decoded and re-encoded here so no metadata (EXIF, GPS) survives,
//! cropped to the rectangle the user framed, and stored beside the CV.
//! Exports draw it only when the resolved CV shows it.

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine as _;
use cv_api::{ApiError, ErrorCode, PhotoApply, PhotoCrop, PhotoInfo, PhotoShape, PhotoSource};
use cv_core::{Id, ResolvedCv};
use cv_store::StoredPhoto;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits, RgbImage};

use crate::{App, Result, store_error};

/// File extensions the photo dialog offers.
pub const PHOTO_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png"];
/// Largest photo file accepted.
pub const MAX_PHOTO_FILE: usize = 5 * 1024 * 1024;
/// The kept original is scaled down to at most this many pixels a side.
const ORIGINAL_MAX_SIDE: u32 = 2000;
/// The cropped image's longer side, unless the crop is smaller.
const IMAGE_MAX_SIDE: u32 = 600;
/// The smallest crop accepted, in original pixels.
const MIN_CROP_SIDE: u32 = 32;
/// Images larger than this a side are refused before decoding.
const MAX_DECODED_SIDE: u32 = 12_000;
/// Chosen files kept at once; choosing another drops the oldest.
const KEPT_SOURCES: usize = 2;

struct Source {
    id: u64,
    jpeg: Arc<[u8]>,
}

/// Chosen photos waiting for `photo_apply`, already without metadata. Kept
/// in memory only.
#[derive(Default)]
pub struct PhotoSources {
    inner: Mutex<(u64, Vec<Source>)>,
}

impl PhotoSources {
    fn put(&self, jpeg: Arc<[u8]>) -> u64 {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.0 += 1;
        let id = inner.0;
        inner.1.push(Source { id, jpeg });
        if inner.1.len() > KEPT_SOURCES {
            inner.1.remove(0);
        }
        id
    }

    fn get(&self, id: u64) -> Result<Arc<[u8]>> {
        let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner
            .1
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.jpeg.clone())
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::Validation,
                    "This photo is no longer open. Choose it again.",
                )
            })
    }
}

fn not_an_image() -> ApiError {
    ApiError::new(
        ErrorCode::UnsupportedFormat,
        "The file is not a JPEG or PNG image that can be read.",
    )
}

fn encode_error() -> ApiError {
    ApiError::new(ErrorCode::Internal, "The photo could not be encoded.")
}

/// Reads at most [`MAX_PHOTO_FILE`] bytes.
fn read_photo(path: &Path) -> Result<Vec<u8>> {
    // The io::Error text can carry the path, which holds the user's name.
    let unreadable = |e: std::io::Error| {
        ApiError::new(
            ErrorCode::Io,
            "Could not read the file. Check that it still exists and you can open it.",
        )
        .with_details(serde_json::json!({ "kind": format!("{:?}", e.kind()) }))
    };
    let file = File::open(path).map_err(unreadable)?;
    let mut bytes = Vec::new();
    file.take(MAX_PHOTO_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    if bytes.len() > MAX_PHOTO_FILE {
        return Err(ApiError::new(
            ErrorCode::Validation,
            "That photo is larger than 5 MB. Choose a smaller one.",
        ));
    }
    Ok(bytes)
}

/// Decodes a JPEG or PNG, turned upright by its EXIF orientation. Only the
/// pixels are kept.
pub fn decode(bytes: &[u8]) -> Result<DynamicImage> {
    let format = image::guess_format(bytes).map_err(|_| not_an_image())?;
    if !matches!(format, ImageFormat::Jpeg | ImageFormat::Png) {
        return Err(not_an_image());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODED_SIDE);
    limits.max_image_height = Some(MAX_DECODED_SIDE);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| not_an_image())?;
    let orientation = decoder.orientation().ok();
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| not_an_image())?;
    if let Some(o) = orientation {
        image.apply_orientation(o);
    }
    Ok(image)
}

/// Flattens transparency onto white and drops everything but RGB pixels.
fn rgb(image: &DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let a = u16::from(p[3]);
        for c in 0..3 {
            let v = (u16::from(p[c]) * a + 255 * (255 - a)) / 255;
            o[c] = u8::try_from(v).unwrap_or(u8::MAX);
        }
    }
    out
}

/// A baseline JPEG of `image`. Encoding writes no metadata.
fn jpeg(image: &RgbImage, quality: u8) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .encode_image(image)
        .map_err(|_| encode_error())?;
    Ok(out)
}

/// `image` scaled down so neither side exceeds `max`; never scaled up.
fn fit_within(image: RgbImage, max_width: u32, max_height: u32) -> RgbImage {
    let (w, h) = image.dimensions();
    if w <= max_width && h <= max_height {
        return image;
    }
    let scale = f64::min(
        f64::from(max_width) / f64::from(w),
        f64::from(max_height) / f64::from(h),
    );
    let side = |n: u32| (f64::from(n) * scale).round().clamp(1.0, f64::from(n)) as u32;
    image::imageops::resize(&image, side(w), side(h), FilterType::Lanczos3)
}

/// The original to keep: upright, without metadata, at most
/// [`ORIGINAL_MAX_SIDE`] a side, as JPEG.
pub fn clean_original(bytes: &[u8]) -> Result<Vec<u8>> {
    let image = rgb(&decode(bytes)?);
    jpeg(&fit_within(image, ORIGINAL_MAX_SIDE, ORIGINAL_MAX_SIDE), 90)
}

/// `crop` made valid for an image of `width` × `height`: inside it, at
/// least [`MIN_CROP_SIDE`], and exactly the shape's aspect ratio (trimmed
/// evenly where the requested rectangle was off by rounding).
pub fn fit_crop(crop: PhotoCrop, width: u32, height: u32) -> Result<PhotoCrop> {
    let outside = || {
        ApiError::new(
            ErrorCode::Validation,
            "The crop is outside the photo. Frame it again.",
        )
    };
    let right = crop.x.checked_add(crop.width).ok_or_else(outside)?;
    let bottom = crop.y.checked_add(crop.height).ok_or_else(outside)?;
    if right > width || bottom > height {
        return Err(outside());
    }
    let (rw, rh) = crop.shape.ratio();
    let unit = (crop.width / rw).min(crop.height / rh);
    let (w, h) = (unit * rw, unit * rh);
    if w < MIN_CROP_SIDE || h < MIN_CROP_SIDE {
        return Err(ApiError::new(
            ErrorCode::Validation,
            "The crop is too small. Zoom out a little.",
        ));
    }
    Ok(PhotoCrop {
        x: crop.x + (crop.width - w) / 2,
        y: crop.y + (crop.height - h) / 2,
        width: w,
        height: h,
        shape: crop.shape,
    })
}

/// The image the CV shows: `crop` of `original`, at most
/// [`IMAGE_MAX_SIDE`] on its longer side.
pub fn cropped(original: &[u8], crop: PhotoCrop) -> Result<(Vec<u8>, PhotoCrop)> {
    let image = rgb(&decode(original)?);
    let crop = fit_crop(crop, image.width(), image.height())?;
    let part =
        image::imageops::crop_imm(&image, crop.x, crop.y, crop.width, crop.height).to_image();
    let (rw, rh) = crop.shape.ratio();
    let (max_w, max_h) = if rw >= rh {
        (IMAGE_MAX_SIDE, IMAGE_MAX_SIDE * rh / rw)
    } else {
        (IMAGE_MAX_SIDE * rw / rh, IMAGE_MAX_SIDE)
    };
    Ok((jpeg(&fit_within(part, max_w, max_h), 85)?, crop))
}

/// The largest crop of `shape` centred in a `width` × `height` image.
pub fn centred_crop(width: u32, height: u32, shape: PhotoShape) -> PhotoCrop {
    let (rw, rh) = shape.ratio();
    let unit = (width / rw).min(height / rh);
    let (w, h) = (unit * rw, unit * rh);
    PhotoCrop {
        x: (width - w) / 2,
        y: (height - h) / 2,
        width: w,
        height: h,
        shape,
    }
}

/// An imported photo (JPEG or PNG bytes from a file), cleaned like a chosen
/// one and cropped to the centred portrait frame, ready to store.
pub fn imported(bytes: &[u8]) -> Result<StoredPhoto> {
    let original = clean_original(bytes)?;
    let (width, height) = dimensions(&original)?;
    let (image, crop) = cropped(&original, centred_crop(width, height, PhotoShape::Portrait))?;
    Ok(StoredPhoto {
        original,
        image,
        crop: serde_json::to_string(&crop).map_err(|_| encode_error())?,
    })
}

pub(crate) fn data_url(jpeg: &[u8]) -> String {
    format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(jpeg)
    )
}

fn dimensions(jpeg: &[u8]) -> Result<(u32, u32)> {
    ImageReader::with_format(Cursor::new(jpeg), ImageFormat::Jpeg)
        .into_dimensions()
        .map_err(|_| not_an_image())
}

fn source(source_id: Option<u64>, jpeg: &[u8]) -> Result<PhotoSource> {
    let (width, height) = dimensions(jpeg)?;
    Ok(PhotoSource {
        source_id,
        data_url: data_url(jpeg),
        width,
        height,
    })
}

fn info(photo: &StoredPhoto) -> Result<PhotoInfo> {
    let crop: PhotoCrop = serde_json::from_str(&photo.crop).map_err(|_| {
        ApiError::new(
            ErrorCode::Internal,
            "The stored photo could not be read. Choose it again.",
        )
    })?;
    Ok(PhotoInfo {
        data_url: data_url(&photo.image),
        crop,
        original: source(None, &photo.original)?,
    })
}

impl App {
    /// Asks for a JPEG or PNG and returns it, without metadata, to crop.
    /// Nothing is saved until `photo_apply`.
    pub fn photo_choose(&self) -> Result<PhotoSource> {
        let path = self
            .dialog
            .choose_open_path(PHOTO_EXTENSIONS)
            .ok_or_else(|| ApiError::new(ErrorCode::Cancelled, "No photo chosen."))?;
        let original: Arc<[u8]> = clean_original(&read_photo(&path)?)?.into();
        let id = self.photo_sources.put(original.clone());
        source(Some(id), &original)
    }

    /// The CV's photo, if it has one.
    pub fn photo_get(&self, cv_id: &Id) -> Result<Option<PhotoInfo>> {
        self.store
            .photo_get(cv_id)
            .map_err(store_error)?
            .as_ref()
            .map(info)
            .transpose()
    }

    /// Crops a chosen photo, or the CV's original again, and saves it as the
    /// CV's photo.
    pub fn photo_apply(&self, req: PhotoApply) -> Result<PhotoInfo> {
        let original: Arc<[u8]> = match req.source_id {
            Some(id) => self.photo_sources.get(id)?,
            None => self
                .store
                .photo_get(&req.cv_id)
                .map_err(store_error)?
                .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "This CV has no photo to crop."))?
                .original
                .into(),
        };
        let (image, crop) = cropped(&original, req.crop)?;
        let photo = StoredPhoto {
            original: original.to_vec(),
            image,
            crop: serde_json::to_string(&crop).map_err(|_| encode_error())?,
        };
        self.store
            .photo_put(&req.cv_id, &photo)
            .map_err(store_error)?;
        info(&photo)
    }

    /// Removes the CV's photo.
    pub fn photo_remove(&self, cv_id: &Id) -> Result<()> {
        self.store.photo_delete(cv_id).map_err(store_error)
    }

    /// The photo to draw for `cv`: its cropped image when it shows one.
    pub(crate) fn photo_for(&self, cv: &ResolvedCv) -> Result<Option<Arc<[u8]>>> {
        if !cv.show_photo {
            return Ok(None);
        }
        Ok(self
            .store
            .photo_get(&cv.cv_id)
            .map_err(store_error)?
            .map(|p| p.image.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crop(x: u32, y: u32, width: u32, height: u32, shape: PhotoShape) -> PhotoCrop {
        PhotoCrop {
            x,
            y,
            width,
            height,
            shape,
        }
    }

    #[test]
    fn crops_are_kept_inside_and_in_shape() {
        let c = fit_crop(crop(10, 20, 301, 400, PhotoShape::Portrait), 400, 500).expect("fits");
        assert_eq!((c.width, c.height), (300, 400));
        assert_eq!((c.x, c.y), (10, 20));
        let c = fit_crop(crop(0, 0, 200, 210, PhotoShape::Square), 400, 500).expect("fits");
        assert_eq!((c.x, c.y, c.width, c.height), (0, 5, 200, 200));
        assert!(fit_crop(crop(300, 0, 200, 200, PhotoShape::Square), 400, 500).is_err());
        assert!(fit_crop(crop(u32::MAX, 0, 2, 2, PhotoShape::Square), 400, 500).is_err());
        assert!(fit_crop(crop(0, 0, 20, 20, PhotoShape::Square), 400, 500).is_err());
    }

    #[test]
    fn the_default_crop_is_centred_and_in_shape() {
        let c = centred_crop(800, 600, PhotoShape::Portrait);
        assert_eq!((c.x, c.y, c.width, c.height), (175, 0, 450, 600));
        let c = centred_crop(300, 901, PhotoShape::Square);
        assert_eq!((c.x, c.y, c.width, c.height), (0, 300, 300, 300));
    }
}
