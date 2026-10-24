# groov

`groov` is a local music player for people who prefer a keyboard. It is a Vim-like TUI with a directory-based library, tag filters, cover art, a configurable audio backend, named themes, and a language-independent module API. The screen is rendered through Ratatui's buffered diff renderer, so unchanged cells are not rewritten on every timer tick.

Current release: `1.0.0`. See [CHANGELOG.md](CHANGELOG.md) for release notes and [LICENSE](LICENSE) for the MIT license.

Source installation:

```sh
make release
make install PREFIX="$HOME/.local"
```

The project CI checks Rust on Linux and macOS, the legacy C target on Linux, documentation links, formatting, clippy, tests, and release builds.

Tagged builds use the `vX.Y.Z` convention. GitHub Actions packages Linux and macOS release archives with the architecture in the filename.

The first implementation is written in Rust. The previous C VK experiment remains in `src/main.c` and `src/vk.c` and can still be built with `make legacy`; it is isolated from the local player.

## Build and run

Rust and Cargo are required. The default player command uses `ffplay` from FFmpeg, so install FFmpeg if audio playback is needed.

```sh
make
make test
make docs-check
make release
./target/debug/groov init
./target/debug/groov add ~/Downloads/song.mp3
./target/debug/groov add ./keep-a-copy.mp3 --copy --tag archive
./target/debug/groov
```

Useful commands are `groov list`, `groov list --tag rock --tag night`, `groov scan`, `groov doctor`, `groov paths`, `groov theme list`, `groov theme install FILE`, `groov theme use NAME`, and `groov modules`. `groov add` accepts `--album`, `--image`, `--lyrics`, `--id`, `--copy`, and `--dry-run` as well. By default it moves the source file; `--copy` keeps the original in Downloads, while `--dry-run` only prints the planned import. If `ffprobe` is installed, title, artist, album, and duration are read automatically. `groov edit 13 --title "New title" --tag live --untag draft` updates an existing song atomically.

## Files and directories

groov follows the XDG layout:

```text
~/.config/groov/settings.json       configuration
~/.local/share/groov/songs/         GDD (Groov data directory)
~/.cache/groov/                     generated/cache data
```

`XDG_CONFIG_HOME`, `XDG_DATA_HOME`, and `XDG_CACHE_HOME` override these defaults. A song is self-contained:

```text
songs/13/song.mp3
songs/13/info.json
songs/13/image.jpg       optional (png, jpg, jpeg, webp)
songs/13/lyrics.txt      optional
```

`info.json` uses the requested capitalized keys (`Artist`, `Lyrics`, `Title`, `Album`, `Image`, `Tags`) and also accepts `Duration` and `AddedAt`. `Tags` are normalized to lowercase and duplicate tags are removed. `groov list --tag a --tag b` uses AND semantics.

## Controls

groov has two actual interaction modes: `NORMAL` for navigation/playback and `COMMAND` for commands. Search and help are commands/panels, so they do not create hidden state modes. Normal mode uses `j/k` or arrow keys to move, `Space` to play/pause or start the selected track, `x` to stop, `h/l` to seek by the configured skip interval, `n`/`]` and `[` for queue navigation, `a` to enqueue, `/` to enter `:search`, `:` for commands, `?` for help, and `q` to quit. Commands include `:tag live`, `:untag draft`, `:tags sad rock`, `:repeat on`, `:repeat off`, `:set title "New title"`, `:set desc "A favorite track"`, `:set artist "Rock group"`, `:set image ~/cover.jpg`, `:set lyrics ~/lyrics.txt`, `:set skips 10`, `:set coversize 16`, `:set theme ocean`, `:reload`, `:queue`, `:enqueue`, `:next`, `:prev`, `:clear`, `:play`, `:stop`, `:rescan`, and `:q`.

Cover art is rendered by `ratatui-image` into one fixed square area. The full source image is scaled directly into that area without cropping or empty padding. `image_mode: "auto"` deliberately uses ANSI half blocks, which work in Linux and macOS terminals, SSH, tmux, and IDE terminals. Sixel, Kitty, and iTerm2 are explicit opt-ins (`sixel`, `kitty`, `iterm2`); `none` disables images. The audio command is configurable through `{file}` and `{start}`; its standard streams are isolated so backend output cannot corrupt the TUI. groov always restores raw mode and the alternate screen when the terminal or player exits with an error.

While a track is playing, the TUI shows its elapsed time and progress bar. `h` and `l` seek backward or forward by `skip_seconds` (5 seconds by default); `:set skips 10` changes and persists that interval. `:repeat on` repeats the current track when it ends; `:repeat off` returns to normal queue progression. The repeat setting is persisted in `settings.json`.

In command mode, `Tab` completes command names, `set` fields, and paths for `image`/`lyrics`. Path completion understands both absolute paths and `~/...`.

Settings can enable external modules with `modules[].command`; they receive playback events through `GROOV_EVENT`, `GROOV_EVENT_JSON`, `GROOV_SONG_ID`, and `GROOV_SONG_FILE`. A module can expose command aliases (`commands`) and key bindings (`keys`), set environment variables (`env`), choose a working directory (`cwd`), enable JSONL RPC views (`rpc`), keep a streaming RPC process (`session`), and restrict events with `events`. RPC modules can return status, overlays, paragraphs, lists, gauges, declarative widget trees, input handling, and redraw messages to the TUI. Standalone modules are discovered from `~/.config/groov/modules/<name>/module.json` and use their manifest directory as the default working directory, so a module can be written in Rust, Python, Lua, shell, or any other language without changing groov. The module process contract is versioned with `GROOV_API_VERSION=1`.

Manage modules from COMMAND mode with `:modules` or `:modules list`, `:modules on NAME`, `:modules off NAME`, and `:modules info NAME`. The module panel shows the manifest path, working directory, metadata, commands, key bindings, events, and environment. The same operations are available from the CLI as `groov modules`, `groov modules on NAME`, `groov modules off NAME`, and `groov modules info NAME`. A complete reference extension lives in [`examples/modules/hello`](examples/modules/hello).

Themes are built in (`monochrome`, `ocean`, `gruvbox`, `nord`, and `dracula`) and can be installed as JSON with `groov theme install theme.json`. Select one interactively with `:set theme ocean`; the choice is persisted in `settings.json`. Installed JSON files in `~/.config/groov/themes/<name>.json` are loaded automatically.

Полный справочник находится в [docs/README.md](docs/README.md): там есть [руководство пользователя](docs/USER_GUIDE.md), [guide по модулям](docs/MODULES.md), [справочник конфигурации](docs/REFERENCE.md), [руководство разработчика](docs/DEVELOPMENT.md) и [архитектурные решения](docs/ARCHITECTURE.md). Third-party software and attribution are listed in [CREDITS.md](CREDITS.md).
