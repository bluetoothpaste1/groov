#!/usr/bin/env python3
"""Demonstrate non-modal HUD and modal fullscreen canvas layers."""

import json
import sys


def send(message):
    print(json.dumps(message, separators=(",", ":")), flush=True)


def hud(song):
    song = song or {}
    title = song.get("title", "no track")
    artist = song.get("artist", "unknown artist")
    send(
        {
            "type": "canvas",
            "id": "hud",
            "z": 40,
            "modal": False,
            "rect": {"x": "2%", "y": "2%", "width": "96%", "height": 5},
            "commands": [
                {
                    "op": "fill",
                    "x": 0,
                    "y": 0,
                    "width": "100%",
                    "height": "100%",
                    "bg": "#202030",
                    "symbol": " ",
                },
                {
                    "op": "rect",
                    "x": 0,
                    "y": 0,
                    "width": "100%",
                    "height": "100%",
                    "fg": "#89b4fa",
                },
                {
                    "op": "text",
                    "x": 2,
                    "y": 2,
                    "text": f"{artist} — {title}",
                    "fg": "#a6e3a1",
                    "bold": True,
                    "max_width": "100%",
                },
            ],
        }
    )


def fullscreen():
    send(
        {
            "type": "canvas",
            "id": "panel",
            "mode": "replace",
            "z": 100,
            "rect": {"x": "10%", "y": "15%", "width": "80%", "height": "70%"},
            "commands": [
                {
                    "op": "fill",
                    "x": 0,
                    "y": 0,
                    "width": "100%",
                    "height": "100%",
                    "bg": "#1e1e2e",
                    "symbol": " ",
                },
                {
                    "op": "rect",
                    "x": 0,
                    "y": 0,
                    "width": "100%",
                    "height": "100%",
                    "fg": "#cba6f7",
                },
                {"op": "text", "x": 3, "y": 2, "text": "Canvas panel", "bold": True},
                {"op": "text", "x": 3, "y": 4, "text": "Press Esc or :q to close."},
            ],
        }
    )
    send(
        {
            "type": "animation",
            "id": "spinner",
            "z": 110,
            "fps": 60,
            "loop": True,
            "duration_ms": 400,
            "rect": {"x": "50%", "y": "20%", "width": 1, "height": 1},
            "frames": [
                {"at_ms": 0, "commands": [{"op": "text", "id": "glyph", "text": "|"}]},
                {"at_ms": 100, "commands": [{"op": "text", "id": "glyph", "text": "/"}]},
                {"at_ms": 200, "commands": [{"op": "text", "id": "glyph", "text": "-"}]},
                {"at_ms": 300, "commands": [{"op": "text", "id": "glyph", "text": "\\"}]},
            ],
        }
    )


for line in sys.stdin:
    try:
        request = json.loads(line)
    except json.JSONDecodeError:
        continue
    if request.get("type") == "event":
        if request.get("event") == "play":
            hud(request.get("song"))
        elif request.get("event") == "stop":
            send({"type": "canvas", "id": "hud", "clear": True})
    elif request.get("type") == "command":
        fullscreen()
    elif request.get("type") == "input" and request.get("key") in {"escape", "q"}:
        send({"type": "canvas", "id": "panel", "clear": True})
        send({"type": "animation", "id": "spinner", "clear": True})
        send({"type": "close"})
