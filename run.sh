#!/bin/bash

# ctrl-space-wsl - dmenu-style application launcher for WSL
#
# Config file: ~/.config/ctrl-space-wsl/config.toml
# Example config:
#   [appearance]
#   foreground = "#F8F8F2"
#   background = "#21222C"
#   selection_fg = "#F8F8F2"
#   selection_bg = "#6272A4"
#   match_highlight = "#8be9fd"
#   prompt_color = "#BD93F9"
#   font_family = "Monospace"
#   font_size = 10
#
# Frequency data: ~/.config/ctrl-space-wsl/freq.txt
# Lock file (PID): ~/.config/ctrl-space-wsl/pid

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/target/release/ctrl-space-wsl"
