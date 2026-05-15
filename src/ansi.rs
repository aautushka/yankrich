// ANSI parsing: tokenizer, column slicer, color tables.

#[derive(Default, Clone, PartialEq, Debug)]
pub struct Style {
    pub fg: Option<(u8, u8, u8)>,
    pub bg: Option<(u8, u8, u8)>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

#[derive(Debug, PartialEq)]
pub enum Token {
    Run(Style, String),
    Newline,
}

pub fn ansi16_rgb(code: u8) -> (u8, u8, u8) {
    match code {
        0 => (0, 0, 0),
        1 => (205, 0, 0),
        2 => (0, 205, 0),
        3 => (205, 205, 0),
        4 => (0, 0, 238),
        5 => (205, 0, 205),
        6 => (0, 205, 205),
        7 => (229, 229, 229),
        8 => (127, 127, 127),
        9 => (255, 0, 0),
        10 => (0, 255, 0),
        11 => (255, 255, 0),
        12 => (92, 92, 255),
        13 => (255, 0, 255),
        14 => (0, 255, 255),
        15 => (255, 255, 255),
        _ => (0, 0, 0),
    }
}

pub fn ansi256_rgb(idx: u8) -> (u8, u8, u8) {
    if idx < 16 {
        return ansi16_rgb(idx);
    }
    if idx >= 232 {
        let v = 8 + (idx - 232).saturating_mul(10);
        return (v, v, v);
    }
    let n = idx - 16;
    let r = n / 36;
    let g = (n % 36) / 6;
    let b = n % 6;
    let comp = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
    (comp(r), comp(g), comp(b))
}

pub fn apply_sgr(params: &[u32], state: &mut Style) {
    if params.is_empty() {
        *state = Style::default();
        return;
    }
    let mut i = 0;
    while i < params.len() {
        let p = params[i];
        match p {
            0 => *state = Style::default(),
            1 => state.bold = true,
            3 => state.italic = true,
            4 => state.underline = true,
            22 => state.bold = false,
            23 => state.italic = false,
            24 => state.underline = false,
            30..=37 => state.fg = Some(ansi16_rgb((p - 30) as u8)),
            39 => state.fg = None,
            40..=47 => state.bg = Some(ansi16_rgb((p - 40) as u8)),
            49 => state.bg = None,
            90..=97 => state.fg = Some(ansi16_rgb((p - 90 + 8) as u8)),
            100..=107 => state.bg = Some(ansi16_rgb((p - 100 + 8) as u8)),
            38 | 48 => {
                let is_fg = p == 38;
                if i + 1 < params.len() {
                    match params[i + 1] {
                        5 => {
                            if i + 2 < params.len() {
                                let c = ansi256_rgb(params[i + 2] as u8);
                                if is_fg {
                                    state.fg = Some(c);
                                } else {
                                    state.bg = Some(c);
                                }
                                i += 2;
                            }
                        }
                        2 => {
                            if i + 4 < params.len() {
                                let c = (
                                    params[i + 2] as u8,
                                    params[i + 3] as u8,
                                    params[i + 4] as u8,
                                );
                                if is_fg {
                                    state.fg = Some(c);
                                } else {
                                    state.bg = Some(c);
                                }
                                i += 4;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
}

pub fn utf8_step(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xc0 {
        1
    } else if b < 0xe0 {
        2
    } else if b < 0xf0 {
        3
    } else {
        4
    }
}

// Walk past an OSC sequence starting at `i` (where bytes[i..i+2] == ESC ]).
// Returns the index of the byte just past the terminator (BEL or ESC \).
fn skip_osc(bytes: &[u8], mut i: usize) -> usize {
    i += 2;
    while i < bytes.len() {
        if bytes[i] == 0x07 {
            return i + 1;
        }
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
            return i + 2;
        }
        i += 1;
    }
    i
}

pub fn tokenize(input: &str) -> Vec<Token> {
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut state = Style::default();
    let mut current = String::new();
    let mut runs: Vec<Token> = Vec::new();

    let flush = |runs: &mut Vec<Token>, state: &Style, current: &mut String| {
        if !current.is_empty() {
            runs.push(Token::Run(state.clone(), std::mem::take(current)));
        }
    };

    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let mut j = i + 2;
            while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                j += 1;
            }
            if j < bytes.len() {
                let final_byte = bytes[j];
                let params_str = std::str::from_utf8(&bytes[i + 2..j]).unwrap_or("");
                if final_byte == b'm' {
                    flush(&mut runs, &state, &mut current);
                    let params: Vec<u32> = if params_str.is_empty() {
                        vec![0]
                    } else {
                        params_str
                            .split(';')
                            .map(|s| s.parse().unwrap_or(0))
                            .collect()
                    };
                    apply_sgr(&params, &mut state);
                }
                i = j + 1;
            } else {
                i = bytes.len();
            }
            continue;
        }
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b']' {
            i = skip_osc(bytes, i);
            continue;
        }
        if bytes[i] == b'\n' {
            flush(&mut runs, &state, &mut current);
            runs.push(Token::Newline);
            i += 1;
            continue;
        }
        if bytes[i] == 0x1b {
            i += 1;
            continue;
        }
        let step = utf8_step(bytes[i]);
        let end = (i + step).min(bytes.len());
        if let Ok(s) = std::str::from_utf8(&bytes[i..end]) {
            current.push_str(s);
        }
        i = end;
    }
    if !current.is_empty() {
        runs.push(Token::Run(state, current));
    }
    runs
}

pub fn slice_line(line: &str, start: usize, end_inclusive: usize) -> String {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut col = 0usize;
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut pre_sgr: Vec<u8> = Vec::new();
    let mut entered = false;
    let mut had_sgr = false;

    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let esc_start = i;
            i += 2;
            while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            let esc = &bytes[esc_start..i];
            had_sgr = true;
            if entered {
                out.extend_from_slice(esc);
            } else {
                pre_sgr.extend_from_slice(esc);
            }
            continue;
        }
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b']' {
            i = skip_osc(bytes, i);
            continue;
        }
        if bytes[i] == 0x1b {
            i += 1;
            continue;
        }
        if col > end_inclusive {
            break;
        }
        let step = utf8_step(bytes[i]);
        if col >= start {
            if !entered {
                out.extend_from_slice(&pre_sgr);
                entered = true;
            }
            let end_byte = (i + step).min(bytes.len());
            out.extend_from_slice(&bytes[i..end_byte]);
        }
        i += step;
        col += 1;
    }

    if entered && had_sgr {
        out.extend_from_slice(b"\x1b[0m");
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn count_visible(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut count = 0;
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
            i = skip_osc(bytes, i);
            continue;
        }
        if bytes[i] == 0x1b {
            i += 1;
            continue;
        }
        let step = utf8_step(bytes[i]);
        i += step;
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi16_basic() {
        assert_eq!(ansi16_rgb(1), (205, 0, 0));
        assert_eq!(ansi16_rgb(9), (255, 0, 0));
        assert_eq!(ansi16_rgb(15), (255, 255, 255));
    }

    #[test]
    fn ansi256_cube() {
        assert_eq!(ansi256_rgb(0), (0, 0, 0));
        assert_eq!(ansi256_rgb(231), (255, 255, 255));
        assert_eq!(ansi256_rgb(232), (8, 8, 8));
        assert_eq!(ansi256_rgb(255), (238, 238, 238));
    }

    #[test]
    fn apply_sgr_reset() {
        let mut s = Style {
            bold: true,
            fg: Some((1, 2, 3)),
            ..Default::default()
        };
        apply_sgr(&[0], &mut s);
        assert_eq!(s, Style::default());
    }

    #[test]
    fn apply_sgr_truecolor() {
        let mut s = Style::default();
        apply_sgr(&[38, 2, 10, 20, 30], &mut s);
        assert_eq!(s.fg, Some((10, 20, 30)));
        apply_sgr(&[48, 2, 100, 110, 120], &mut s);
        assert_eq!(s.bg, Some((100, 110, 120)));
    }

    #[test]
    fn apply_sgr_256() {
        let mut s = Style::default();
        apply_sgr(&[38, 5, 196], &mut s);
        assert_eq!(s.fg, Some(ansi256_rgb(196)));
    }

    #[test]
    fn tokenize_plain() {
        let toks = tokenize("hello");
        assert_eq!(toks.len(), 1);
        match &toks[0] {
            Token::Run(style, text) => {
                assert_eq!(style, &Style::default());
                assert_eq!(text, "hello");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn tokenize_sgr_split() {
        let toks = tokenize("\x1b[31mred\x1b[0m plain");
        assert_eq!(toks.len(), 2);
        match &toks[0] {
            Token::Run(s, t) => {
                assert_eq!(s.fg, Some((205, 0, 0)));
                assert_eq!(t, "red");
            }
            _ => panic!(),
        }
        match &toks[1] {
            Token::Run(s, t) => {
                assert_eq!(s, &Style::default());
                assert_eq!(t, " plain");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn tokenize_strips_osc_hyperlink() {
        let input = "\x1b]8;;file:///x\x1b\\label\x1b]8;;\x1b\\";
        let toks = tokenize(input);
        assert_eq!(toks.len(), 1);
        match &toks[0] {
            Token::Run(_, t) => assert_eq!(t, "label"),
            _ => panic!(),
        }
    }

    #[test]
    fn tokenize_newlines() {
        let toks = tokenize("a\nb\n");
        let kinds: Vec<&'static str> = toks
            .iter()
            .map(|t| match t {
                Token::Run(..) => "run",
                Token::Newline => "nl",
            })
            .collect();
        assert_eq!(kinds, vec!["run", "nl", "run", "nl"]);
    }

    #[test]
    fn tokenize_unicode() {
        let toks = tokenize("✗ ➜");
        assert_eq!(toks.len(), 1);
        if let Token::Run(_, t) = &toks[0] {
            assert_eq!(t, "✗ ➜");
        }
    }

    #[test]
    fn slice_no_sgr() {
        let s = slice_line("abcdef", 1, 3);
        assert_eq!(s, "bcd");
    }

    #[test]
    fn slice_carries_sgr_into_range() {
        // SGR set before the slice range must be carried into the output
        let s = slice_line("\x1b[31mabcdef", 2, 4);
        // expect "cde" prefixed by the red SGR and followed by reset
        assert!(s.starts_with("\x1b[31m"));
        assert!(s.ends_with("\x1b[0m"));
        assert!(s.contains("cde"));
    }

    #[test]
    fn slice_unicode_char_counted_as_one() {
        let s = slice_line("✗ab", 1, 2);
        assert_eq!(s, "ab");
    }

    #[test]
    fn slice_strips_osc() {
        let s = slice_line("\x1b]8;;file:///x\x1b\\hello", 0, 4);
        assert_eq!(s, "hello");
    }

    #[test]
    fn count_visible_counts_non_escape_chars() {
        assert_eq!(count_visible("abc"), 3);
        assert_eq!(count_visible("\x1b[31mred\x1b[0m"), 3);
        assert_eq!(count_visible("✗ab"), 3);
        assert_eq!(count_visible("\x1b]8;;u\x1b\\hi\x1b]8;;\x1b\\"), 2);
    }
}
