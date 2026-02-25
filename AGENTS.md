# Agent Guidelines

## Build

```bash
make release
```

Or: `cargo build --release`

## Test

Run the release binary directly:

```bash
./target/release/ctrl-space-wsl
```

## Project Structure

- `src/main.rs` - Entry point, window setup
- `src/ui.rs` - UI rendering and input handling (eframe/egui)
- `src/filter.rs` - Fuzzy matching and scoring
- `src/frequency.rs` - Usage tracking and app caching
- `src/launcher.rs` - Command execution
- `src/lock.rs` - Single instance management
- `src/config.rs` - Configuration loading
- `src/app_discovery.rs` - PATH scanning

## Config Location

Config and data stored in `~/.config/ctrl-space-wsl/`.

Run `ctrl-space-wsl --info` to see file paths.
