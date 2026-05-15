// Detect bordered tables in a sliced ANSI string and group lines into
// Text / Table blocks for the renderer.
//
// Scope: only "bordered" tables — those with explicit Unicode/ASCII box-drawing
// characters along the top, sides, and bottom. Space-aligned columns are NOT
// detected (too brittle).

use crate::ansi::utf8_step;

#[derive(Debug, PartialEq)]
pub enum Block {
    Text(String),
    Table(Table),
}

#[derive(Debug, PartialEq)]
pub struct Table {
    /// Visible-column widths of each cell (excluding the vertical-bar columns).
    pub col_widths: Vec<usize>,
    /// Each row's cells, raw ANSI text per cell (without surrounding `│`).
    pub rows: Vec<Vec<String>>,
}

const TOP_LEFT: &[char] = &['┌', '╭', '╔', '┏', '+'];
const TOP_RIGHT: &[char] = &['┐', '╮', '╗', '┓', '+'];
const BOT_LEFT: &[char] = &['└', '╰', '╚', '┗', '+'];
const BOT_RIGHT: &[char] = &['┘', '╯', '╝', '┛', '+'];
const HORIZ: &[char] = &['─', '━', '═', '-'];
const T_DOWN: &[char] = &['┬', '┳', '╦', '+']; // ┬ on top border
const T_UP: &[char] = &['┴', '┻', '╩', '+']; // ┴ on bottom border
const T_RIGHT: &[char] = &['├', '┣', '╠', '+']; // left side of inner separator
const T_LEFT: &[char] = &['┤', '┫', '╣', '+']; // right side of inner separator
const CROSS: &[char] = &['┼', '╋', '╬', '+'];
const VERT: &[char] = &['│', '┃', '║', '|'];

/// Walk a line stripping ANSI escapes, returning Vec<(visible_column, char)>.
fn visible_chars(line: &str) -> Vec<(usize, char)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut col = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            i += 2;
            while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b']' {
            i += 2;
            while i < bytes.len() {
                if bytes[i] == 0x07 {
                    i += 1;
                    break;
                }
                if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if bytes[i] == 0x1b {
            i += 1;
            continue;
        }
        let step = utf8_step(bytes[i]);
        let end = (i + step).min(bytes.len());
        if let Ok(s) = std::str::from_utf8(&bytes[i..end]) {
            if let Some(c) = s.chars().next() {
                out.push((col, c));
            }
        }
        i = end;
        col += 1;
    }
    out
}

/// Visible columns of every character in the line, paired with the BYTE
/// offsets so we can later slice the original string (preserving ANSI).
fn visible_chars_with_bytes(line: &str) -> Vec<(usize, char, usize, usize)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut col = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            i += 2;
            while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b']' {
            i += 2;
            while i < bytes.len() {
                if bytes[i] == 0x07 {
                    i += 1;
                    break;
                }
                if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if bytes[i] == 0x1b {
            i += 1;
            continue;
        }
        let step = utf8_step(bytes[i]);
        let end = (i + step).min(bytes.len());
        if let Ok(s) = std::str::from_utf8(&bytes[i..end]) {
            if let Some(c) = s.chars().next() {
                out.push((col, c, i, end));
            }
        }
        i = end;
        col += 1;
    }
    out
}

fn first_visible(line: &str) -> Option<char> {
    visible_chars(line).into_iter().next().map(|(_, c)| c)
}

/// Top border: starts with TOP_LEFT, contains only horizontals + T_DOWN, ends with TOP_RIGHT.
fn is_top_border(line: &str) -> bool {
    let chars = visible_chars(line);
    let trimmed: Vec<char> = chars.iter().map(|(_, c)| *c).filter(|c| !c.is_whitespace()).collect();
    if trimmed.len() < 2 {
        return false;
    }
    if !TOP_LEFT.contains(&trimmed[0]) || !TOP_RIGHT.contains(&trimmed[trimmed.len() - 1]) {
        return false;
    }
    trimmed[1..trimmed.len() - 1]
        .iter()
        .all(|c| HORIZ.contains(c) || T_DOWN.contains(c))
}

fn is_bottom_border(line: &str) -> bool {
    let chars = visible_chars(line);
    let trimmed: Vec<char> = chars.iter().map(|(_, c)| *c).filter(|c| !c.is_whitespace()).collect();
    if trimmed.len() < 2 {
        return false;
    }
    if !BOT_LEFT.contains(&trimmed[0]) || !BOT_RIGHT.contains(&trimmed[trimmed.len() - 1]) {
        return false;
    }
    trimmed[1..trimmed.len() - 1]
        .iter()
        .all(|c| HORIZ.contains(c) || T_UP.contains(c))
}

/// Inner separator: `├───┼───┤` style. Skipped (purely decorative).
fn is_inner_separator(line: &str) -> bool {
    let chars = visible_chars(line);
    let trimmed: Vec<char> = chars.iter().map(|(_, c)| *c).filter(|c| !c.is_whitespace()).collect();
    if trimmed.len() < 2 {
        return false;
    }
    if !T_RIGHT.contains(&trimmed[0]) || !T_LEFT.contains(&trimmed[trimmed.len() - 1]) {
        return false;
    }
    trimmed[1..trimmed.len() - 1]
        .iter()
        .all(|c| HORIZ.contains(c) || CROSS.contains(c))
}

/// Visible column positions of vertical-bar boundaries on a border line:
/// TOP_LEFT + T_DOWN + TOP_RIGHT (or analogous bottom set).
fn boundary_cols(line: &str) -> Vec<usize> {
    visible_chars(line)
        .into_iter()
        .filter(|(_, c)| {
            TOP_LEFT.contains(c)
                || TOP_RIGHT.contains(c)
                || T_DOWN.contains(c)
                || BOT_LEFT.contains(c)
                || BOT_RIGHT.contains(c)
                || T_UP.contains(c)
        })
        .map(|(col, _)| col)
        .collect()
}

/// Given a content row's raw text and the column positions of its `│`
/// boundaries (from the top border), extract each cell's raw ANSI text.
fn split_row_cells(line: &str, boundaries: &[usize]) -> Option<Vec<String>> {
    if boundaries.len() < 2 {
        return None;
    }
    let chars = visible_chars_with_bytes(line);
    // Index by visible column position.
    let mut col_to_byte_start: std::collections::HashMap<usize, usize> = Default::default();
    let mut col_to_byte_end: std::collections::HashMap<usize, usize> = Default::default();
    for (col, _, s, e) in &chars {
        col_to_byte_start.insert(*col, *s);
        col_to_byte_end.insert(*col, *e);
    }
    let mut cells = Vec::new();
    let bytes = line.as_bytes();
    for w in boundaries.windows(2) {
        let left = w[0];
        let right = w[1];
        // cell content occupies visible cols (left+1 .. right-1) inclusive
        let content_start = left + 1;
        let content_end = right.saturating_sub(1);
        // find the first char at or after content_start and the last char at or before content_end
        let mut start_byte = None;
        let mut end_byte = None;
        for (col, _, s, e) in &chars {
            if *col >= content_start && *col <= content_end {
                if start_byte.is_none() {
                    start_byte = Some(*s);
                }
                end_byte = Some(*e);
            }
        }
        let cell_text = match (start_byte, end_byte) {
            (Some(s), Some(e)) => String::from_utf8_lossy(&bytes[s..e]).into_owned(),
            _ => String::new(),
        };
        cells.push(cell_text);
    }
    Some(cells)
}

/// Visible-column widths between each pair of boundaries (= cell content widths).
fn col_widths_from_boundaries(boundaries: &[usize]) -> Vec<usize> {
    boundaries
        .windows(2)
        .map(|w| w[1].saturating_sub(w[0]).saturating_sub(1))
        .collect()
}

/// True if the line is a "content row": starts with a vertical bar and has at
/// least one more vertical bar somewhere after.
fn is_content_row(line: &str) -> bool {
    let mut seen_vert = 0;
    for (_, c) in visible_chars(line) {
        if VERT.contains(&c) {
            seen_vert += 1;
            if seen_vert >= 2 {
                return true;
            }
        }
    }
    false
}

pub fn parse_blocks(sliced: &str) -> Vec<Block> {
    let lines: Vec<&str> = sliced.split('\n').collect();
    let mut blocks: Vec<Block> = Vec::new();
    let mut text_buf: Vec<&str> = Vec::new();

    let flush_text = |text_buf: &mut Vec<&str>, blocks: &mut Vec<Block>| {
        if !text_buf.is_empty() {
            blocks.push(Block::Text(text_buf.join("\n")));
            text_buf.clear();
        }
    };

    let mut i = 0;
    while i < lines.len() {
        if is_top_border(lines[i]) {
            // Search for the matching bottom border.
            let mut j = i + 1;
            while j < lines.len() && !is_bottom_border(lines[j]) {
                // bail out if a non-table line appears (no `│` at all)
                if !is_content_row(lines[j]) && !is_inner_separator(lines[j]) {
                    break;
                }
                j += 1;
            }
            if j < lines.len() && is_bottom_border(lines[j]) {
                let boundaries = boundary_cols(lines[i]);
                let col_widths = col_widths_from_boundaries(&boundaries);
                let mut rows: Vec<Vec<String>> = Vec::new();
                for r in (i + 1)..j {
                    if is_inner_separator(lines[r]) {
                        continue;
                    }
                    if let Some(cells) = split_row_cells(lines[r], &boundaries) {
                        rows.push(cells);
                    }
                }
                flush_text(&mut text_buf, &mut blocks);
                blocks.push(Block::Table(Table { col_widths, rows }));
                i = j + 1;
                continue;
            }
        }
        text_buf.push(lines[i]);
        i += 1;
    }
    flush_text(&mut text_buf, &mut blocks);
    blocks
}

#[allow(dead_code)]
pub fn _line_starts_with_box(line: &str) -> bool {
    first_visible(line)
        .map(|c| {
            TOP_LEFT.contains(&c)
                || TOP_RIGHT.contains(&c)
                || BOT_LEFT.contains(&c)
                || BOT_RIGHT.contains(&c)
                || HORIZ.contains(&c)
                || VERT.contains(&c)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_top_and_bottom_border() {
        assert!(is_top_border("┌───┬───┐"));
        assert!(is_bottom_border("└───┴───┘"));
        assert!(!is_top_border("hello"));
        assert!(!is_top_border("│ a │ b │"));
    }

    #[test]
    fn detects_inner_separator() {
        assert!(is_inner_separator("├───┼───┤"));
        assert!(!is_inner_separator("┌───┬───┐"));
    }

    #[test]
    fn boundaries_from_top_border() {
        let cols = boundary_cols("┌──┬─┐");
        assert_eq!(cols, vec![0, 3, 5]);
    }

    #[test]
    fn parses_simple_table() {
        let input = "\
┌────┬────┐
│ A  │ B  │
├────┼────┤
│ 1  │ 22 │
└────┴────┘";
        let blocks = parse_blocks(input);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            Block::Table(t) => {
                assert_eq!(t.col_widths, vec![4, 4]);
                assert_eq!(t.rows.len(), 2);
                assert_eq!(t.rows[0][0].trim(), "A");
                assert_eq!(t.rows[0][1].trim(), "B");
                assert_eq!(t.rows[1][0].trim(), "1");
                assert_eq!(t.rows[1][1].trim(), "22");
            }
            _ => panic!("expected table"),
        }
    }

    #[test]
    fn parses_mixed_text_then_table() {
        let input = "\
prefix line
┌──┐
│ X│
└──┘
trailing line";
        let blocks = parse_blocks(input);
        assert_eq!(blocks.len(), 3);
        match &blocks[0] {
            Block::Text(t) => assert_eq!(t, "prefix line"),
            _ => panic!(),
        }
        assert!(matches!(blocks[1], Block::Table(_)));
        match &blocks[2] {
            Block::Text(t) => assert_eq!(t, "trailing line"),
            _ => panic!(),
        }
    }

    #[test]
    fn ascii_plus_dash_table_also_detected() {
        let input = "\
+---+---+
| a | b |
+---+---+
| c | d |
+---+---+";
        let blocks = parse_blocks(input);
        assert!(matches!(blocks[0], Block::Table(_)));
    }

    #[test]
    fn non_table_stays_text() {
        let input = "just\nplain\ntext";
        let blocks = parse_blocks(input);
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], Block::Text(_)));
    }
}
