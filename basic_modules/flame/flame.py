#!/usr/bin/env python3
"""Animated fire border module for groov's JSONL session protocol."""

import json
import random
import sys
import threading
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parent
DEFAULT_COLORS = [
    "#fff4b0",
    "#ffd166",
    "#ff9f43",
    "#f97316",
    "#ef4444",
    "#9f1239",
    "#4c0519",
]
stop_event = threading.Event()
writer_lock = threading.Lock()
animation_thread = None


def send(message):
    with writer_lock:
        print(json.dumps(message, separators=(",", ":")), flush=True)


def load_config():
    try:
        config = json.loads((ROOT / "flame.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        config = {}
    colors = [
        color
        for color in config.get("colors", DEFAULT_COLORS)
        if isinstance(color, str) and len(color) == 7 and color.startswith("#")
    ]
    if not colors:
        colors = DEFAULT_COLORS
    interval = config.get("interval_ms", 90)
    try:
        interval = max(40, min(1000, int(interval))) / 1000
    except (TypeError, ValueError):
        interval = 0.09
    return colors, interval


def animate():
    colors, interval = load_config()
    phase = random.randrange(len(colors))
    while not stop_event.is_set():
        # Neighboring shades are preferred, with occasional jumps that make
        # the border look alive instead of cycling as a rigid gradient.
        phase = (phase + random.choice((-1, 0, 0, 1))) % len(colors)
        send({"type": "palette", "border": colors[phase], "flush": True})
        stop_event.wait(interval)


def start():
    global animation_thread
    if animation_thread is not None and animation_thread.is_alive():
        return
    stop_event.clear()
    animation_thread = threading.Thread(target=animate, daemon=True)
    animation_thread.start()
    send({"type": "status", "text": "flame borders: on"})


def stop():
    global animation_thread
    stop_event.set()
    if animation_thread is not None:
        animation_thread.join(timeout=1.1)
        animation_thread = None
    send({"type": "palette", "clear": True, "flush": True})
    send({"type": "status", "text": "flame borders: off"})


def command(request):
    args = request.get("args") or []
    action = str(args[0]).lower() if args else "on"
    if action in {"on", "start"}:
        start()
    elif action in {"off", "stop"}:
        stop()
    else:
        send({"type": "error", "text": "usage: :flame on|off"})


for line in sys.stdin:
    try:
        request = json.loads(line)
    except json.JSONDecodeError:
        continue
    if request.get("type") == "command" and request.get("event") == "command:flame":
        command(request)
