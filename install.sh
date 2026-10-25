#!/usr/bin/env sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix=${PREFIX:-"${HOME:-}/.local"}
if [ -z "$prefix" ] || [ "$prefix" = "/.local" ]; then
    echo "install.sh: set PREFIX when HOME is unavailable" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "install.sh: cargo is required (install Rust with rustup)" >&2
    exit 1
fi
if ! command -v rustc >/dev/null 2>&1; then
    echo "install.sh: rustc is required (install Rust with rustup)" >&2
    exit 1
fi
if ! command -v install >/dev/null 2>&1; then
    echo "install.sh: the install utility is required" >&2
    exit 1
fi

echo "Building groov in release mode..."
cargo build --release --locked --manifest-path "$script_dir/Cargo.toml"

bindir="$prefix/bin"
mandir="$prefix/share/man/man1"
mkdir -p "$bindir" "$mandir"
install -m 755 "$script_dir/target/release/groov" "$bindir/groov"
if [ -f "$script_dir/packaging/groov.1" ]; then
    install -m 644 "$script_dir/packaging/groov.1" "$mandir/groov.1"
fi

if command -v mpv >/dev/null 2>&1; then
    echo "Detected mpv: live speedr playback is available."
else
    echo "install.sh: mpv was not found; groov will use ffplay fallback." >&2
    echo "Install mpv for seamless speedr control (macOS: brew install mpv; Debian/Ubuntu: sudo apt install mpv)." >&2
fi

module_source="$script_dir/basic_modules/speedr"
config_home=${XDG_CONFIG_HOME:-"${HOME:-}/.config"}
if [ -f "$module_source/module.json" ] && [ -n "$config_home" ]; then
    module_destination="$config_home/groov/modules/speedr"
    mkdir -p "$module_destination"
    cp -R "$module_source/." "$module_destination/"
    echo "Installed speedr module to $module_destination"
fi

echo "Installed groov to $bindir/groov"
case ":${PATH:-}:" in
    *:"$bindir":*) ;;
    *) echo "Add $bindir to PATH: export PATH=\"$bindir:\$PATH\"" ;;
esac
echo "Initialize the library with: $bindir/groov init"
