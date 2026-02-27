# ctrl-space-wsl

A fast application launcher for WSL2, inspired by dmenu/yeganesh.

## Features

- Fuzzy search across PATH executables
- Frequency-based sorting
- Cached app list for fast startup
- Single instance

## Installation

```bash
make release
make install
```

Or manually:

```bash
cargo build --release
cp target/release/ctrl-space-wsl ~/.local/bin/
```

### Backends

Two backends are available:

- **x11** (default) - Lightweight, minimal dependencies
- **sdl2** - Uses egui/SDL2 for rendering

```bash
# Build with X11 backend (default)
cargo build --release

# Build with SDL2 backend
cargo build --release --no-default-features --features sdl2-backend
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
foreground = "#F8F8F2"
background = "#21222C"
selection_fg = "#F8F8F2"
selection_bg = "#6272A4"
match_highlight = "#8be9fd"
prompt_color = "#BD93F9"
font_family = "Monospace"
font_size = 10
dpi = 96

[launcher]
terminal = "x-terminal-emulator -e"  # Linux default
# terminal = "alacritty.exe -e wsl.exe"      # WSLg from Windows
```
