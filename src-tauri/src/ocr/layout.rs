//! Which detected lines belong to the same block of text (a two-line subtitle, a paragraph, a set
//! of vertical columns).

/// A block never has more lines than this: enough for a subtitle, short for a long paragraph.
pub const MAX_BLOCK_LINES: usize = 3;

/// Lines whose heights differ by more than this factor are different kinds of text.
const MAX_HEIGHT_RATIO: f32 = 1.6;
/// Stacked lines are at most this many line heights apart.
const MAX_GAP_HEIGHTS: f32 = 1.0;
/// Stacked lines may overlap vertically by at most this many line heights (otherwise they sit on
/// the same row side by side).
const MAX_OVERLAP_HEIGHTS: f32 = 0.3;
/// Stacked lines share at least this part of the shorter line's width.
const MIN_HORIZONTAL_OVERLAP: f32 = 0.3;

/// A box is a vertical column only when it is clearly taller than wide: this many fifths taller.
/// A single character is nearly square and is read as a normal line.
const VERTICAL_RATIO_NUMERATOR: i32 = 5;
const VERTICAL_RATIO_DENOMINATOR: i32 = 4;

/// Whether a box (`[x1, y1, x2, y2]`) is a vertical column of text.
pub fn is_vertical(bbox: [i32; 4]) -> bool {
    let (width, height) = (bbox[2] - bbox[0], bbox[3] - bbox[1]);
    height * VERTICAL_RATIO_DENOMINATOR > width * VERTICAL_RATIO_NUMERATOR
}

/// The box with its axes swapped, so vertical columns can be compared like stacked lines.
fn transposed(bbox: [i32; 4]) -> [i32; 4] {
    [bbox[1], bbox[0], bbox[3], bbox[2]]
}

/// Whether two boxes (`[x1, y1, x2, y2]`) belong to the same block: stacked lines (similar
/// height, close one above the other, overlapping horizontally) or neighbouring vertical columns
/// (similar width, close side by side, overlapping vertically). A line and a column never do.
pub fn same_block(a: [i32; 4], b: [i32; 4]) -> bool {
    match (is_vertical(a), is_vertical(b)) {
        (false, false) => same_stacked_block(a, b),
        (true, true) => same_stacked_block(transposed(a), transposed(b)),
        _ => false,
    }
}

fn same_stacked_block(a: [i32; 4], b: [i32; 4]) -> bool {
    let (height_a, height_b) = (a[3] - a[1], b[3] - b[1]);
    let (width_a, width_b) = (a[2] - a[0], b[2] - b[0]);
    if height_a <= 0 || height_b <= 0 || width_a <= 0 || width_b <= 0 {
        return false;
    }
    let (shorter, taller) = (height_a.min(height_b) as f32, height_a.max(height_b) as f32);
    if taller > shorter * MAX_HEIGHT_RATIO {
        return false;
    }

    let gap = if a[1] <= b[1] {
        b[1] - a[3]
    } else {
        a[1] - b[3]
    } as f32;
    if gap > shorter * MAX_GAP_HEIGHTS || gap < -shorter * MAX_OVERLAP_HEIGHTS {
        return false;
    }

    let overlap = (a[2].min(b[2]) - a[0].max(b[0])) as f32;
    overlap >= width_a.min(width_b) as f32 * MIN_HORIZONTAL_OVERLAP
}

/// The indices of the block that contains `boxes[index]`, in reading order: lines from top to
/// bottom, vertical columns from right to left. The line itself plus its neighbours, at most
/// [`MAX_BLOCK_LINES`] (one on each side first).
pub fn block_around(boxes: &[[i32; 4]], index: usize) -> Vec<usize> {
    let vertical = is_vertical(boxes[index]);
    let mut block = vec![index];
    while block.len() < MAX_BLOCK_LINES {
        let before = block.len();
        let first = block[0];
        if let Some(previous) = neighbour(boxes, first, &block, Side::Before, vertical) {
            block.insert(0, previous);
        }
        if block.len() >= MAX_BLOCK_LINES {
            break;
        }
        let last = block[block.len() - 1];
        if let Some(next) = neighbour(boxes, last, &block, Side::After, vertical) {
            block.push(next);
        }
        if block.len() == before {
            break;
        }
    }
    block
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Before,
    After,
}

/// Position of a box along the reading direction of its block: the vertical center for lines,
/// the mirrored horizontal center for columns (they are read from right to left).
fn reading_position(bbox: [i32; 4], vertical: bool) -> i32 {
    if vertical {
        -(bbox[0] + bbox[2])
    } else {
        bbox[1] + bbox[3]
    }
}

/// The nearest box of the same block before or after `boxes[from]` in reading order that is not
/// in `block` yet.
fn neighbour(
    boxes: &[[i32; 4]],
    from: usize,
    block: &[usize],
    side: Side,
    vertical: bool,
) -> Option<usize> {
    let from_position = reading_position(boxes[from], vertical);
    boxes
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            let position = reading_position(**candidate, vertical);
            !block.contains(index)
                && same_block(boxes[from], **candidate)
                && match side {
                    Side::Before => position < from_position,
                    Side::After => position > from_position,
                }
        })
        .min_by_key(|(_, candidate)| {
            (reading_position(**candidate, vertical) - from_position).abs()
        })
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line box of the given height starting at (x, y) and `width` wide.
    fn line(x: i32, y: i32, width: i32, height: i32) -> [i32; 4] {
        [x, y, x + width, y + height]
    }

    #[test]
    fn two_centered_subtitle_lines_are_one_block() {
        // Heights 50, 20 px apart, the second line shorter and centered under the first.
        assert!(same_block(
            line(400, 900, 1000, 50),
            line(550, 970, 700, 50)
        ));
        assert!(same_block(
            line(550, 970, 700, 50),
            line(400, 900, 1000, 50)
        ));
    }

    #[test]
    fn lines_with_normal_paragraph_spacing_are_one_block() {
        // A 60 px step between 16 px lines is a blank line: another paragraph.
        assert!(!same_block(
            line(300, 534, 800, 16),
            line(300, 594, 1000, 16)
        ));
        assert!(same_block(
            line(300, 534, 800, 16),
            line(300, 556, 1000, 16)
        ));
    }

    #[test]
    fn lines_far_apart_vertically_are_different_blocks() {
        assert!(!same_block(
            line(400, 900, 1000, 50),
            line(400, 1010, 1000, 50)
        ));
    }

    #[test]
    fn lines_side_by_side_on_the_same_row_are_different_blocks() {
        assert!(!same_block(
            line(100, 900, 400, 50),
            line(700, 900, 400, 50)
        ));
        // Even when they overlap horizontally a little, a line on the same row is not stacked.
        assert!(!same_block(
            line(100, 900, 400, 50),
            line(450, 905, 400, 50)
        ));
    }

    #[test]
    fn lines_in_different_columns_are_different_blocks() {
        assert!(!same_block(
            line(100, 900, 300, 50),
            line(900, 960, 300, 50)
        ));
    }

    #[test]
    fn lines_with_a_barely_shared_width_are_different_blocks() {
        // Overlap of 20 px out of a 400 px line is 5%.
        assert!(!same_block(
            line(100, 900, 400, 50),
            line(480, 960, 400, 50)
        ));
        // Overlap of 160 px out of 400 px is 40%.
        assert!(same_block(line(100, 900, 400, 50), line(340, 960, 400, 50)));
    }

    #[test]
    fn lines_of_very_different_heights_are_different_blocks() {
        // A big title above small text.
        assert!(!same_block(
            line(300, 440, 120, 38),
            line(300, 490, 800, 16)
        ));
        // A 1.5 times taller line still counts.
        assert!(same_block(line(300, 440, 800, 24), line(300, 470, 800, 16)));
    }

    #[test]
    fn degenerate_boxes_never_match() {
        assert!(!same_block([10, 10, 10, 50], line(10, 60, 100, 40)));
        assert!(!same_block(line(10, 10, 100, 40), [10, 60, 110, 60]));
    }

    #[test]
    fn the_block_of_a_single_line_is_the_line() {
        assert_eq!(block_around(&[line(400, 900, 1000, 50)], 0), vec![0]);
    }

    #[test]
    fn the_block_lists_lines_top_to_bottom_whatever_the_detection_order() {
        let boxes = [line(550, 970, 700, 50), line(400, 900, 1000, 50)];

        assert_eq!(block_around(&boxes, 0), vec![1, 0]);
        assert_eq!(block_around(&boxes, 1), vec![1, 0]);
    }

    #[test]
    fn an_unrelated_line_elsewhere_is_not_part_of_the_block() {
        let boxes = [
            line(400, 900, 1000, 50),
            line(550, 970, 700, 50),
            line(100, 100, 300, 50),
        ];

        assert_eq!(block_around(&boxes, 0), vec![0, 1]);
    }

    #[test]
    fn a_paragraph_is_cut_to_the_hovered_line_and_one_neighbour_on_each_side() {
        let boxes: Vec<[i32; 4]> = (0..6)
            .map(|row| line(300, 500 + row * 22, 900, 16))
            .collect();

        assert_eq!(block_around(&boxes, 2), vec![1, 2, 3]);
        assert_eq!(block_around(&boxes, 3), vec![2, 3, 4]);
    }

    #[test]
    fn at_the_edges_of_a_paragraph_the_block_takes_two_lines_from_the_other_side() {
        let boxes: Vec<[i32; 4]> = (0..6)
            .map(|row| line(300, 500 + row * 22, 900, 16))
            .collect();

        assert_eq!(block_around(&boxes, 0), vec![0, 1, 2]);
        assert_eq!(block_around(&boxes, 5), vec![3, 4, 5]);
    }

    #[test]
    fn the_block_never_exceeds_the_maximum() {
        let boxes: Vec<[i32; 4]> = (0..10)
            .map(|row| line(300, 500 + row * 22, 900, 16))
            .collect();

        for index in 0..boxes.len() {
            assert!(block_around(&boxes, index).len() <= MAX_BLOCK_LINES);
        }
    }

    #[test]
    fn the_nearest_candidate_is_chosen_when_several_lines_are_stacked() {
        // Two candidates below; the closer one wins.
        let boxes = [
            line(400, 900, 1000, 50),
            line(400, 1000, 1000, 50),
            line(400, 960, 1000, 50),
        ];

        assert_eq!(block_around(&boxes, 0), vec![0, 2, 1]);
    }

    // --- vertical columns ---

    /// A column box of the given width starting at (x, y) and `height` tall.
    fn column(x: i32, y: i32, width: i32, height: i32) -> [i32; 4] {
        [x, y, x + width, y + height]
    }

    #[test]
    fn boxes_clearly_taller_than_wide_are_vertical() {
        assert!(is_vertical(column(100, 100, 40, 400)));
        assert!(is_vertical(column(100, 100, 40, 80)));
        assert!(!is_vertical(line(100, 100, 400, 40)));
    }

    #[test]
    fn nearly_square_boxes_such_as_a_single_character_are_not_vertical() {
        assert!(!is_vertical(line(100, 100, 40, 40)));
        assert!(!is_vertical(column(100, 100, 40, 50)));
        assert!(is_vertical(column(100, 100, 40, 51)));
    }

    #[test]
    fn neighbouring_columns_are_one_block() {
        // Width 40, 20 px apart, the second column shorter and starting lower.
        assert!(same_block(
            column(600, 100, 40, 500),
            column(540, 140, 40, 380)
        ));
        assert!(same_block(
            column(540, 140, 40, 380),
            column(600, 100, 40, 500)
        ));
    }

    #[test]
    fn columns_far_apart_are_different_blocks() {
        assert!(!same_block(
            column(600, 100, 40, 500),
            column(400, 100, 40, 500)
        ));
    }

    #[test]
    fn columns_at_different_heights_without_shared_extent_are_different_blocks() {
        assert!(!same_block(
            column(600, 100, 40, 300),
            column(540, 600, 40, 300)
        ));
    }

    #[test]
    fn a_line_and_a_column_never_share_a_block() {
        assert!(!same_block(
            line(100, 100, 400, 40),
            column(150, 160, 40, 400)
        ));
        assert!(!same_block(
            column(150, 160, 40, 400),
            line(100, 100, 400, 40)
        ));
    }

    #[test]
    fn columns_of_very_different_widths_are_different_blocks() {
        assert!(!same_block(
            column(600, 100, 80, 500),
            column(530, 100, 30, 500)
        ));
    }

    #[test]
    fn the_block_of_columns_is_listed_from_right_to_left() {
        let boxes = [
            column(300, 100, 40, 500),
            column(420, 100, 40, 500),
            column(360, 100, 40, 500),
        ];

        assert_eq!(block_around(&boxes, 0), vec![1, 2, 0]);
        assert_eq!(block_around(&boxes, 1), vec![1, 2, 0]);
    }

    #[test]
    fn a_vertical_block_never_exceeds_the_maximum_and_keeps_the_neighbours_on_both_sides() {
        let boxes: Vec<[i32; 4]> = (0..6).map(|n| column(800 - n * 60, 100, 40, 500)).collect();

        assert_eq!(block_around(&boxes, 2), vec![1, 2, 3]);
        assert_eq!(block_around(&boxes, 0), vec![0, 1, 2]);
        assert_eq!(block_around(&boxes, 5), vec![3, 4, 5]);
    }

    #[test]
    fn horizontal_lines_do_not_join_a_column_block() {
        let boxes = [column(600, 100, 40, 500), line(100, 650, 400, 40)];

        assert_eq!(block_around(&boxes, 0), vec![0]);
        assert_eq!(block_around(&boxes, 1), vec![1]);
    }
}
