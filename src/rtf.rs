// RTF renderer.

use crate::ansi::{tokenize, Token};
use crate::blocks::{parse_blocks, Block, Table};

/// Twips per visible character cell at 12pt Menlo (approximate).
/// 12pt Menlo glyph is roughly 7.2pt wide → 144 twips per char.
const CHAR_TWIPS: usize = 144;

fn palette_index(palette: &mut Vec<(u8, u8, u8)>, rgb: (u8, u8, u8)) -> usize {
    if let Some(i) = palette.iter().position(|&c| c == rgb) {
        i + 1
    } else {
        palette.push(rgb);
        palette.len()
    }
}

fn emit_rtf_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            c if (c as u32) < 128 => out.push(c),
            c => {
                let cp = c as u32;
                if cp <= 0xFFFF {
                    let signed = if cp >= 0x8000 {
                        (cp as i32) - 0x10000
                    } else {
                        cp as i32
                    };
                    out.push_str(&format!("\\u{}?", signed));
                } else {
                    let v = cp - 0x10000;
                    let high = 0xD800 + (v >> 10);
                    let low = 0xDC00 + (v & 0x3FF);
                    let hi_signed = (high as i32) - 0x10000;
                    let lo_signed = (low as i32) - 0x10000;
                    out.push_str(&format!("\\u{}?\\u{}?", hi_signed, lo_signed));
                }
            }
        }
    }
}

/// Encode an (R,G,B) triple as Word's COLORREF (R | G<<8 | B<<16) for use
/// inside an `{\*\background\shp ...}` fill shape.
fn colorref(rgb: (u8, u8, u8)) -> u32 {
    (rgb.0 as u32) | ((rgb.1 as u32) << 8) | ((rgb.2 as u32) << 16)
}

fn background_shape(rgb: (u8, u8, u8)) -> String {
    format!(
        "{{\\*\\background {{\\shp{{\\*\\shpinst\\shpleft0\\shptop0\\shpright0\\shpbottom0\
         \\shpfhdr0\\shpbxmargin\\shpbymargin\\shpwr0\\shpwrk0\\shpfblwtxt1\\shpz0\\shplid1025\
         {{\\sp{{\\sn shapeType}}{{\\sv 1}}}}\
         {{\\sp{{\\sn fillColor}}{{\\sv {}}}}}\
         {{\\sp{{\\sn fFilled}}{{\\sv 1}}}}\
         {{\\sp{{\\sn fLine}}{{\\sv 0}}}}\
         {{\\sp{{\\sn fBackground}}{{\\sv 1}}}}}}}}}}",
        colorref(rgb)
    )
}

fn emit_run(
    style: &crate::ansi::Style,
    text: &str,
    default_bg: Option<(u8, u8, u8)>,
    default_fg: Option<(u8, u8, u8)>,
    palette: &mut Vec<(u8, u8, u8)>,
    body: &mut String,
) {
    body.push_str("{\\plain\\f0\\fs24 ");
    let fg = style.fg.or(default_fg);
    let bg = style.bg.or(default_bg);
    if let Some(rgb) = fg {
        let idx = palette_index(palette, rgb);
        body.push_str(&format!("\\cf{} ", idx));
    }
    if let Some(rgb) = bg {
        let idx = palette_index(palette, rgb);
        body.push_str(&format!("\\cb{} ", idx));
    }
    if style.bold {
        body.push_str("\\b ");
    }
    if style.italic {
        body.push_str("\\i ");
    }
    if style.underline {
        body.push_str("\\ul ");
    }
    emit_rtf_text(text, body);
    body.push('}');
}

fn emit_text_block(
    text: &str,
    default_bg: Option<(u8, u8, u8)>,
    default_fg: Option<(u8, u8, u8)>,
    palette: &mut Vec<(u8, u8, u8)>,
    body: &mut String,
) {
    for tok in tokenize(text) {
        match tok {
            Token::Newline => body.push_str("\\par\n"),
            Token::Run(style, text) => emit_run(&style, &text, default_bg, default_fg, palette, body),
        }
    }
}

fn emit_table_block(
    table: &Table,
    default_bg: Option<(u8, u8, u8)>,
    default_fg: Option<(u8, u8, u8)>,
    palette: &mut Vec<(u8, u8, u8)>,
    body: &mut String,
) {
    // Cumulative right-edge positions (twips) for each column.
    let mut cumulative = 0usize;
    let cellx: Vec<usize> = table
        .col_widths
        .iter()
        .map(|w| {
            cumulative += w * CHAR_TWIPS;
            cumulative
        })
        .collect();
    let bg_idx = default_bg.map(|rgb| palette_index(palette, rgb));

    for row in &table.rows {
        // Row defaults + cell defs.
        body.push_str("\\trowd\\trgaph0\\trleft0");
        for &x in &cellx {
            if let Some(idx) = bg_idx {
                body.push_str(&format!("\\clcbpat{}\\clshdng10000", idx));
            }
            body.push_str("\\clvertalt");
            body.push_str(&format!("\\cellx{}", x));
        }
        body.push('\n');
        // Cell contents.
        for cell in row {
            body.push_str("\\pard\\intbl\\plain\\f0\\fs24");
            if let Some(idx) = bg_idx {
                body.push_str(&format!("\\cbpat{}\\shading10000", idx));
            }
            body.push(' ');
            for tok in tokenize(cell) {
                match tok {
                    Token::Newline => {} // cells are single-line
                    Token::Run(style, text) => {
                        emit_run(&style, &text, default_bg, default_fg, palette, body)
                    }
                }
            }
            body.push_str("\\cell\n");
        }
        body.push_str("\\row\n");
        // Reset paragraph after row so next block starts fresh.
        body.push_str("\\pard\\plain\\f0\\fs24\\sl-275\\slmult0\\sb0\\sa0");
        if let Some(idx) = bg_idx {
            body.push_str(&format!("\\cbpat{}\\shading10000", idx));
        }
        body.push('\n');
    }
}

pub fn render(
    input: &str,
    default_bg: Option<(u8, u8, u8)>,
    default_fg: Option<(u8, u8, u8)>,
) -> String {
    let blocks = parse_blocks(input);
    render_blocks(&blocks, default_bg, default_fg)
}

pub fn render_blocks(
    blocks: &[Block],
    default_bg: Option<(u8, u8, u8)>,
    default_fg: Option<(u8, u8, u8)>,
) -> String {
    let mut palette: Vec<(u8, u8, u8)> = Vec::new();
    let mut body = String::new();

    // Reserve palette[1] for default_bg so its index is stable for paragraph
    // shading and per-run \cb fallback.
    let default_bg_idx = default_bg.map(|rgb| palette_index(&mut palette, rgb));

    body.push_str("\\pard\\plain\\f0\\fs24\\sl-275\\slmult0\\sb0\\sa0");
    if let Some(idx) = default_bg_idx {
        body.push_str(&format!("\\cbpat{}\\shading10000", idx));
    }
    body.push('\n');

    for block in blocks {
        match block {
            Block::Text(t) => emit_text_block(t, default_bg, default_fg, &mut palette, &mut body),
            Block::Table(t) => emit_table_block(t, default_bg, default_fg, &mut palette, &mut body),
        }
    }

    let mut color_table = String::from("{\\colortbl;");
    for &(r, g, b) in &palette {
        color_table.push_str(&format!("\\red{}\\green{}\\blue{};", r, g, b));
    }
    color_table.push('}');

    let bg_shape = default_bg.map(background_shape).unwrap_or_default();

    format!(
        "{{\\rtf1\\ansi\\ansicpg1252\\uc1{}{}{}\n{}\n}}",
        "{\\fonttbl{\\f0\\fmodern\\fcharset0 Menlo;}}", color_table, bg_shape, body
    )
}

pub fn pbcopy_prefer() -> &'static str {
    "rtf"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_plain_text() {
        let out = render("hello", None, None);
        assert!(out.starts_with("{\\rtf1"));
        assert!(out.contains("hello"));
        assert!(out.ends_with("}"));
    }

    #[test]
    fn escapes_special_chars() {
        let mut buf = String::new();
        emit_rtf_text("a\\b{c}d", &mut buf);
        assert_eq!(buf, "a\\\\b\\{c\\}d");
    }

    #[test]
    fn emits_bmp_unicode_as_signed_u_escape() {
        let mut buf = String::new();
        emit_rtf_text("✗", &mut buf);
        assert_eq!(buf, "\\u10007?");
    }

    #[test]
    fn emits_supplementary_unicode_as_surrogate_pair() {
        let mut buf = String::new();
        emit_rtf_text("\u{1F600}", &mut buf);
        assert_eq!(buf, "\\u-10179?\\u-8704?");
    }

    #[test]
    fn includes_color_for_ansi_red() {
        let out = render("\x1b[31mred\x1b[0m", None, None);
        assert!(out.contains("\\red205\\green0\\blue0;"));
        assert!(out.contains("\\cf1 "));
    }

    #[test]
    fn deduplicates_colors_in_palette() {
        let out = render("\x1b[31ma\x1b[0m\x1b[31mb\x1b[0m", None, None);
        let n = out.matches("\\red205\\green0\\blue0;").count();
        assert_eq!(n, 1);
    }

    #[test]
    fn applies_default_bg_when_style_has_none() {
        let out = render("plain", None, None);
        assert!(!out.contains("\\cb"));
        let out_bg = render("plain", Some((0x1e, 0x1e, 0x1e)), None);
        assert!(out_bg.contains("\\cb1 "));
        assert!(out_bg.contains("\\red30\\green30\\blue30;"));
    }

    #[test]
    fn ansi_bg_overrides_default() {
        let out = render(
            "plain \x1b[41mred-bg\x1b[0m",
            Some((0x1e, 0x1e, 0x1e)),
            None,
        );
        assert!(out.contains("\\red205\\green0\\blue0;"));
        assert!(out.contains("\\red30\\green30\\blue30;"));
    }

    #[test]
    fn line_break_renders_par() {
        let out = render("a\nb", None, None);
        assert!(out.contains("\\par"));
    }

    #[test]
    fn colorref_encoding() {
        assert_eq!(colorref((255, 0, 0)), 255);
        assert_eq!(colorref((0, 255, 0)), 65280);
        assert_eq!(colorref((0, 0, 255)), 16711680);
        assert_eq!(colorref((30, 30, 30)), 30 + 30 * 256 + 30 * 65536);
    }

    #[test]
    fn includes_paragraph_shading_and_page_fill_when_default_bg_set() {
        let out = render("plain", Some((30, 30, 30)), None);
        assert!(out.contains("\\cbpat1\\shading10000"));
        assert!(out.contains("\\*\\background"));
        assert!(out.contains("fillColor"));
    }

    #[test]
    fn no_doc_bg_extras_when_default_bg_unset() {
        let out = render("plain", None, None);
        assert!(!out.contains("\\cbpat"));
        assert!(!out.contains("\\*\\background"));
    }

    #[test]
    fn bold_italic_underline_emitted() {
        let out = render("\x1b[1;3;4mfancy\x1b[0m", None, None);
        assert!(out.contains("\\b "));
        assert!(out.contains("\\i "));
        assert!(out.contains("\\ul "));
    }

    #[test]
    fn renders_table_with_trowd_and_cells() {
        let input = "\
┌────┬────┐
│ A  │ B  │
└────┴────┘";
        let out = render(input, None, None);
        assert!(out.contains("\\trowd"));
        assert!(out.contains("\\cellx"));
        assert!(out.contains("\\intbl"));
        assert!(out.contains("\\cell"));
        assert!(out.contains("\\row"));
        assert!(out.contains("A"));
        assert!(out.contains("B"));
    }

    #[test]
    fn table_cells_have_shading_when_default_bg() {
        let input = "\
┌──┐
│ X│
└──┘";
        let out = render(input, Some((30, 30, 30)), None);
        assert!(out.contains("\\clcbpat1\\clshdng10000"));
    }
}
