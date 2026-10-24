# easyadd

`easyadd` is an out-of-tree JSONL RPC session module that opens a small form for importing a track without a long `groov add` command. It collects:

- audio file (required);
- cover image (optional);
- title;
- description;
- artist;
- lyrics file (optional).

Install it by copying this directory to `$XDG_CONFIG_HOME/groov/modules/easyadd/`, then run:

```text
:reload
:modules on easyadd
:easyadd
```

The `a` key opens the same form. `Tab`, `Down`, and `Up` move between fields; `Enter` advances and submits from the last field; `Backspace` edits the current value. Close with `Esc` or `:q` while the form is active.

On submit the module creates the next numeric song directory below `GROOV_SONGS_DIR`, copies the audio, cover, and lyrics into it, writes normalized `info.json`, and asks groov to rescan the library. The staging directory is renamed into place only after every required file is ready.
