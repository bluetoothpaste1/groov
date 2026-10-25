# Changelog

## 1.0.0 — 2026-10-24

The first public release of the Rust-based local player.

- Portable Ratatui/Crossterm TUI for Linux, macOS, SSH, tmux, and IDE terminals.
- Directory-based local library with atomic imports and metadata writes.
- External audio backend with configurable command, seek, pause, and repeat.
- Built-in and user-installed themes.
- Language-independent module API with events, commands, key bindings, environment variables, and module management.
- JSONL RPC foundation for module status, overlays, paragraphs, lists, gauges, and redraw messages.
- Full user, module, configuration, architecture, and development documentation.

## Unreleased

Use this section for changes after 1.0.0. Every behavior change must update the documentation and tests in the same change.

- Made speedr apply the selected speed and pitch immediately when a new track starts instead of waiting for the adjustment debounce.
- Fixed seek with the default mpv backend and preserved seek for legacy mpv settings without `{start}`.
- Added runtime `:mode normal|compact` switching. Compact view renders only the current cover and maps `j/k` to immediate adjacent-track playback.
- Added persisted `:follow on|off` selection tracking for automatic playback transitions.
- Added `groov uninstall` with explicit confirmation before removing the complete local groov data set.
- Added Solarized, Catppuccin, Tokyo Night, and transparent built-in themes plus wrapped custom theme files.
- Added `groov modules install/add`, `remove`, and `enable/disable` commands with a singular `groov module` alias.
- Added the generic JSONL `playback` response contract for module-controlled restart, seek, filters, pause, resume, and stop actions.
- Added the optional `basic_modules/speedr` extension with live 0–100 speed and pitch controls.
- Unified module view input with the shared `:` command line and `:q`/`:close` lifecycle, and made `Esc` cancel instead of quitting NORMAL mode.
- Added configurable speedr adjustment steps, selected-control highlighting, debounced filter restarts, and higher-quality fallback resampling.
- Added the generic `{player_ipc}` / `GROOV_PLAYER_IPC` backend-control contract and an mpv live path for speedr, avoiding process restarts during adjustments.
- Made mpv the preferred default backend when available, installed speedr automatically from `install.sh`, and kept ffplay as an automatic compatibility fallback.
- Simplified the build and installation path to the Rust application target.
