//! Where the popup goes: next to the word, never on top of the subtitle line.

/// Gap (in pixels) between the subtitle line and the popup.
pub const GAP: i32 = 12;

/// Rectangle in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// The rectangle grown by `margin` on every side.
    pub fn grow(&self, margin: i32) -> Rect {
        Rect {
            x: self.x - margin,
            y: self.y - margin,
            width: self.width + 2 * margin,
            height: self.height + 2 * margin,
        }
    }
}

/// Places a `popup_width` x `popup_height` popup under the line, or above it when there is no
/// room below, with its left edge near the cursor and kept inside the screen.
pub fn popup_position(
    line_bbox: [i32; 4],
    cursor_x: i32,
    popup_width: i32,
    popup_height: i32,
    screen_width: i32,
    screen_height: i32,
) -> (i32, i32) {
    let below = line_bbox[3] + GAP;
    let above = line_bbox[1] - GAP - popup_height;
    let y = if below + popup_height <= screen_height {
        below
    } else if above >= 0 {
        above
    } else {
        // Neither side fits: use the larger space and let the popup scroll.
        below.min(screen_height - popup_height).max(0)
    };
    let x = (cursor_x - popup_width / 4).clamp(0, (screen_width - popup_width).max(0));
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (i32, i32) = (1920, 1080);

    fn position(line: [i32; 4], cursor_x: i32) -> (i32, i32) {
        popup_position(line, cursor_x, 440, 320, SCREEN.0, SCREEN.1)
    }

    #[test]
    fn the_popup_goes_under_the_line_when_there_is_room() {
        assert_eq!(position([400, 300, 900, 350], 600), (490, 362));
    }

    #[test]
    fn the_popup_goes_above_the_line_when_there_is_no_room_below() {
        // A subtitle near the bottom of the screen.
        assert_eq!(position([400, 960, 900, 1010], 600), (490, 960 - 12 - 320));
    }

    #[test]
    fn the_popup_is_kept_inside_the_screen_horizontally() {
        assert_eq!(position([0, 300, 100, 350], 5).0, 0);
        assert_eq!(position([1800, 300, 1900, 350], 1890).0, 1920 - 440);
    }

    #[test]
    fn the_popup_starts_to_the_left_of_the_cursor() {
        assert_eq!(position([400, 300, 900, 350], 800).0, 800 - 110);
    }

    #[test]
    fn the_popup_fits_exactly_at_the_bottom_edge() {
        // below = 1080 - 320 = 760 leaves the popup ending exactly at the bottom.
        let line = [400, 760 - GAP - 50, 900, 760 - GAP];

        assert_eq!(position(line, 600).1, 760);
    }

    #[test]
    fn when_neither_side_fits_the_popup_is_pushed_into_the_screen() {
        let tall_screen_line = [10, 100, 500, 800];

        let (_, y) = popup_position(tall_screen_line, 100, 440, 320, 1920, 900);

        assert!((0..=900 - 320).contains(&y));
    }

    #[test]
    fn a_popup_wider_than_the_screen_starts_at_zero() {
        assert_eq!(
            popup_position([0, 0, 100, 50], 50, 3000, 100, 800, 600).0,
            0
        );
    }

    #[test]
    fn rect_contains_includes_the_top_left_and_excludes_the_bottom_right() {
        let rect = Rect {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        };

        assert!(rect.contains(10, 20));
        assert!(rect.contains(109, 69));
        assert!(!rect.contains(110, 69));
        assert!(!rect.contains(109, 70));
        assert!(!rect.contains(9, 20));
    }

    #[test]
    fn grow_extends_every_side() {
        let rect = Rect {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        };

        assert_eq!(
            rect.grow(5),
            Rect {
                x: 5,
                y: 15,
                width: 110,
                height: 60
            }
        );
        assert!(rect.grow(5).contains(5, 15));
    }
}
