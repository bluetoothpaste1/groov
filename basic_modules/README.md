# Basic modules

`basic_modules` contains real out-of-tree groov extensions. They are not compiled into groov. `flame` and `easyadd` are disabled until enabled; the installer installs `speedr` as the default live-control module. Copy a module into the user's module directory, reload groov, and enable it from the TUI when needed:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/groov/modules"
cp -R basic_modules/flame "${XDG_CONFIG_HOME:-$HOME/.config}/groov/modules/"
cp -R basic_modules/easyadd "${XDG_CONFIG_HOME:-$HOME/.config}/groov/modules/"
cp -R basic_modules/speedr "${XDG_CONFIG_HOME:-$HOME/.config}/groov/modules/"
groov
```

Then run `:reload`, `:modules on flame`, and `:modules on easyadd`; speedr is enabled by its manifest.

These modules use the documented JSONL RPC session protocol. They need Python 3 and use the module working directory assigned by groov. `easyadd` additionally uses the standard `GROOV_SONGS_DIR` path exposed to RPC modules, while `speedr` uses the generic playback response, its `speedr.json` step setting, and optional `ffprobe` metadata.

- [flame](flame/README.md) animates the TUI border color using a configurable fire palette.
- [easyadd](easyadd/README.md) opens an interactive add-track form and writes a complete song directory atomically.
- [speedr](speedr/README.md) provides live speed and pitch controls through the generic playback RPC contract.
