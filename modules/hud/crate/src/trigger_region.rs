//! The native window follows the same soft tab contour as the Vue button.
//! CSS transparency alone does not remove a WebView's rectangular hit area.

use super::layout::Edge;
#[cfg(any(windows, test))]
use super::layout::TOP_RADIUS;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
struct Geometry {
    width: f64,
    height: f64,
    curves: Vec<[f64; 8]>,
}

fn geometry() -> &'static Geometry {
    static GEOMETRY: OnceLock<Geometry> = OnceLock::new();
    GEOMETRY.get_or_init(|| {
        serde_json::from_str(include_str!("../../app/assets/trigger-tab.json"))
            .expect("valid bundled HUD trigger geometry")
    })
}

pub fn height(scale: f64) -> u32 {
    (geometry().height * scale).round() as u32
}

/// The expanded panel and its tab form one region. Mirror the entire contour
/// for the right screen edge; transparent gutters never become click targets.
#[cfg(any(windows, test))]
fn outline(width: u32, height: u32, scale: f64, right: bool, expanded: bool) -> Vec<(i32, i32)> {
    let g = geometry();
    let content = if expanded {
        width as f64 - g.width * scale
    } else {
        0.0
    };
    let top = (height as f64 - g.height * scale) / 2.0;
    let mut points = Vec::new();
    if expanded {
        points.extend([(0, 0), (content.round() as i32, 0)]);
    }
    for curve in &g.curves {
        for step in 0..=32 {
            let t = step as f64 / 32.0;
            let u = 1.0 - t;
            let at = |axis| {
                u * u * u * curve[axis]
                    + 3.0 * u * u * t * curve[axis + 2]
                    + 3.0 * u * t * t * curve[axis + 4]
                    + t * t * t * curve[axis + 6]
            };
            // Start with an enclosing contour. make_region then insets its
            // curved edge onto opaque paint, outside the SVG's alpha fringe.
            points.push((
                (content + at(0) * scale).ceil() as i32,
                (top + at(1) * scale).round() as i32,
            ));
        }
    }
    if expanded {
        points.extend([(content.round() as i32, height as i32), (0, height as i32)]);
    }
    if right {
        for (x, _) in &mut points {
            *x = width as i32 - *x;
        }
    }
    points.dedup();
    points
}

/// Transpose the shared tab contour to point down. The top panel has rounded
/// lower corners; the screen-facing edge remains flush with the work area.
#[cfg(any(windows, test))]
fn top_outline(width: u32, height: u32, scale: f64, expanded: bool) -> Vec<(i32, i32)> {
    let tab = outline(height, width, scale, false, false);
    // outline centers the long side; its depth origin for this transposition
    // is the lower edge of the content area.
    let content = if expanded {
        height as f64 - geometry().width * scale
    } else {
        0.0
    };
    let mut points = Vec::new();
    let radius = (TOP_RADIUS * scale).min(content).min(width as f64 / 2.0);
    let arc = |cx: f64, start: f64| -> Vec<(i32, i32)> {
        (0..=24)
            .map(|i| {
                let angle = start - i as f64 / 24.0 * std::f64::consts::FRAC_PI_2;
                (
                    (cx + radius * angle.cos()).round() as i32,
                    (content - radius + radius * angle.sin()).round() as i32,
                )
            })
            .collect()
    };
    if expanded {
        points.push((0, 0));
        points.extend(arc(radius, std::f64::consts::PI));
    }
    points.extend(
        tab.into_iter()
            .map(|(x, y)| (y, x + content.round() as i32)),
    );
    if expanded {
        points.extend(arc(width as f64 - radius, std::f64::consts::FRAC_PI_2));
        points.push((width as i32, 0));
    }
    points.dedup();
    points
}

#[cfg(windows)]
fn make_region(
    width: u32,
    height: u32,
    scale: f64,
    edge: Edge,
    expanded: bool,
) -> Result<windows::Win32::Graphics::Gdi::HRGN, String> {
    use windows::Win32::{
        Foundation::POINT,
        Graphics::Gdi::{
            CombineRgn, CreatePolygonRgn, CreateRectRgn, DeleteObject, OffsetRgn, SetRectRgn,
            RGN_AND, RGN_COPY, RGN_OR, WINDING,
        },
    };
    let outline = if edge == Edge::Top {
        top_outline(width, height, scale, expanded)
    } else {
        outline(width, height, scale, edge == Edge::Right, expanded)
    };
    let points: Vec<POINT> = outline.into_iter().map(|(x, y)| POINT { x, y }).collect();
    let original = unsafe { CreatePolygonRgn(&points, WINDING) };
    let shifted = unsafe { CreateRectRgn(0, 0, 0, 0) };
    let region = unsafe { CreateRectRgn(0, 0, 0, 0) };
    let result = (|| {
        if [original, shifted, region].iter().any(|r| r.0.is_null()) {
            return Err("Could not create HUD trigger region".into());
        }
        let check = |value: i32| {
            if value == 0 {
                Err("Could not inset HUD trigger region".to_string())
            } else {
                Ok(())
            }
        };
        unsafe {
            check(CombineRgn(Some(region), Some(original), None, RGN_COPY).0)?;
            // A regioned WebView2 surface composites partially transparent
            // edge pixels as bright flecks. Keep the native boundary one
            // PHYSICAL pixel inside the painted curve, including its sloping
            // shoulders. Do not shrink the straight screen-side edge.
            for offset in -1..=1 {
                let (dx, dy) = match edge {
                    Edge::Left => (-1, offset),
                    Edge::Right => (1, offset),
                    Edge::Top => (offset, -1),
                };
                check(CombineRgn(Some(shifted), Some(original), None, RGN_COPY).0)?;
                check(OffsetRgn(shifted, dx, dy).0)?;
                check(CombineRgn(Some(region), Some(region), Some(shifted), RGN_AND).0)?;
            }
            // The rectangular panel stays full height and width: only the
            // tab needs the inset, never the panel's top/bottom rows.
            if expanded {
                let (left, right, bottom) = if edge == Edge::Top {
                    // Restore the straight edges above the rounded corners;
                    // keep the bottom curves inside their opaque paint.
                    (
                        0,
                        width as i32,
                        (height as f64 - (geometry().width + TOP_RADIUS) * scale).round() as i32,
                    )
                } else {
                    let content = (width as f64 - geometry().width * scale).round() as i32;
                    if edge == Edge::Right {
                        (width as i32 - content, width as i32, height as i32)
                    } else {
                        (0, content, height as i32)
                    }
                };
                check(SetRectRgn(shifted, left, 0, right, bottom.max(0)).as_bool() as i32)?;
                check(CombineRgn(Some(region), Some(region), Some(shifted), RGN_OR).0)?;
            }
        }
        Ok(region)
    })();
    // Only the returned region transfers to the caller (then Windows).
    for scratch in [original, shifted] {
        if !scratch.0.is_null() {
            unsafe {
                let _ = DeleteObject(scratch.into());
            }
        }
    }
    if result.is_err() && !region.0.is_null() {
        unsafe {
            let _ = DeleteObject(region.into());
        }
    }
    result
}

pub fn apply(
    window: &tauri::WebviewWindow,
    scale: f64,
    edge: Edge,
    expanded: bool,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::{
            Foundation::HWND,
            Graphics::Gdi::{DeleteObject, SetWindowRgn},
        };
        let size = window.inner_size().map_err(|e| e.to_string())?;
        let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
        let region = make_region(size.width, size.height, scale, edge, expanded)?;
        // SetWindowRgn transfers ownership only on success. Do not delete a
        // region now owned by Windows. See Microsoft SetWindowRgn docs.
        if unsafe { SetWindowRgn(hwnd, Some(region), true) } == 0 {
            unsafe {
                let _ = DeleteObject(region.into());
            }
            return Err("Could not apply HUD trigger region".into());
        }
    }
    #[cfg(not(windows))]
    let _ = (window, scale, edge, expanded);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_contour_stays_inside_its_frame() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for expanded in [false, true] {
                let width = ((if expanded { 1920.0 } else { 88.0 }) * scale) as u32;
                let height = ((if expanded { 232.0 } else { 20.0 }) * scale) as u32;
                let points = top_outline(width, height, scale, expanded);
                assert!(points
                    .iter()
                    .all(|&(x, y)| x >= 0 && y >= 0 && x <= width as i32 && y <= height as i32));
                assert!(points.iter().any(|&(x, y)| (x - width as i32 / 2).abs() < 2
                    && y > height as i32 - (3.0 * scale) as i32));
            }
        }
    }

    #[test]
    #[cfg(windows)]
    fn top_region_follows_the_rotated_tab_and_rounded_panel() {
        use windows::Win32::Graphics::Gdi::{DeleteObject, PtInRegion};
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for expanded in [false, true] {
                let width = ((if expanded { 1920.0 } else { 88.0 }) * scale) as u32;
                let height = ((if expanded { 232.0 } else { 20.0 }) * scale) as u32;
                let region = make_region(width, height, scale, Edge::Top, expanded).unwrap();
                let x = width as i32 / 2;
                let y = height as i32 - (10.0 * scale) as i32;
                unsafe {
                    assert!(PtInRegion(region, x, y).as_bool(), "tab center");
                    assert!(!PtInRegion(region, 1, y).as_bool(), "empty gutter");
                    assert!(!PtInRegion(region, width as i32 - 2, y).as_bool());
                    if expanded {
                        assert!(PtInRegion(region, 0, 0).as_bool(), "flush screen edge");
                        assert!(PtInRegion(region, width as i32 - 1, 0).as_bool());
                        assert!(
                            !PtInRegion(region, 1, height as i32 - (20.0 * scale) as i32 - 1)
                                .as_bool(),
                            "rounded corner"
                        );
                        assert!(PtInRegion(region, x, 1).as_bool());
                    } else if scale == 1.0 {
                        for (x, y) in [(10, 5), (13, 10), (13, 11)] {
                            assert!(!PtInRegion(region, x, y).as_bool(), "rotated fringe");
                        }
                    }
                    assert!(DeleteObject(region.into()).as_bool());
                }
            }
        }
    }

    #[test]
    fn contour_scales_and_mirrors_without_a_full_height_trigger_strip() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for expanded in [false, true] {
                let width = ((if expanded { 340.0 } else { 20.0 }) * scale) as u32;
                let height = if expanded { 1040 } else { height(scale) };
                let left = outline(width, height, scale, false, expanded);
                let right = outline(width, height, scale, true, expanded);
                assert_eq!(left.len(), right.len());
                for ((x, y), (rx, ry)) in left.iter().zip(&right) {
                    assert_eq!(x + rx, width as i32);
                    assert_eq!(y, ry);
                    assert!(*x >= 0 && *x <= width as i32 && *y >= 0 && *y <= height as i32);
                }
            }
        }
        assert_eq!(geometry().width, super::super::TRIGGER_WIDTH as f64);
    }

    #[test]
    #[cfg(windows)]
    fn native_region_includes_content_and_tab_but_excludes_empty_corners() {
        use windows::Win32::Graphics::Gdi::{DeleteObject, PtInRegion};
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for right in [false, true] {
                for expanded in [false, true] {
                    let width = ((if expanded { 340.0 } else { 20.0 }) * scale) as u32;
                    let height = if expanded { 1040 } else { height(scale) };
                    let region = make_region(
                        width,
                        height,
                        scale,
                        if right { Edge::Right } else { Edge::Left },
                        expanded,
                    )
                    .unwrap();
                    let tab_x = if expanded { 330.0 } else { 10.0 };
                    let x = (tab_x * scale).round() as i32;
                    let x = if right { width as i32 - x } else { x };
                    unsafe {
                        assert!(PtInRegion(region, x, height as i32 / 2).as_bool());
                        assert!(!PtInRegion(region, x, 1).as_bool());
                        assert!(!PtInRegion(region, x, height as i32 - 2).as_bool());
                        if expanded {
                            assert!(PtInRegion(region, width as i32 / 2, 1).as_bool());
                            assert!(PtInRegion(region, width as i32 / 2, 0).as_bool());
                            assert!(
                                PtInRegion(region, width as i32 / 2, height as i32 - 1).as_bool()
                            );
                        }
                        assert!(DeleteObject(region.into()).as_bool());
                    }
                }
            }
        }
    }

    #[test]
    #[cfg(windows)]
    fn native_boundary_excludes_the_observed_bright_fringe_pixels() {
        use windows::Win32::Graphics::Gdi::{DeleteObject, PtInRegion};
        // Screen capture of the 100% DPI native WebView showed bright pixels
        // at these shoulder coordinates, while the SVG preview was clean.
        for right in [false, true] {
            let region = make_region(
                20,
                88,
                1.0,
                if right { Edge::Right } else { Edge::Left },
                false,
            )
            .unwrap();
            unsafe {
                for (x, y) in [(5, 10), (10, 13), (11, 13)] {
                    let x = if right { 19 - x } else { x };
                    assert!(!PtInRegion(region, x, y).as_bool(), "fringe at ({x}, {y})");
                }
                assert!(PtInRegion(region, 10, 44).as_bool());
                assert!(DeleteObject(region.into()).as_bool());
            }
        }
    }
}
