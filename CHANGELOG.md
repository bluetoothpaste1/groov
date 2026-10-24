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
- Legacy C VK client retained as a separately built target.

## Unreleased

Use this section for changes after 1.0.0. Every behavior change must update the documentation and tests in the same change.

- Added runtime `:mode normal|compact` switching. Compact view renders only the current cover and maps `j/k` to immediate adjacent-track playback.
- Added persisted `:follow on|off` selection tracking for automatic playback transitions.
- Added `groov uninstall` with explicit confirmation before removing the complete local groov data set.
