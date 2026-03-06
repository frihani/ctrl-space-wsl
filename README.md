# ctrl-space-wsl

[![crates.io](https://img.shields.io/crates/v/ctrl-space-wsl.svg)](https://crates.io/crates/ctrl-space-wsl)

A fast application launcher for WSL2, inspired by dmenu/yeganesh.

## Features

- Fuzzy search across PATH executables
- Frequency-based sorting
- Cached app list for fast startup
- Single instance
- Stdin filter mode (pipe in any list, select with fuzzy search)

## Installation

### From crates.io

```bash
cargo install ctrl-space-wsl
```

### From source

```bash
make release
make install
```

Or manually:

```bash
cargo build --release
cp target/release/ctrl-space-wsl ~/.local/bin/
```

Ensure `~/.local/bin` is in your PATH.

## Usage

```bash
ctrl-space-wsl --info         # Show version and file paths
ctrl-space-wsl --init-config  # Create default config file
```

## Global Hotkey (PowerToys)

1. Open **PowerToys** → **Keyboard Manager** → **Remap a shortcut**
2. Add a shortcut (e.g., `Ctrl+Space`) with action **Run Program**:
   - Program: `C:\Program Files\WSL\wslg.exe`
   - Arguments: `-- ctrl-space-wsl`

## Keys

- Type to filter
- `Enter` launch selected
- `Shift+Enter` launch in terminal (for TUI apps like `top`, `htop`, `vim`)
- `Tab` autocomplete
- `Escape` close
- `Left/Right` navigate
- `Delete` remove from history

## Configuration

Config file: `~/.config/ctrl-space-wsl/config.toml`

```toml
[appearance]
foreground = "#f8f8f2"
background = "#21222c"
selection_fg = "#f8f8f2"
selection_bg = "#6272a4"
match_highlight = "#8be9fd"
prompt_color = "#bd93f9"
font_family = "Monospace"
font_size = 10
dpi = 96
position = "top"  # "top", "center", or "bottom"

[launcher]
terminal = "x-terminal-emulator -e"  # Linux default
# terminal = "alacritty.exe -e wsl.exe"      # WSLg from Windows with alacritty terminal
```

## Usage as a filter

Pipe any list into ctrl-space-wsl to use it as a general-purpose selector. Your selection gets printed to stdout (not launched), and the program disables frequency tracking.

```bash
# Select a file from a directory
ls | ctrl-space-wsl

# Git branch switcher
git branch | ctrl-space-wsl | xargs git checkout

# Search git commits
git log --oneline | ctrl-space-wsl | awk '{print $1}' | xargs git show

# Process killer
ps aux | ctrl-space-wsl | awk '{print $2}' | xargs kill

# Open a recent file
find . -type f -name "*.rs" | ctrl-space-wsl | xargs code

# Search environment variables
printenv | ctrl-space-wsl

# Look through installed fonts
fc-list : family | sort -u | ctrl-space-wsl
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
