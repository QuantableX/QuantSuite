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

/// Capture the primary screen, optionally cropping to a region, return as base64 PNG
/// When `default_crop` is true and no region is given, crops to right 20% (fib levels).
/// When false, returns the full screen.
pub fn capture_screen_base64(region: Option<[i32; 4]>, default_crop: bool) -> Result<(String, u32, u32), CaptureError> {
    let screens = Screen::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;

    if screens.is_empty() {
        return Err(CaptureError::NoScreens);
    }

    // Get primary screen (first one)
    let screen = &screens[0];

    let capture = screen
        .capture()
        .map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;

    // Convert from screenshots' image type to our image crate version
    let width = capture.width();
    let height = capture.height();
    let raw_pixels: Vec<u8> = capture.into_raw();

    let img = RgbaImage::from_raw(width, height, raw_pixels)
        .ok_or_else(|| CaptureError::CaptureFailed("Failed to create image".into()))?;

    let mut dynamic_img = DynamicImage::ImageRgba8(img);

    // Apply region crop if specified [x, y, width, height]
    if let Some([x, y, w, h]) = region {
        let crop_x = x.max(0) as u32;
        let crop_y = y.max(0) as u32;
        let crop_w = w.max(1) as u32;
        let crop_h = h.max(1) as u32;

        dynamic_img = dynamic_img.crop_imm(crop_x, crop_y, crop_w, crop_h);
    } else if default_crop {
        // Default: crop to right 20% where fib levels typically appear
        let crop_x = (width as f32 * 0.80) as u32;
        dynamic_img = dynamic_img.crop_imm(crop_x, 0, width - crop_x, height);
    }

    let final_width = dynamic_img.width();
    let final_height = dynamic_img.height();

    // Encode to PNG and base64
    let mut png_bytes = Cursor::new(Vec::new());
    dynamic_img
        .write_to(&mut png_bytes, ImageFormat::Png)
        .map_err(|e| CaptureError::EncodeFailed(e.to_string()))?;

    let base64_data = STANDARD.encode(png_bytes.into_inner());

    Ok((base64_data, final_width, final_height))
}

