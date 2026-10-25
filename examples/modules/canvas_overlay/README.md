# canvas-overlay

This session module demonstrates the composited canvas protocol:

- playback events draw a non-modal HUD over the normal library;
- `:canvas-overlay` or `O` opens a modal panel with its own z-layer;
- the panel includes a host-driven 60 FPS spinner without per-frame JSONL traffic;
- `Esc` and `:q` close the modal layer while preserving the HUD.

Install it from the repository checkout:

```sh
groov modules install examples/modules/canvas_overlay
groov modules enable canvas-overlay
```

Reload a running player with `:reload`. Canvas coordinates can be absolute
cells or percentages of the target layer. Modules can draw text, fills,
single cells, horizontal/vertical lines, arbitrary lines, and borders with
foreground/background colors and modifiers.
