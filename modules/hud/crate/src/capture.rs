use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageFormat, RgbaImage};
use screenshots::Screen;
use std::io::Cursor;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("No screens found")]
    NoScreens,
    #[error("Failed to capture screen: {0}")]
    CaptureFailed(String),
    #[error("Failed to encode image: {0}")]
    EncodeFailed(String),
}

/// A region needs at least this much of itself on the screen in both axes —
/// the region selector itself rejects drags of 10px or less.
const MIN_REGION_PX: u32 = 10;

pub struct CapturedImage {
    pub image_base64: String,
    pub width: u32,
    pub height: u32,
    /// The requested region was (almost) entirely off the captured screen —
    /// saved under another display layout — so the default crop was used.
    pub region_dropped: bool,
}

/// The `[x, y, width, height]` to keep of a `width` x `height` capture, and
/// whether the requested region had to be dropped. A region is clipped to the
/// screen; one that keeps less than `MIN_REGION_PX` in either axis is dropped
/// for the default crop, which would otherwise be an empty image.
fn resolve_crop(width: u32, height: u32, region: Option<[i32; 4]>, default_crop: bool) -> ([u32; 4], bool) {
    let fallback = if default_crop {
        // Right 20% of the screen, where fib levels typically appear
        let crop_x = (width as f32 * 0.80) as u32;
        [crop_x, 0, width - crop_x, height]
    } else {
        [0, 0, width, height]
    };
    let Some([x, y, w, h]) = region else {
        return (fallback, false);
    };
    // i64: x + w must not overflow, and negative origins clip to 0
    let left = (x as i64).clamp(0, width as i64);
    let top = (y as i64).clamp(0, height as i64);
    let right = (x as i64 + w.max(0) as i64).clamp(0, width as i64);
    let bottom = (y as i64 + h.max(0) as i64).clamp(0, height as i64);
    let (crop_w, crop_h) = ((right - left) as u32, (bottom - top) as u32);
    if crop_w < MIN_REGION_PX || crop_h < MIN_REGION_PX {
        return (fallback, true);
    }
    ([left as u32, top as u32, crop_w, crop_h], false)
}

/// Capture the primary screen, optionally cropping to a region, return as base64 PNG
/// When `default_crop` is true and no usable region is given, crops to right 20% (fib levels).
/// When false, returns the full screen.
pub fn capture_screen_base64(region: Option<[i32; 4]>, default_crop: bool) -> Result<CapturedImage, CaptureError> {
    let screens = Screen::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;

    // The region selector opens on the primary monitor, which is not always
    // the first one enumerated.
    let screen = screens
        .iter()
        .find(|s| s.display_info.is_primary)
        .or_else(|| screens.first())
        .ok_or(CaptureError::NoScreens)?;

    let capture = screen
        .capture()
        .map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;

    // Convert from screenshots' image type to our image crate version
    let width = capture.width();
    let height = capture.height();
    let raw_pixels: Vec<u8> = capture.into_raw();

    let img = RgbaImage::from_raw(width, height, raw_pixels)
        .ok_or_else(|| CaptureError::CaptureFailed("Failed to create image".into()))?;

    let ([crop_x, crop_y, crop_w, crop_h], region_dropped) = resolve_crop(width, height, region, default_crop);
    let dynamic_img = DynamicImage::ImageRgba8(img).crop_imm(crop_x, crop_y, crop_w, crop_h);

    let final_width = dynamic_img.width();
    let final_height = dynamic_img.height();

    // Encode to PNG and base64
    let mut png_bytes = Cursor::new(Vec::new());
    dynamic_img
        .write_to(&mut png_bytes, ImageFormat::Png)
        .map_err(|e| CaptureError::EncodeFailed(e.to_string()))?;

    let base64_data = STANDARD.encode(png_bytes.into_inner());

    Ok(CapturedImage {
        image_base64: base64_data,
        width: final_width,
        height: final_height,
        region_dropped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_region_uses_the_default_crop() {
        assert_eq!(resolve_crop(1920, 1200, None, true), ([1536, 0, 384, 1200], false));
        assert_eq!(resolve_crop(1920, 1200, None, false), ([0, 0, 1920, 1200], false));
    }

    #[test]
    fn region_inside_the_screen_is_kept() {
        assert_eq!(resolve_crop(1920, 1200, Some([1500, 100, 276, 900]), true), ([1500, 100, 276, 900], false));
    }

    #[test]
    fn region_partly_off_screen_is_clipped() {
        // The chart analyzer region saved on a 2560px-wide layout
        assert_eq!(resolve_crop(1920, 1200, Some([436, 96, 1978, 1365]), false), ([436, 96, 1484, 1104], false));
        assert_eq!(resolve_crop(1920, 1200, Some([-50, -20, 200, 120]), true), ([0, 0, 150, 100], false));
    }

    #[test]
    fn region_off_screen_is_dropped_for_the_default_crop() {
        // The position sizer region that produced a zero-width image
        assert_eq!(resolve_crop(1920, 1200, Some([2154, 103, 276, 1358]), true), ([1536, 0, 384, 1200], true));
        assert_eq!(resolve_crop(1920, 1200, Some([2154, 103, 276, 1358]), false), ([0, 0, 1920, 1200], true));
        // Only a sliver on screen, and a degenerate region
        assert_eq!(resolve_crop(1920, 1200, Some([1915, 100, 300, 300]), true), ([1536, 0, 384, 1200], true));
        assert_eq!(resolve_crop(1920, 1200, Some([100, 100, 0, 0]), true), ([1536, 0, 384, 1200], true));
    }

    #[test]
    fn huge_values_do_not_overflow() {
        assert_eq!(resolve_crop(1920, 1200, Some([i32::MAX, i32::MAX, i32::MAX, i32::MAX]), true), ([1536, 0, 384, 1200], true));
        assert_eq!(resolve_crop(1920, 1200, Some([i32::MIN, i32::MIN, i32::MAX, i32::MAX]), false), ([0, 0, 1920, 1200], true));
    }
}
