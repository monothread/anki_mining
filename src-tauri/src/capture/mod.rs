//! Screen capture behind a trait, so the X11 backend can be replaced by a Wayland one later
//! (ADR-003).

pub mod x11;

use image::RgbImage;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use x11::X11Capture;

/// A rectangle of the screen in pixels; `x` and `y` are measured from the top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    /// The part of the region that lies inside a screen of the given size; `None` if nothing does.
    pub fn clamp_to(&self, screen_width: u32, screen_height: u32) -> Option<Region> {
        let left = i64::from(self.x).max(0);
        let top = i64::from(self.y).max(0);
        let right = (i64::from(self.x) + i64::from(self.width)).min(i64::from(screen_width));
        let bottom = (i64::from(self.y) + i64::from(self.height)).min(i64::from(screen_height));
        if right <= left || bottom <= top {
            return None;
        }
        Some(Region {
            x: left as i32,
            y: top as i32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CaptureError {
    #[error("cannot connect to the display: {0}")]
    Connection(String),
    #[error("the screen cannot be captured: {0}")]
    Unsupported(String),
    #[error("screen capture failed: {0}")]
    Failed(String),
    #[error("the region {0:?} is outside the screen")]
    OutsideScreen(Region),
}

/// Takes screenshots of parts of the screen.
pub trait ScreenCapture {
    /// Size of the screen in pixels.
    fn screen_size(&self) -> (u32, u32);

    /// Captures `region`, clipped to the screen.
    fn capture(&mut self, region: Region) -> Result<RgbImage, CaptureError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(x: i32, y: i32, width: u32, height: u32) -> Region {
        Region {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_region_inside_the_screen_is_unchanged() {
        assert_eq!(
            region(10, 20, 300, 40).clamp_to(1920, 1080),
            Some(region(10, 20, 300, 40))
        );
    }

    #[test]
    fn a_region_touching_the_screen_edges_is_unchanged() {
        assert_eq!(
            region(0, 0, 1920, 1080).clamp_to(1920, 1080),
            Some(region(0, 0, 1920, 1080))
        );
    }

    #[test]
    fn a_region_sticking_out_is_cut_at_each_edge() {
        assert_eq!(
            region(-50, 10, 200, 40).clamp_to(1920, 1080),
            Some(region(0, 10, 150, 40))
        );
        assert_eq!(
            region(10, -5, 200, 40).clamp_to(1920, 1080),
            Some(region(10, 0, 200, 35))
        );
        assert_eq!(
            region(1800, 10, 200, 40).clamp_to(1920, 1080),
            Some(region(1800, 10, 120, 40))
        );
        assert_eq!(
            region(10, 1060, 200, 40).clamp_to(1920, 1080),
            Some(region(10, 1060, 200, 20))
        );
    }

    #[test]
    fn a_region_larger_than_the_screen_becomes_the_whole_screen() {
        assert_eq!(
            region(-10, -10, 5000, 5000).clamp_to(800, 600),
            Some(region(0, 0, 800, 600))
        );
    }

    #[test]
    fn regions_outside_the_screen_or_empty_have_no_visible_part() {
        assert_eq!(region(1920, 0, 10, 10).clamp_to(1920, 1080), None);
        assert_eq!(region(0, 1080, 10, 10).clamp_to(1920, 1080), None);
        assert_eq!(region(-100, 0, 100, 10).clamp_to(1920, 1080), None);
        assert_eq!(region(0, -10, 10, 10).clamp_to(1920, 1080), None);
        assert_eq!(region(10, 10, 0, 10).clamp_to(1920, 1080), None);
        assert_eq!(region(10, 10, 10, 0).clamp_to(1920, 1080), None);
    }

    #[test]
    fn huge_regions_do_not_overflow() {
        assert_eq!(
            region(i32::MAX, i32::MAX, u32::MAX, u32::MAX).clamp_to(1920, 1080),
            None
        );
        assert_eq!(
            region(i32::MIN, i32::MIN, u32::MAX, u32::MAX).clamp_to(1920, 1080),
            Some(region(0, 0, 1920, 1080))
        );
    }

    #[test]
    fn region_serializes_with_camel_case_keys() {
        let value = serde_json::to_value(region(1, 2, 3, 4)).expect("serialize");

        assert_eq!(
            value,
            serde_json::json!({ "x": 1, "y": 2, "width": 3, "height": 4 })
        );
    }

    #[test]
    fn errors_have_descriptive_messages() {
        assert_eq!(
            CaptureError::Connection("no display".to_string()).to_string(),
            "cannot connect to the display: no display"
        );
        assert_eq!(
            CaptureError::Unsupported("8 bit".to_string()).to_string(),
            "the screen cannot be captured: 8 bit"
        );
        assert_eq!(
            CaptureError::Failed("bad match".to_string()).to_string(),
            "screen capture failed: bad match"
        );
        assert_eq!(
            CaptureError::OutsideScreen(region(1, 2, 3, 4)).to_string(),
            "the region Region { x: 1, y: 2, width: 3, height: 4 } is outside the screen"
        );
    }
}
