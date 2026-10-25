# groov architecture

This file is the durable design note for the project. It records the data contract and extension points so the implementation can grow without changing the user's library layout.

For user workflows see [USER_GUIDE.md](USER_GUIDE.md), for extension authors see [MODULES.md](MODULES.md), for exact fields see [REFERENCE.md](REFERENCE.md), and for contribution rules see [DEVELOPMENT.md](DEVELOPMENT.md).

## Storage contract

The data directory is resolved using the XDG Base Directory convention. The config directory is `$XDG_CONFIG_HOME/groov` (or `~/.config/groov`), the GDD is `$XDG_DATA_HOME/groov` (or `~/.local/share/groov`), and the cache is `$XDG_CACHE_HOME/groov` (or `~/.cache/groov`). The GDD contains `songs/<id>/` directories. A playable song must contain `song.mp3` and `info.json`; `lyrics.txt` and one of `image.png`, `image.jpg`, `image.jpeg`, or `image.webp` are optional.

The directory name is the stable song ID. `info.json` is deliberately human-editable:

```json
{
  "Id": "13",
  "Artist": "Artist name",
  "Lyrics": "lyrics.txt",
  "Title": "A title",
  "Album": "An album",
  "Description": "A short note about the track",
  "Image": "image.jpg",
  "Tags": ["rock", "night"],
  "Duration": 0,
  "AddedAt": "unix-seconds"
}
```

Paths in metadata are relative to the song directory. Missing `Lyrics` or `Image` values use the conventional filenames. Unknown JSON keys are preserved when settings are loaded, so future configuration can be added without invalidating older clients.

`groov add FILE` creates the next numeric song directory, copies the source into the staging directory as `song.mp3`, writes metadata, and commits the directory atomically. The default behavior then removes the source file, which makes a Downloads folder act as an inbox. Use `groov add FILE --copy` to keep the original or `--dry-run` to inspect the planned destination and metadata without changing anything. When `ffprobe` is available, the importer uses embedded title, artist, album, and duration tags before falling back to the filename.

## Configuration, themes, and modules

`settings.json` is the single configuration file. The initial schema is:

```json
{
  "refresh_ms": 100,
  "theme": "monochrome",
  "image_mode": "auto",
  "player_command": "mpv --no-video --no-terminal --audio-pitch-correction=yes --start={start} --input-ipc-server={player_ipc} {file}",
  "skip_seconds": 5,
  "repeat": false,
  "follow_playback": false,
  "cover_size": 16,
  "themes": {},
  "modules": [
    {
      "name": "scrobbler",
      "version": "1.0.0",
      "description": "A playback scrobbler",
      "author": "Example author",
      "rpc": false,
      "enabled": true,
      "command": "groov-scrobbler",
      "args": ["{event}", "{id}"],
      "events": ["play", "stop"],
      "commands": [{"name": "scrobble", "args": ["{id}"]}],
      "keys": {"s": "scrobble"}
    }
  ],
  "keymap": { "quit": "q", "play": "space", "stop": "x", "search": "/" }
}
```

The `modules` array is intentionally data-driven. Each entry has a stable `name`, `enabled` flag, optional `command`, `args`, `events`, `commands`, `keys`, `env`, `cwd`, `rpc`, and `session`. Enabled command modules receive `GROOV_API_VERSION=1`, `GROOV_EVENT` (`play`, `stop`, `rescan`, or `metadata`), `GROOV_EVENT_JSON`, `GROOV_SONG_ID`, and `GROOV_SONG_FILE`; `{event}`, `{id}`, and `{file}` are expanded in arguments and environment values. A command alias is started when its name is entered after `:`. A key binding invokes the same alias directly from NORMAL mode. Standalone manifests at `$XDG_CONFIG_HOME/groov/modules/<name>/module.json` are discovered and merged with settings; their directory becomes the default `cwd`, so relative executables work. With `rpc: true`, the process receives JSONL requests and can return status, overlay, paragraph, list, gauge, declarative widget trees, generic playback actions, or redraw messages to the TUI. With `session: true`, input events can be sent back to an active module view. This process boundary lets modules be written in any language and keeps a broken module from taking down the TUI.

Module management is available without restarting groov: `:modules` (or `:modules list`) lists all modules, while the CLI installs independent modules with `groov modules install DIRECTORY` (or `groov module add DIRECTORY`), removes them with `groov modules remove NAME`, persists activation with `groov modules enable|disable NAME`, and exposes metadata with `groov modules info NAME`. The reference extension in `examples/modules/hello` is intentionally small and can be copied into the user's module directory as a starting point.

Themes are built in (`monochrome`, `ocean`, `gruvbox`, `nord`, `dracula`, `solarized`, `catppuccin`, `tokyo-night`, and `transparent`) and can be embedded under `themes` or installed as `$XDG_CONFIG_HOME/groov/themes/<name>.json`. Standalone files may be direct `ThemeConfig` objects or `{ "name": ..., "colors": ... }` wrappers. The commands `:set theme name` and `groov theme use name` validate and persist the selected name. Every widget reads the resolved palette, so theme selection affects the complete interface; `transparent` uses terminal reset colors instead of a fixed background.

A standalone module needs only a manifest and an executable. For example:

```json
{
  "name": "visualizer",
  "enabled": true,
  "command": "groov-visualizer",
  "events": ["play", "stop"],
  "commands": [{"name": "visualizer", "args": ["{file}"]}],
  "keys": {"v": "visualizer"}
}
```

The executable can inspect `GROOV_EVENT_JSON` and return immediately; module failures are reported as status text and never become part of the player process. This gives extensions a stable boundary without forcing every extension author to compile against groov internals.

## Interface model

The TUI separates input state from view state. Input is `NORMAL` for navigation or `COMMAND` for the `:` prompt; the view is `NORMAL` for the full library and `COMPACT` for a cover-only player. `:mode normal|compact` changes the view without writing settings. Search is the `search` command prefilled by `/`; help is a transient overlay toggled by `?`. Compact rendering deliberately skips every widget except the current cover, while `j/k` (and arrows) immediately start the next or previous visible queue item. `:follow on|off` persists whether the selected library row follows the track that starts playing; the default is off. `:reload` reloads settings, themes, and module manifests without restarting the player. Playback is delegated to a configurable process; new settings select mpv with live IPC when available and otherwise use ffplay. Pressing `Space` starts or pauses playback; `:queue`, `:enqueue`, `:next`, and `:prev` manage the queue. `:repeat on` restarts the completed track from zero, while `:repeat off` advances through the queue. The timeline is a diff-rendered gauge driven by elapsed playback time; `h` and `l` restart the player at an offset of `skip_seconds`. A custom `player_command` may use `{file}`, `{start}`, and `{audio_filter}` placeholders. RPC modules can request generic playback actions through a `playback` response; the host owns process lifecycle and preserves the extension boundary.

An active module view follows the same input model as the rest of the TUI. Normal keys are forwarded to the module, `:` enters the shared command line, `:q` or `:close` closes the view, and `Esc` cancels it. A module does not need a private exit convention or a second command parser.

Playback has two extension layers: `playback` responses provide safe host-owned restart/filter actions, while `{player_ipc}` and `GROOV_PLAYER_IPC` let a backend with its own control protocol receive live commands without being restarted. The latter remains backend-neutral at the host boundary; each module speaks the selected backend's documented IPC protocol.

Tag commands have one responsibility each: `:tag name` adds a tag to the selected song, `:untag name` removes it, and `:tags a b` replaces the active library filter with an AND selection. An empty `:tags` clears the filter. This keeps metadata editing separate from choosing the current playlist.

The universal `:set` command edits the selected song. It accepts `title`, `artist`, `album`, `desc`/`description`, `image`, `lyrics`, and `duration`. Values may be quoted, so `:set title "Title with spaces"` remains one value. `image` and `lyrics` copy the referenced file into the song directory and update the relative metadata path; the source files remain unchanged. `:set skips N` updates the persisted seek interval in `settings.json`, and `:set coversize N` changes the fixed cover width in terminal cells; the height is adjusted using the terminal font aspect so the rendered image remains square.

Command completion is state-local: `Tab` completes command names, `set` fields, and filesystem entries for image/lyrics paths. The path resolver expands `~` from `HOME` before reading directories and shortens paths back to `~/` in the command line.

Rendering follows the same separation used by mature terminal UIs: application state changes in response to events, and a render pass turns the complete state into a cell grid. Ratatui keeps a front and back buffer and sends only the cell diff to Crossterm. This avoids the old full-screen clear-and-print loop, which caused flicker and alignment errors. The design was checked against Neovim's `ui-linegrid` model, where redraws are expressed as changed `grid_line` ranges, cursor moves, and optimized scroll events rather than arbitrary terminal writes.

Cover decoding, fixed-size resize, and protocol encoding run in a worker thread. The TUI receives a generation-tagged result and ignores stale images when the selection changes quickly, so a large cover cannot block key handling or list navigation.

Cover rendering scales the full source image directly into one fixed square cell area. No crop and no padding are added, so every cover occupies exactly the same cells. `image_mode: "auto"` deliberately uses ANSI half blocks, which work in Linux and macOS terminals, SSH, tmux, and IDE terminals. Sixel, Kitty, and iTerm2 are explicit opt-ins (`sixel`, `kitty`, `iterm2`); `none` disables images. We intentionally avoid guessing from terminal names because environment variables are unreliable in multiplexers and IDE terminals.

## Language choice

Rust is the only application language because the filesystem, JSON, process, and terminal layers benefit from memory safety and typed errors. Network integrations remain outside the local library contract and can be implemented as external modules when needed.
