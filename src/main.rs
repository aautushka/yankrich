mod ansi;
mod blocks;
mod rtf;

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::process::{Command, Stdio};

use ansi::{count_visible, slice_line};

#[derive(Copy, Clone, Debug)]
enum Format {
    Rtf,
}

impl Format {
    fn parse(s: &str) -> Option<Format> {
        match s {
            "rtf" => Some(Format::Rtf),
            _ => None,
        }
    }

    fn render(
        self,
        input: &str,
        bg: Option<(u8, u8, u8)>,
        fg: Option<(u8, u8, u8)>,
    ) -> (String, &'static str) {
        match self {
            Format::Rtf => (rtf::render(input, bg, fg), rtf::pbcopy_prefer()),
        }
    }
}

fn log(msg: &str) {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/yankrich.log")
    {
        let _ = writeln!(f, "{}", msg);
    }
}

/// Extract `--format <name>` from argv, returning the chosen Format
/// (default: rtf) and argv with the flag removed.
fn extract_format(mut args: Vec<String>) -> (Format, Vec<String>) {
    let mut format = Format::Rtf;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--format" && i + 1 < args.len() {
            format = Format::parse(&args[i + 1]).unwrap_or_else(|| {
                eprintln!("unknown --format: {}", args[i + 1]);
                std::process::exit(2);
            });
            args.drain(i..=i + 1);
            continue;
        }
        i += 1;
    }
    (format, args)
}

fn main() {
    let raw_args: Vec<String> = env::args().collect();
    log(&format!("argv: {:?}", raw_args));
    let (format, args) = extract_format(raw_args);

    if args.len() == 2 && args[1] == "calibrate" {
        run_calibrate();
        return;
    }

    if args.len() == 2 && args[1] == "--stdin" {
        use std::io::Read;
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).expect("stdin");
        let (bg, fg) = read_terminal_colors();
        let (out, _) = format.render(&input, bg, fg);
        print!("{}", out);
        return;
    }

    if args.len() < 8 {
        eprintln!(
            "usage: yankrich [--format rtf] <pane_id> <sx> <sy> <ex> <ey> <rect> <history_size>"
        );
        std::process::exit(2);
    }
    let pane_id = &args[1];
    let sx: i32 = args[2].parse().unwrap_or(0);
    let sy_abs: i32 = args[3].parse().unwrap_or(0);
    let ex: i32 = args[4].parse().unwrap_or(0);
    let ey_abs: i32 = args[5].parse().unwrap_or(0);
    let rect = args[6] == "1";
    let history_size: i32 = args[7].parse().unwrap_or(0);

    // tmux selection_*_y is absolute buffer line (history + visible);
    // capture-pane -S/-E uses 0 = top of visible, negative = into history.
    let sy = sy_abs - history_size;
    let ey = ey_abs - history_size;

    let ((sx, sy), (ex, ey)) = if rect {
        ((sx.min(ex), sy.min(ey)), (sx.max(ex), sy.max(ey)))
    } else if (sy, sx) <= (ey, ex) {
        ((sx, sy), (ex, ey))
    } else {
        ((ex, ey), (sx, sy))
    };

    log(&format!(
        "normalized: sx={} sy={} ex={} ey={} rect={}",
        sx, sy, ex, ey, rect
    ));

    let out = Command::new("tmux")
        .args([
            "capture-pane",
            "-e",
            "-p",
            "-J",
            "-t",
            pane_id,
            "-S",
            &sy.to_string(),
            "-E",
            &ey.to_string(),
        ])
        .output()
        .expect("failed to run tmux capture-pane");

    if !out.status.success() {
        eprintln!(
            "capture-pane failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        std::process::exit(1);
    }

    let captured = String::from_utf8_lossy(&out.stdout).to_string();
    log(&format!("captured {} bytes", captured.len()));

    let lines: Vec<&str> = captured.split('\n').collect();
    let n = lines.len();
    let mut pieces: Vec<String> = Vec::with_capacity(n);

    for (i, line) in lines.iter().enumerate() {
        let (col_start, col_end_inclusive) = if rect {
            (sx as usize, ex as usize)
        } else if n == 1 {
            (sx as usize, ex as usize)
        } else if i == 0 {
            (sx as usize, usize::MAX)
        } else if i == n - 1 {
            (0, ex as usize)
        } else {
            (0, usize::MAX)
        };
        pieces.push(slice_line(line, col_start, col_end_inclusive));
    }

    let max_visible = pieces.iter().map(|p| count_visible(p)).max().unwrap_or(0);
    for piece in pieces.iter_mut() {
        let v = count_visible(piece);
        if v < max_visible {
            piece.push_str(&" ".repeat(max_visible - v));
        }
    }

    let sliced = pieces.join("\n");
    log(&format!("sliced: {:?}", sliced));

    let (term_bg, term_fg) = read_terminal_colors();
    let (rendered, prefer) = format.render(&sliced, term_bg, term_fg);
    let _ = std::fs::write(format!("/tmp/yankrich.{}", prefer), &rendered);
    log(&format!(
        "rendered {} bytes ({}) — written to /tmp/yankrich.{}",
        rendered.len(),
        prefer,
        prefer
    ));

    let mut pbcopy = Command::new("pbcopy")
        .args(["-Prefer", prefer])
        .stdin(Stdio::piped())
        .spawn()
        .expect("spawn pbcopy");
    {
        let stdin = pbcopy.stdin.as_mut().expect("pbcopy stdin");
        stdin
            .write_all(rendered.as_bytes())
            .expect("write rendered");
    }
    let status = pbcopy.wait().expect("wait pbcopy");
    log(&format!("pbcopy exit: {:?}", status.code()));
}

fn config_dir() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    Some(format!("{}/.config/yankrich", home))
}

fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    if !s.starts_with('#') || s.len() != 7 {
        return None;
    }
    Some((
        u8::from_str_radix(&s[1..3], 16).ok()?,
        u8::from_str_radix(&s[3..5], 16).ok()?,
        u8::from_str_radix(&s[5..7], 16).ok()?,
    ))
}

fn read_terminal_colors() -> (Option<(u8, u8, u8)>, Option<(u8, u8, u8)>) {
    let Some(dir) = config_dir() else {
        return (None, None);
    };
    let content = std::fs::read_to_string(format!("{}/colors", dir)).unwrap_or_default();
    let mut bg = None;
    let mut fg = None;
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("bg=") {
            bg = parse_hex_color(v);
        } else if let Some(v) = line.strip_prefix("fg=") {
            fg = parse_hex_color(v);
        }
    }
    (bg, fg)
}

fn run_calibrate() {
    let saved = Command::new("stty")
        .args(["-f", "/dev/tty", "-g"])
        .output()
        .expect("stty -g");
    let saved_str = String::from_utf8_lossy(&saved.stdout).trim().to_string();

    let restore = |saved: &str| {
        let _ = Command::new("stty").args(["-f", "/dev/tty", saved]).status();
    };

    let _ = Command::new("stty")
        .args(["-f", "/dev/tty", "raw", "-echo"])
        .status();
    let _ = Command::new("stty")
        .args(["-f", "/dev/tty", "min", "0", "time", "2"])
        .status();

    let result = (|| -> Option<((u8, u8, u8), (u8, u8, u8))> {
        let mut tty = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .ok()?;
        tty.write_all(b"\x1b]11;?\x07\x1b]10;?\x07").ok()?;
        tty.flush().ok()?;

        use std::io::Read;
        let mut buf = vec![0u8; 512];
        let mut total = 0;
        for _ in 0..10 {
            let n = tty.read(&mut buf[total..]).ok()?;
            if n == 0 {
                break;
            }
            total += n;
            let s = std::str::from_utf8(&buf[..total]).unwrap_or("");
            if s.matches("rgb:").count() >= 2 {
                break;
            }
        }

        let resp = std::str::from_utf8(&buf[..total]).ok()?;
        let parse_at = |start: usize| -> Option<(u8, u8, u8)> {
            let s = &resp[start + 4..];
            let mut parts = s.splitn(3, '/');
            let r4 = parts.next()?.get(..4)?;
            let g4 = parts.next()?.get(..4)?;
            let b_str = parts.next()?;
            let b_end = b_str
                .find(|c: char| !c.is_ascii_hexdigit())
                .unwrap_or(b_str.len());
            let b4 = b_str[..b_end].get(..4)?;
            let r = u16::from_str_radix(r4, 16).ok()?;
            let g = u16::from_str_radix(g4, 16).ok()?;
            let b = u16::from_str_radix(b4, 16).ok()?;
            Some(((r >> 8) as u8, (g >> 8) as u8, (b >> 8) as u8))
        };
        let mut iter = resp.match_indices("rgb:");
        let i1 = iter.next()?.0;
        let i2 = iter.next()?.0;
        Some((parse_at(i1)?, parse_at(i2)?))
    })();

    restore(&saved_str);

    match result {
        Some((bg, fg)) => {
            let dir = config_dir().expect("HOME not set");
            let _ = std::fs::create_dir_all(&dir);
            let content = format!(
                "bg=#{:02x}{:02x}{:02x}\nfg=#{:02x}{:02x}{:02x}\n",
                bg.0, bg.1, bg.2, fg.0, fg.1, fg.2
            );
            std::fs::write(format!("{}/colors", dir), &content).expect("write config");
            println!("calibrated:\n{}", content);
        }
        None => {
            eprintln!("OSC 10/11 query failed — no response from terminal within 200ms");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_color_valid() {
        assert_eq!(parse_hex_color("#1e1e1e"), Some((0x1e, 0x1e, 0x1e)));
        assert_eq!(parse_hex_color("#ffffff"), Some((255, 255, 255)));
        assert_eq!(parse_hex_color("  #000000  "), Some((0, 0, 0)));
    }

    #[test]
    fn parse_hex_color_invalid() {
        assert_eq!(parse_hex_color(""), None);
        assert_eq!(parse_hex_color("1e1e1e"), None);
        assert_eq!(parse_hex_color("#1e1e1"), None);
        assert_eq!(parse_hex_color("#zzzzzz"), None);
    }

    #[test]
    fn extract_format_default_is_rtf() {
        let (f, rest) = extract_format(vec!["bin".into(), "a".into(), "b".into()]);
        assert!(matches!(f, Format::Rtf));
        assert_eq!(rest, vec!["bin", "a", "b"]);
    }

    #[test]
    fn extract_format_strips_flag() {
        let (f, rest) = extract_format(vec![
            "bin".into(),
            "--format".into(),
            "rtf".into(),
            "a".into(),
        ]);
        assert!(matches!(f, Format::Rtf));
        assert_eq!(rest, vec!["bin", "a"]);
    }
}
