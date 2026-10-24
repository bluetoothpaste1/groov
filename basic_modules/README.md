# Basic modules

`basic_modules` contains real out-of-tree groov extensions. They are not compiled into groov and are not enabled automatically. Copy a module into the user's module directory, reload groov, and enable it from the TUI:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/groov/modules"
cp -R basic_modules/flame "${XDG_CONFIG_HOME:-$HOME/.config}/groov/modules/"
cp -R basic_modules/easyadd "${XDG_CONFIG_HOME:-$HOME/.config}/groov/modules/"
groov
```

Then run `:reload`, `:modules on flame`, and `:modules on easyadd`.

Both modules use the documented JSONL RPC session protocol. They need Python 3 and use the module working directory assigned by groov. `easyadd` additionally uses the standard `GROOV_SONGS_DIR` path exposed to RPC modules.

- [flame](flame/README.md) animates the TUI border color using a configurable fire palette.
- [easyadd](easyadd/README.md) opens an interactive add-track form and writes a complete song directory atomically.
