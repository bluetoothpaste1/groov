# flame

`flame` is an out-of-tree JSONL RPC session module. It animates groov's border color through a configurable fire palette. The module does not link against groov or modify its source.

Install it by copying this directory to `$XDG_CONFIG_HOME/groov/modules/flame/`, then run:

```text
:reload
:modules on flame
:flame
```

The `f` key is also bound to `:flame`. Use `:flame off` to stop the animation and restore the active theme border. The module keeps its own settings in `flame.json`:

```json
{
  "interval_ms": 90,
  "colors": ["#fff4b0", "#ffd166", "#ff9f43", "#f97316", "#ef4444"]
}
```

`interval_ms` is clamped by the module to a safe animation rate. Colors accept the same six-digit hex form used by groov themes.
