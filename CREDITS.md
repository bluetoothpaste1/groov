# Credits and attribution

groov does not copy source code from the projects below. It uses their published APIs and crates under their respective licenses:

- [Rust](https://www.rust-lang.org/) and the Rust standard library.
- [Serde](https://serde.rs/) and [serde_json](https://github.com/serde-rs/json) for typed JSON configuration and metadata.
- [crossterm](https://github.com/crossterm-rs/crossterm) for portable terminal input and screen control.
- [image](https://github.com/image-rs/image) for decoding and resizing PNG, JPEG, and WebP cover art.
- [Ratatui](https://ratatui.rs/) for buffered, diff-based terminal rendering.
- [ratatui-image](https://github.com/ratatui/ratatui-image) for terminal image protocol detection and image widgets.
- [Neovim's UI event protocol](https://github.com/neovim/neovim/blob/master/runtime/doc/api-ui-events.txt) as an architectural reference for grid-based redraws; no Neovim source code is copied.
- [FFmpeg](https://ffmpeg.org/), specifically `ffplay`, as the default external audio player when installed.

Each dependency remains under its upstream license. See `Cargo.lock` and the dependency license files for exact versions and notices.
