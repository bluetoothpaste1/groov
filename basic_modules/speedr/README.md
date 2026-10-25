# speedr

`speedr` is an optional JSONL RPC module for live slowed and speed-up reverb playback. It opens a compact window with two 0–100 gauges:

- **Speed**: `0` is `0.50x`, `50` is normal playback, and `100` is `2.00x`;
- **Pitch**: `0` is `-12` semitones, `50` is unchanged, and `100` is `+12` semitones.

With a live IPC backend the player stays open and receives changes without a restart. Without IPC, the module uses the generic filter/restart path. Non-neutral settings add a short, subtle echo; 50/50 leaves the original audio untouched. Pitch uses the track's native sample rate in the fallback path so the tempo and pitch controls remain independent. The module needs Python 3, `ffprobe` for precise fallback pitch processing, and an `ffplay` backend or mpv.

For fast, seamless live control, use mpv's IPC backend. mpv keeps one player process alive and receives speed/pitch changes directly:

```json
{
  "player_command": "mpv --no-video --no-terminal --audio-pitch-correction=yes --input-ipc-server={player_ipc} {file}"
}
```

On macOS install it with `brew install mpv`; on Debian/Ubuntu use `sudo apt install mpv`. Without `{player_ipc}`, speedr falls back to the generic filter/restart path, which is compatible with ffplay but necessarily slower while changing controls.

## Install

From the groov checkout:

```sh
groov modules install basic_modules/speedr
groov modules enable speedr
```

Reload a running player with `:reload`, then open the panel with `:speedr` or `s` in NORMAL mode. Use `j`/`k` (or `↑`/`↓`) to choose a control and `h`/`l` (or `←`/`→`) to change it. The active control is marked with `▸`; `r` returns both controls to 50, and `:q` closes the panel.

The default adjustment step is five points. Change it for the current session with `:speedr step N`, where `N` is 1–25. To make a step persistent, edit `speedr.json` in the module directory:

```json
{
  "step": 2
}
```

The module is disabled by default, so installing it never changes playback until it is enabled.
