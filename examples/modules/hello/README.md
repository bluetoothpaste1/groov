# hello module

This is a minimal reference extension. Copy this directory to
`$XDG_CONFIG_HOME/groov/modules/hello/`, then run `:reload` in groov.

It demonstrates:

- manifest metadata (`name`, `version`, `description`, `author`);
- playback event hooks through `GROOV_EVENT`;
- a `:hello` command;
- an `H` key binding in NORMAL mode;
- module environment variables and a relative executable.

Set `GROOV_MODULE_LOG=/tmp/groov-hello.log` in the manifest's `env` section if
you want to observe invocations while testing.
