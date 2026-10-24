#!/bin/sh
# Reference groov module. Set GROOV_MODULE_LOG to observe invocations.
if [ -n "${GROOV_MODULE_LOG:-}" ]; then
    printf '%s event=%s id=%s\n' \
        "${GROOV_HELLO_MODULE:-0}" \
        "${GROOV_EVENT:-unknown}" \
        "${GROOV_SONG_ID:-}" >> "$GROOV_MODULE_LOG"
fi
