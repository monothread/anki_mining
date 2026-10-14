//! Which detected lines belong to the same block of text (a two-line subtitle, a paragraph).

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

/// Whether two line boxes (`[x1, y1, x2, y2]`) are stacked lines of the same block: similar
/// height, close one above the other and overlapping horizontally.
pub fn same_block(a: [i32; 4], b: [i32; 4]) -> bool {
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

/// The indices of the block that contains `boxes[index]`, from top to bottom: the line itself
/// plus its stacked neighbours, at most [`MAX_BLOCK_LINES`] lines (one above and one below first).
pub fn block_around(boxes: &[[i32; 4]], index: usize) -> Vec<usize> {
    let mut block = vec![index];
    while block.len() < MAX_BLOCK_LINES {
        let before = block.len();
        let top = block[0];
        if let Some(above) = neighbour(boxes, top, &block, Side::Above) {
            block.insert(0, above);
        }
        if block.len() >= MAX_BLOCK_LINES {
            break;
        }
        let bottom = block[block.len() - 1];
        if let Some(below) = neighbour(boxes, bottom, &block, Side::Below) {
            block.push(below);
        }
        if block.len() == before {
            break;
        }
    }
    block
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Above,
    Below,
}

/// The nearest line of the same block above or below `boxes[from]` that is not in `block` yet.
fn neighbour(boxes: &[[i32; 4]], from: usize, block: &[usize], side: Side) -> Option<usize> {
    let from_center = boxes[from][1] + boxes[from][3];
    boxes
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            let center = candidate[1] + candidate[3];
            !block.contains(index)
                && same_block(boxes[from], **candidate)
                && match side {
                    Side::Above => center < from_center,
                    Side::Below => center > from_center,
                }
        })
        .min_by_key(|(_, candidate)| (candidate[1] + candidate[3] - from_center).abs())
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
}
