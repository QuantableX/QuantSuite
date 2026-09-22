//! Physical placement is shared by every overlay transition. The frontend
//! fills the resulting viewport; it never guesses a monitor's DPI or origin.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
}

impl Edge {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "top" => Ok(Self::Top),
            _ => Err(format!("Unknown HUD edge: {value}")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[cfg(any(windows, test))]
pub const TOP_RADIUS: f64 = 14.0;

pub fn frame(area: Frame, scale: f64, edge: Edge, expanded: bool) -> Frame {
    let px = |value: f64| (value * scale).round() as u32;
    let tab_depth = px(super::TRIGGER_WIDTH as f64);
    let tab_length = super::trigger_region::height(scale);
    let (width, height) = match (edge, expanded) {
        (Edge::Top, true) => (area.width, px(232.0).min(area.height)),
        (Edge::Top, false) => (tab_length, tab_depth),
        (_, true) => (px(super::TOTAL_WIDTH as f64), area.height),
        (_, false) => (tab_depth, tab_length),
    };
    let width = width.min(area.width);
    let height = height.min(area.height);
    Frame {
        x: area.x
            + match edge {
                Edge::Left => 0,
                Edge::Right => (area.width - width) as i32,
                Edge::Top => ((area.width - width) / 2) as i32,
            },
        y: area.y
            + if edge != Edge::Top && !expanded {
                ((area.height - height) / 2) as i32
            } else {
                0
            },
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_is_centered_in_the_work_area_at_every_dpi_and_size() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for (width, height) in [(3840, 2120), (1920, 1040), (800, 560)] {
                // Negative monitor origin and a taskbar on the top/left.
                let area = Frame {
                    x: -3800,
                    y: -1000,
                    width,
                    height,
                };
                for expanded in [false, true] {
                    let f = frame(area, scale, Edge::Top, expanded);
                    assert_eq!(f.y, area.y);
                    assert!((2 * (f.x - area.x) + f.width as i32 - width as i32).abs() <= 1);
                    assert!(f.x >= area.x && f.x + f.width as i32 <= area.x + width as i32);
                    assert!(f.height <= height);
                    if expanded {
                        assert_eq!(f.x, area.x);
                        assert_eq!(f.width, area.width);
                        assert_eq!(f.height, (232.0 * scale).round().min(height as f64) as u32);
                    }
                    if !expanded {
                        assert_eq!(f.width, (88.0 * scale) as u32);
                        assert_eq!(f.height, (20.0 * scale) as u32);
                    }
                }
            }
        }
    }

    #[test]
    fn side_panels_and_tabs_keep_their_sizes_and_selected_edge() {
        let area = Frame {
            x: -1920,
            y: 48,
            width: 1920,
            height: 1032,
        };
        for edge in [Edge::Left, Edge::Right] {
            for expanded in [false, true] {
                let f = frame(area, 1.0, edge, expanded);
                assert_eq!(f.width, if expanded { 340 } else { 20 });
                assert_eq!(f.height, if expanded { 1032 } else { 88 });
                if edge == Edge::Left {
                    assert_eq!(f.x, area.x);
                } else {
                    assert_eq!(f.x + f.width as i32, area.x + area.width as i32);
                }
            }
        }
    }
}
