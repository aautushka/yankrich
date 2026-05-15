# yankrich

Yank styled text out of tmux as RTF, with colors and backgrounds intact, so it
pastes into TextEdit, Mail, Notes, Slack, Word, Google Docs, etc. looking like
a screenshot of the terminal.

## How it works

1. You select text in tmux's `copy-mode-vi`.
2. You press `Y` (capital).
3. tmux's `capture-pane -e` hands us the selection with ANSI escape sequences
   intact. yankrich slices it to your selection, converts ANSI → RTF, applies
   your terminal's background as the document fill, and pipes it to `pbcopy`.
4. You paste anywhere that accepts rich text.

Bordered tables (`┌─┐│└┘` and ASCII `+-|`) are detected and emitted as RTF
tables with explicit cell widths, so the paste survives in editors that use
proportional fonts.

## Install

macOS only (depends on `pbcopy`, `reattach-to-user-namespace`, and tmux's
copy-mode formats).

```
git clone <repo> ~/proj/yankrich
cd ~/proj/yankrich
./install.sh           # dev install (rebuilds in place)
# or
./install.sh system    # copies binary to ~/.local/bin
```

`install.sh` will:
- `cargo build --release`
- Generate `tmux.conf` from `tmux.conf.template` with the absolute binary path
- Calibrate your terminal's foreground/background by sending OSC 10/11 queries
  to `/dev/tty` and saving the answer to `~/.config/yankrich/colors`
- Source the tmux conf if you're inside tmux
- Offer to append `source-file <path>` to your `~/.tmux.conf` or
  `~/.config/tmux/tmux.conf`

After install, open a tmux pane, select text in copy-mode-vi (`prefix [`,
move, `v`, move), press `Y`, paste.

## Re-calibrate

If you change your terminal colorscheme, run:

```
yankrich calibrate
```

## What gets emitted in the RTF

For maximum reader compatibility, the output stacks three background-color
mechanisms — each reader ignores what it doesn't understand:

| Layer | RTF control | Who honors it |
| --- | --- | --- |
| Page fill | `{\*\background\shp...}` | MS Word, WordPad |
| Paragraph shading | `\cbpat<N>\shading10000` | Word, LibreOffice, Google Docs |
| Character bg | `\cb<N>` per run | Cocoa (TextEdit, Mail, Notes) |

Tables additionally get `\clcbpat<N>\clshdng10000` per cell, which fills the
full cell rectangle (covers the inter-line gap that `\cb` can't).

## Layout

```
src/
  main.rs    — CLI, tmux capture, slicing, calibrate, dispatch
  ansi.rs    — ANSI parser, tokenizer, column slicer, color tables
  blocks.rs  — bordered-table detector, splits sliced text into Text/Table blocks
  rtf.rs     — RTF renderer (text paragraphs + native RTF tables)
install.sh           — build/install + tmux conf patching
tmux.conf.template   — committed; install.sh fills __BIN__ → real path
demo-colors.sh       — exercise script for testing
```

Adding another format (HTML, PNG, ...) = one new module + one variant on
`Format` in `main.rs`.

## Tests

```
cargo test
```

## Known limitations

- **TextEdit dark mode** auto-inverts explicit colors; output may look
  inverted relative to your terminal. Switch the doc to light mode.
- **TextEdit cannot fill inter-line gaps via RTF** — `\cb` paints character
  cells only; `\cbpat` and `{\*\background}` are ignored by Cocoa. The
  rendered output uses tight line spacing (`\sl-275` = 13.75pt) to minimize
  the visible gap. Word/LibreOffice/Google Docs honor the paragraph and
  page-level fills so they look pixel-clean.
- **Proportional-font paste targets** (Notion, some web editors) flatten
  spaces and break monospace alignment for non-table text. Tables still look
  right because their widths are in twips, not space-padding.
- **Space-aligned tables** (`ls -l`, `git log`) are *not* detected — only
  bordered tables. By design.
- **Selection coordinates** assume tmux 3.x format variables; older versions
  expose different y semantics.
- **No `/dev/tty`** in the calibration path means no terminal color detection
  (e.g., running calibrate from inside `tmux source-file`). Run it from a
  normal shell prompt.
