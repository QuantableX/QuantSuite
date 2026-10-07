//! Geometry shared by monitor selection and screenshot cropping.
//! All rectangles use xcap's desktop coordinates: points on macOS, pixels elsewhere.

#[derive(Clone, Copy, Debug)]
pub(crate) struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// Tauri scales both the global origin and size by the window's backing scale
    /// on macOS. Undo that together; scaling each monitor's origin independently
    /// would break mixed-DPI layouts. Other platforms pass a scale of 1.
    pub fn from_window(x: i32, y: i32, width: i32, height: i32, scale: f64) -> Self {
        Self {
            x: f64::from(x) / scale,
            y: f64::from(y) / scale,
            width: f64::from(width) / scale,
            height: f64::from(height) / scale,
        }
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right > x && bottom > y).then_some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }
}

/// Prefer the display showing the largest part of the window. Its center can be
/// off-screen (or in a gap between displays) while the window is still visible.
pub(crate) fn select_monitor(window: Rect, monitors: &[Rect]) -> Option<usize> {
    monitors
        .iter()
        .enumerate()
        .filter_map(|(index, monitor)| {
            window
                .intersection(*monitor)
                .map(|visible| (index, visible.width * visible.height))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

/// Intersect in desktop space first, then map the edges to captured image pixels.
/// The actual bitmap size is authoritative, including Retina/scaled resolutions.
pub(crate) fn crop_rect(
    window: Rect,
    monitor: Rect,
    image_width: u32,
    image_height: u32,
) -> Option<(u32, u32, u32, u32)> {
    if image_width == 0 || image_height == 0 {
        return None;
    }
    let visible = window.intersection(monitor)?;
    let scale_x = f64::from(image_width) / monitor.width;
    let scale_y = f64::from(image_height) / monitor.height;
    let left = ((visible.x - monitor.x) * scale_x)
        .floor()
        .clamp(0.0, f64::from(image_width)) as u32;
    let top = ((visible.y - monitor.y) * scale_y)
        .floor()
        .clamp(0.0, f64::from(image_height)) as u32;
    let right = ((visible.x + visible.width - monitor.x) * scale_x)
        .ceil()
        .clamp(0.0, f64::from(image_width)) as u32;
    let bottom = ((visible.y + visible.height - monitor.y) * scale_y)
        .ceil()
        .clamp(0.0, f64::from(image_height)) as u32;
    (right > left && bottom > top).then_some((left, top, right - left, bottom - top))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn maximized_retina_window_is_found_and_cropped_at_native_resolution() {
        let monitor = rect(0.0, 0.0, 1512.0, 982.0);
        // Previously the unconverted center was x=1512, outside this monitor.
        let window = Rect::from_window(0, 48, 3024, 1800, 2.0);
        assert_eq!(select_monitor(window, &[monitor]), Some(0));
        assert_eq!(
            crop_rect(window, monitor, 3024, 1964),
            Some((0, 48, 3024, 1800))
        );
    }

    #[test]
    fn retina_window_on_offset_display_uses_window_scale_for_global_origin() {
        let monitors = [
            rect(0.0, 0.0, 1920.0, 1080.0),
            rect(1920.0, -200.0, 1512.0, 982.0),
        ];
        let window = Rect::from_window(4000, -200, 2400, 1400, 2.0);
        assert_eq!(select_monitor(window, &monitors), Some(1));
        assert_eq!(
            crop_rect(window, monitors[1], 3024, 1964),
            Some((160, 200, 2400, 1400))
        );
    }

    #[test]
    fn unscaled_external_display_left_of_retina_display() {
        let monitors = [
            rect(0.0, 0.0, 1512.0, 982.0),
            rect(-1920.0, -100.0, 1920.0, 1080.0),
        ];
        let window = Rect::from_window(-1800, -50, 1600, 900, 1.0);
        assert_eq!(select_monitor(window, &monitors), Some(1));
        assert_eq!(
            crop_rect(window, monitors[1], 1920, 1080),
            Some((120, 50, 1600, 900))
        );
    }

    #[test]
    fn windows_physical_coordinates_are_not_scaled_again() {
        let monitor = rect(1920.0, 0.0, 2560.0, 1440.0);
        let window = Rect::from_window(2020, 80, 1600, 1200, 1.0);
        assert_eq!(
            crop_rect(window, monitor, 2560, 1440),
            Some((100, 80, 1600, 1200))
        );
    }

    #[test]
    fn crop_uses_bitmap_dimensions_instead_of_assuming_backing_scale() {
        let monitor = rect(0.0, 0.0, 1600.0, 1000.0);
        let window = Rect::from_window(200, 100, 2000, 1600, 2.0);
        assert_eq!(
            crop_rect(window, monitor, 2400, 1500),
            Some((150, 75, 1500, 1200))
        );
    }

    #[test]
    fn partly_off_screen_window_does_not_include_extra_desktop_pixels() {
        let monitor = rect(0.0, 0.0, 1000.0, 800.0);
        let window = rect(-100.0, -50.0, 600.0, 400.0);
        assert_eq!(
            crop_rect(window, monitor, 2000, 1600),
            Some((0, 0, 1000, 700))
        );
        let window = rect(900.0, 700.0, 600.0, 400.0);
        assert_eq!(
            crop_rect(window, monitor, 2000, 1600),
            Some((1800, 1400, 200, 200))
        );
    }

    #[test]
    fn visible_window_with_center_off_screen_can_still_be_captured() {
        let monitor = rect(0.0, 0.0, 1000.0, 800.0);
        let window = rect(-700.0, 100.0, 1000.0, 600.0);
        assert_eq!(select_monitor(window, &[monitor]), Some(0));
        assert_eq!(
            crop_rect(window, monitor, 1000, 800),
            Some((0, 100, 300, 600))
        );
    }

    #[test]
    fn spanning_window_selects_largest_visible_area_even_with_center_in_gap() {
        let monitors = [
            rect(0.0, 0.0, 1000.0, 800.0),
            rect(1200.0, 0.0, 1000.0, 800.0),
        ];
        let window = rect(700.0, 100.0, 700.0, 600.0);
        assert_eq!(select_monitor(window, &monitors), Some(0));
    }

    #[test]
    fn fractional_edges_round_outward_and_stay_in_image() {
        let monitor = rect(0.0, 0.0, 1000.0, 800.0);
        let window = rect(10.25, 20.25, 100.5, 200.5);
        assert_eq!(
            crop_rect(window, monitor, 1500, 1200),
            Some((15, 30, 152, 302))
        );
    }

    #[test]
    fn disjoint_empty_and_touching_rectangles_do_not_produce_a_crop() {
        let monitor = rect(0.0, 0.0, 1000.0, 800.0);
        for window in [
            rect(1000.0, 0.0, 500.0, 500.0),
            rect(-600.0, 0.0, 500.0, 500.0),
            rect(10.0, 10.0, 0.0, 10.0),
        ] {
            assert_eq!(select_monitor(window, &[monitor]), None);
            assert_eq!(crop_rect(window, monitor, 1000, 800), None);
        }
        assert_eq!(select_monitor(monitor, &[]), None);
        assert_eq!(crop_rect(monitor, monitor, 0, 800), None);
        assert_eq!(crop_rect(monitor, monitor, 1000, 0), None);
    }
}
