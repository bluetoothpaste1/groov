# groov

`groov` is a terminal-first VK Music player. The first slice is deliberately small: it provides a reusable VK client and a command that fetches the current user's audio library.

## Build

macOS provides `clang` and `libcurl` when Xcode Command Line Tools are installed:

```sh
make clean && make
make test
make run
```

`make run` prints the command help. To request tracks, provide a VK API access token with the `audio` permission:

```sh
export GROOV_VK_ACCESS_TOKEN='your-token'
./build/groov tracks --count 100
```

The token is never written to disk. `--token TOKEN` is also supported for one-off local use, although an environment variable avoids exposing the token in the process list.

The client currently uses VK's public `audio.get` API method. A browser `session_key` is a different credential and is intentionally not sent as an API access token; support for VK Music's private web endpoints can be added behind the same client boundary later.
