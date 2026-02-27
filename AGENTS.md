# Agent Guidelines

## Build

```bash
make release
```

Or: `cargo build --release`

Build with SDL2 backend instead of default X11:

```bash
cargo build --release --no-default-features --features sdl2-backend
```

## Format

```bash
make lint
```

Runs `cargo fmt` and `cargo clippy`. Run this after making changes to ensure consistent code style.

## Test

Run the release binary directly:

```bash
./target/release/ctrl-space-wsl
```

## Project Structure

- `src/main.rs` - Entry point, CLI handling, backend dispatch
- `src/backend_x11.rs` - X11 backend (default)
- `src/backend_sdl2.rs` - SDL2/egui backend
- `src/ui.rs` - UI rendering for SDL2 backend (egui)
- `src/filter.rs` - Fuzzy matching and scoring
- `src/frequency.rs` - Usage tracking and app caching
- `src/launcher.rs` - Command execution
- `src/lock.rs` - Single instance management
- `src/config.rs` - Configuration loading
- `src/app_discovery.rs` - PATH scanning

## Config Location

Config and data stored in `~/.config/ctrl-space-wsl/`.

Run `ctrl-space-wsl --info` to see file paths.

## X11 Backend Notes

### Window Positioning (IMPORTANT)

The X11 backend MUST set `WM_NORMAL_HINTS` with `USPosition | PPosition` flags before mapping the window. This tells the window manager to respect our requested position (0, 0) instead of placing the window automatically.

Without this, rapid relaunching causes the window manager to place new windows at random Y positions (it tries to avoid the still-closing previous window).

```rust
// flags: USPosition (1) | PPosition (4) = 5
let size_hints: [u32; 18] = [
    5,    // flags: USPosition | PPosition
    0,    // x
    0,    // y
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
conn.change_property32(
    PropMode::REPLACE,
    win_id,
    AtomEnum::WM_NORMAL_HINTS,
    AtomEnum::WM_SIZE_HINTS,
    &size_hints,
)?;
```

### Other X11 Window Properties

- `_NET_WM_WINDOW_TYPE_DOCK` - Marks as dock window
- `_NET_WM_STATE_ABOVE` + `_NET_WM_STATE_STICKY` - Always on top, visible on all workspaces
- `_NET_WM_STRUT` / `_NET_WM_STRUT_PARTIAL` - Reserves screen space at top
- `_MOTIF_WM_HINTS` with decorations=0 - Removes window decorations
