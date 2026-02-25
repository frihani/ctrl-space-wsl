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
- `Tab` autocomplete
- `Escape` close
- `Left/Right` navigate
- `Delete` remove from history
