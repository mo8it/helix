use std::borrow::Cow;

use crate::{
    chars::char_is_line_ending,
    graphemes::{ensure_grapheme_boundary_prev, grapheme_width, RopeGraphemes},
    line_ending::line_end_char_index,
    RopeSlice,
};

/// Represents a single point in a text buffer. Zero indexed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub row: usize,
    pub col: usize,
}

impl Position {
    pub const fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }

    pub const fn is_zero(self) -> bool {
        self.row == 0 && self.col == 0
    }

    // TODO: generalize
    pub fn traverse(self, text: &crate::Tendril) -> Self {
        let Self { mut row, mut col } = self;
        // TODO: there should be a better way here
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if char_is_line_ending(ch) && !(ch == '\r' && chars.peek() == Some(&'\n')) {
                row += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        Self { row, col }
    }
}

impl From<(usize, usize)> for Position {
    fn from(tuple: (usize, usize)) -> Self {
        Self {
            row: tuple.0,
            col: tuple.1,
        }
    }
}

impl From<Position> for tree_sitter::Point {
    fn from(pos: Position) -> Self {
        Self::new(pos.row, pos.col)
    }
}
/// Convert a character index to (line, column) coordinates.
///
/// TODO: this should be split into two methods: one for visual
/// row/column, and one for "objective" row/column (possibly with
/// the column specified in `char`s).  The former would be used
/// for cursor movement, and the latter would be used for e.g. the
/// row:column display in the status line.
pub fn coords_at_pos(text: RopeSlice, pos: usize) -> Position {
    let line = text.char_to_line(pos);

    let line_start = text.line_to_char(line);
    let pos = ensure_grapheme_boundary_prev(text, pos);
    let col = RopeGraphemes::new(text.slice(line_start..pos))
        .map(|g| {
            let g: Cow<str> = g.into();
            grapheme_width(&g)
        })
        .sum();

    Position::new(line, col)
}

/// Convert (line, column) coordinates to a character index.
///
/// `is_1_width` specifies whether the position should be treated
/// as a block cursor or not.  This effects how line-ends are handled.
/// `false` corresponds to properly round-tripping with `coords_at_pos()`,
/// whereas `true` will ensure that block cursors don't jump off the
/// end of the line.
pub fn pos_at_coords(text: RopeSlice, coords: Position, is_1_width: bool) -> usize {
    let Position { row, col } = coords;
    let line_start = text.line_to_char(row);
    let line_end = if is_1_width {
        line_end_char_index(&text, row)
    } else {
        text.line_to_char((row + 1).min(text.len_lines()))
    };

    let mut prev_col = 0;
    let mut col_char_offset = 0;
    for g in RopeGraphemes::new(text.slice(line_start..line_end)) {
        let g: Cow<str> = g.into();
        let next_col = prev_col + grapheme_width(&g);

        if next_col > col {
            break;
        }

        prev_col = next_col;
        col_char_offset += g.chars().count();
    }

    line_start + col_char_offset
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Rope;

    #[test]
    fn test_ordering() {
        // (0, 5) is less than (1, 0)
        assert!(Position::new(0, 5) < Position::new(1, 0));
    }

    #[test]
    fn test_coords_at_pos() {
        let text = Rope::from("ḧëḷḷö\nẅöṛḷḋ");
        let slice = text.slice(..);
        assert_eq!(coords_at_pos(slice, 0), (0, 0).into());
        assert_eq!(coords_at_pos(slice, 5), (0, 5).into()); // position on \n
        assert_eq!(coords_at_pos(slice, 6), (1, 0).into()); // position on w
        assert_eq!(coords_at_pos(slice, 7), (1, 1).into()); // position on o
        assert_eq!(coords_at_pos(slice, 10), (1, 4).into()); // position on d

        // Test with wide characters.
        let text = Rope::from("今日はいい\n");
        let slice = text.slice(..);
        assert_eq!(coords_at_pos(slice, 0), (0, 0).into());
        assert_eq!(coords_at_pos(slice, 1), (0, 2).into());
        assert_eq!(coords_at_pos(slice, 2), (0, 4).into());
        assert_eq!(coords_at_pos(slice, 3), (0, 6).into());
        assert_eq!(coords_at_pos(slice, 4), (0, 8).into());
        assert_eq!(coords_at_pos(slice, 5), (0, 10).into());
        assert_eq!(coords_at_pos(slice, 6), (1, 0).into());

        // test with grapheme clusters
        let text = Rope::from("a̐éö̲\r\n");
        let slice = text.slice(..);
        assert_eq!(coords_at_pos(slice, 0), (0, 0).into());
        assert_eq!(coords_at_pos(slice, 2), (0, 1).into());
        assert_eq!(coords_at_pos(slice, 4), (0, 2).into());
        assert_eq!(coords_at_pos(slice, 7), (0, 3).into());
        assert_eq!(coords_at_pos(slice, 9), (1, 0).into());

        let text = Rope::from("किमपि\n");
        let slice = text.slice(..);
        assert_eq!(coords_at_pos(slice, 0), (0, 0).into());
        assert_eq!(coords_at_pos(slice, 2), (0, 2).into());
        assert_eq!(coords_at_pos(slice, 3), (0, 3).into());
        assert_eq!(coords_at_pos(slice, 5), (0, 5).into());
        assert_eq!(coords_at_pos(slice, 6), (1, 0).into());
    }

    #[test]
    fn test_pos_at_coords() {
        let text = Rope::from("ḧëḷḷö\nẅöṛḷḋ");
        let slice = text.slice(..);
        assert_eq!(pos_at_coords(slice, (0, 0).into(), false), 0);
        assert_eq!(pos_at_coords(slice, (0, 5).into(), false), 5); // position on \n
        assert_eq!(pos_at_coords(slice, (0, 6).into(), false), 6); // position after \n
        assert_eq!(pos_at_coords(slice, (0, 6).into(), true), 5); // position after \n
        assert_eq!(pos_at_coords(slice, (1, 0).into(), false), 6); // position on w
        assert_eq!(pos_at_coords(slice, (1, 1).into(), false), 7); // position on o
        assert_eq!(pos_at_coords(slice, (1, 4).into(), false), 10); // position on d

        // Test with wide characters.
        let text = Rope::from("今日はいい\n");
        let slice = text.slice(..);
        assert_eq!(pos_at_coords(slice, (0, 0).into(), false), 0);
        assert_eq!(pos_at_coords(slice, (0, 1).into(), false), 0);
        assert_eq!(pos_at_coords(slice, (0, 2).into(), false), 1);
        assert_eq!(pos_at_coords(slice, (0, 3).into(), false), 1);
        assert_eq!(pos_at_coords(slice, (0, 4).into(), false), 2);
        assert_eq!(pos_at_coords(slice, (0, 6).into(), false), 3);
        assert_eq!(pos_at_coords(slice, (0, 8).into(), false), 4);
        assert_eq!(pos_at_coords(slice, (0, 10).into(), false), 5);
        assert_eq!(pos_at_coords(slice, (0, 11).into(), false), 6);
        assert_eq!(pos_at_coords(slice, (0, 11).into(), true), 5);
        assert_eq!(pos_at_coords(slice, (1, 0).into(), false), 6);

        // test with grapheme clusters
        let text = Rope::from("a̐éö̲\r\n");
        let slice = text.slice(..);
        assert_eq!(pos_at_coords(slice, (0, 0).into(), false), 0);
        assert_eq!(pos_at_coords(slice, (0, 1).into(), false), 2);
        assert_eq!(pos_at_coords(slice, (0, 2).into(), false), 4);
        assert_eq!(pos_at_coords(slice, (0, 3).into(), false), 7); // \r\n is one char here
        assert_eq!(pos_at_coords(slice, (0, 4).into(), false), 9);
        assert_eq!(pos_at_coords(slice, (0, 4).into(), true), 7);
        assert_eq!(pos_at_coords(slice, (1, 0).into(), false), 9);

        let text = Rope::from("किमपि");
        // 2 - 1 - 2 codepoints
        // TODO: delete handling as per https://news.ycombinator.com/item?id=20058454
        let slice = text.slice(..);
        assert_eq!(pos_at_coords(slice, (0, 0).into(), false), 0);
        assert_eq!(pos_at_coords(slice, (0, 1).into(), false), 0);
        assert_eq!(pos_at_coords(slice, (0, 2).into(), false), 2);
        assert_eq!(pos_at_coords(slice, (0, 3).into(), false), 3);
        assert_eq!(pos_at_coords(slice, (0, 4).into(), false), 3);
        assert_eq!(pos_at_coords(slice, (0, 5).into(), false), 5); // eol
    }
}
