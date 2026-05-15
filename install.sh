#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

MODE="${1:-dev}"
REPO="$(pwd)"

build() {
    echo ">>> Building..."
    cargo build --release
}

# Render the committed tmux.conf template by substituting __BIN__ with the
# given absolute binary path. Single source of truth for the binding line.
render_tmux_conf() {
    local bin="$1"
    sed "s|__BIN__|$bin|g" "$REPO/tmux.conf.template"
}

# Locate the user's actual tmux config. Returns path on stdout or empty.
find_tmux_config() {
    local candidates=(
        "$HOME/.tmux.conf"
        "$HOME/.config/tmux/tmux.conf"
        "${XDG_CONFIG_HOME:-$HOME/.config}/tmux/tmux.conf"
    )
    for c in "${candidates[@]}"; do
        if [[ -f "$c" ]]; then
            echo "$c"
            return
        fi
    done
}

# If $1 is the path to a tmux config, ask the user if we should add a
# `source-file` line pointing at $2. Idempotent: skips if line already present.
maybe_patch_tmux_config() {
    local config="$1" snippet="$2"
    local line="source-file $snippet"
    if [[ -z "$config" ]]; then
        echo ">>> No tmux config found at any of ~/.tmux.conf, ~/.config/tmux/tmux.conf."
        echo "    Add this line yourself once you create one:"
        echo "      $line"
        return
    fi
    if grep -qsF "$line" "$config"; then
        echo ">>> $config already sources $snippet — nothing to patch."
        return
    fi
    echo ">>> Found tmux config at: $config"
    read -r -p "    Append \`$line\` to it? [y/N] " ans
    case "$ans" in
        y|Y|yes)
            printf '\n# yankrich\n%s\n' "$line" >> "$config"
            echo "    Patched."
            ;;
        *)
            echo "    Skipped. Add it manually when ready."
            ;;
    esac
}

case "$MODE" in
    dev)
        build
        render_tmux_conf "$REPO/target/release/yankrich" > "$REPO/tmux.conf"
        echo ">>> Calibrating terminal colors..."
        "$REPO/target/release/yankrich" calibrate
        if [[ -n "${TMUX:-}" ]]; then
            echo ">>> Sourcing tmux config..."
            tmux source-file "$REPO/tmux.conf"
        fi
        maybe_patch_tmux_config "$(find_tmux_config)" "$REPO/tmux.conf"
        echo
        echo ">>> Dev install done. Rebuilds in this directory take effect immediately."
        ;;
    system)
        BIN_DIR="$HOME/.local/bin"
        CONFIG_DIR="$HOME/.config/yankrich"
        mkdir -p "$BIN_DIR" "$CONFIG_DIR"
        build
        cp "$REPO/target/release/yankrich" "$BIN_DIR/"
        render_tmux_conf "$BIN_DIR/yankrich" > "$CONFIG_DIR/tmux.conf"
        echo ">>> Calibrating terminal colors..."
        "$BIN_DIR/yankrich" calibrate
        if [[ -n "${TMUX:-}" ]]; then
            echo ">>> Sourcing tmux config..."
            tmux source-file "$CONFIG_DIR/tmux.conf"
        fi
        maybe_patch_tmux_config "$(find_tmux_config)" "$CONFIG_DIR/tmux.conf"
        echo
        echo ">>> System install done."
        echo "    Binary:      $BIN_DIR/yankrich"
        echo "    Tmux config: $CONFIG_DIR/tmux.conf"
        echo "    Ensure $BIN_DIR is in your PATH."
        ;;
    *)
        cat <<EOF
usage: $0 [dev|system]
  dev     (default) build in place; tmux conf points at $REPO/target/release/yankrich
  system  copy binary to ~/.local/bin and write tmux conf to ~/.config/yankrich
EOF
        exit 1
        ;;
esac
