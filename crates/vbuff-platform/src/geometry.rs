//! Physical-coordinate popup placement for mixed-DPI work areas.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl PhysicalRect {
    fn contains(self, point: PhysicalPoint) -> bool {
        let right = i64::from(self.x) + i64::from(self.width);
        let bottom = i64::from(self.y) + i64::from(self.height);
        i64::from(point.x) >= i64::from(self.x)
            && i64::from(point.x) < right
            && i64::from(point.y) >= i64::from(self.y)
            && i64::from(point.y) < bottom
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonitorWorkArea {
    pub id: u32,
    pub work_area: PhysicalRect,
    pub scale_factor: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupPlacement {
    pub monitor_id: u32,
    pub physical_origin: PhysicalPoint,
    pub scale_factor: f32,
}

pub fn place_popup(
    cursor: PhysicalPoint,
    popup_logical_size: (f32, f32),
    monitors: &[MonitorWorkArea],
) -> Option<PopupPlacement> {
    let monitor = monitors
        .iter()
        .find(|monitor| monitor.work_area.contains(cursor))
        .or_else(|| monitors.first())?;
    let scale = if monitor.scale_factor.is_finite() && monitor.scale_factor > 0.0 {
        monitor.scale_factor
    } else {
        1.0
    };
    let popup_width = (popup_logical_size.0.max(1.0) * scale).round() as i64;
    let popup_height = (popup_logical_size.1.max(1.0) * scale).round() as i64;
    let area = monitor.work_area;
    let min_x = i64::from(area.x);
    let min_y = i64::from(area.y);
    let max_x = (min_x + i64::from(area.width) - popup_width).max(min_x);
    let max_y = (min_y + i64::from(area.height) - popup_height).max(min_y);
    let desired_x = i64::from(cursor.x) - popup_width / 2;
    let desired_y = i64::from(cursor.y) + (12.0 * scale).round() as i64;
    Some(PopupPlacement {
        monitor_id: monitor.id,
        physical_origin: PhysicalPoint {
            x: desired_x.clamp(min_x, max_x) as i32,
            y: desired_y.clamp(min_y, max_y) as i32,
        },
        scale_factor: scale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_cursor_monitor_and_clamps_mixed_dpi_popup() {
        let monitors = [
            MonitorWorkArea {
                id: 1,
                work_area: PhysicalRect {
                    x: 0,
                    y: 0,
                    width: 1_920,
                    height: 1_080,
                },
                scale_factor: 1.0,
            },
            MonitorWorkArea {
                id: 2,
                work_area: PhysicalRect {
                    x: 1_920,
                    y: 0,
                    width: 2_560,
                    height: 1_400,
                },
                scale_factor: 2.0,
            },
        ];
        let placement = place_popup(
            PhysicalPoint { x: 4_470, y: 1_390 },
            (500.0, 400.0),
            &monitors,
        )
        .unwrap();
        assert_eq!(placement.monitor_id, 2);
        assert_eq!(placement.scale_factor, 2.0);
        assert!(placement.physical_origin.x >= 1_920);
        assert!(placement.physical_origin.y <= 600);
    }
}
