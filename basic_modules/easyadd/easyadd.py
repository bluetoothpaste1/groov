#!/usr/bin/env python3
"""Interactive track importer implemented as an external groov module."""

import json
import os
import shutil
import sys
import time
from pathlib import Path


FIELDS = [
    ("Audio file*", "audio"),
    ("Cover image", "cover"),
    ("Title", "title"),
    ("Description", "description"),
    ("Artist", "artist"),
    ("Lyrics file", "lyrics"),
]
values = {name: "" for _, name in FIELDS}
field_index = 0
colon_pending = False


def send(message):
    print(json.dumps(message, separators=(",", ":")), flush=True)


def show():
    children = []
    for index, (label, name) in enumerate(FIELDS):
        value = values[name] or ("required" if name == "audio" else "optional")
        marker = "▸ " if index == field_index else "  "
        children.append(
            {
                "kind": "paragraph",
                "title": label,
                "text": marker + value,
                "flex": 1,
            }
        )
    children.append(
        {
            "kind": "paragraph",
            "text": "Tab/↑↓ move   Enter next/save   Backspace edit   :q or Esc close",
            "flex": 1,
        }
    )
    send(
        {
            "type": "view",
            "flush": True,
            "root": {
                "kind": "column",
                "title": "Easy Add",
                "children": children,
            },
        }
    )


def close():
    send({"type": "close", "flush": True})


def error(message):
    send({"type": "error", "text": message, "flush": True})
    show()


def source_path(value):
    return Path(os.path.expanduser(value)).expanduser()


def next_song_id(songs_dir):
    ids = [
        int(path.name)
        for path in songs_dir.iterdir()
        if path.is_dir() and path.name.isdigit()
    ]
    return str(max(ids, default=0) + 1)


def add_song():
    songs_dir_value = os.environ.get("GROOV_SONGS_DIR")
    if not songs_dir_value:
        error("GROOV_SONGS_DIR is unavailable")
        return
    audio = source_path(values["audio"])
    if not audio.is_file():
        error("audio file not found: " + str(audio))
        return
    cover = source_path(values["cover"]) if values["cover"] else None
    lyrics = source_path(values["lyrics"]) if values["lyrics"] else None
    if cover is not None:
        if not cover.is_file():
            error("cover file not found: " + str(cover))
            return
        if cover.suffix.lower() not in {".jpg", ".jpeg", ".png", ".webp"}:
            error("cover must be jpg, jpeg, png, or webp")
            return
    if lyrics is not None and not lyrics.is_file():
        error("lyrics file not found: " + str(lyrics))
        return

    songs_dir = Path(songs_dir_value)
    songs_dir.mkdir(parents=True, exist_ok=True)
    song_id = next_song_id(songs_dir)
    staging = songs_dir / (".easyadd-" + song_id + "-" + str(os.getpid()))
    destination = songs_dir / song_id
    try:
        staging.mkdir()
        shutil.copy2(audio, staging / "song.mp3")
        image_name = ""
        if cover is not None:
            image_name = "image" + cover.suffix.lower()
            shutil.copy2(cover, staging / image_name)
        lyrics_name = ""
        if lyrics is not None:
            lyrics_name = "lyrics.txt"
            shutil.copy2(lyrics, staging / lyrics_name)
        info = {
            "Id": song_id,
            "Artist": values["artist"] or "Unknown artist",
            "Lyrics": lyrics_name,
            "Title": values["title"] or audio.stem or "Untitled",
            "Album": "",
            "Description": values["description"],
            "Image": image_name,
            "Tags": [],
            "Duration": 0,
            "AddedAt": str(int(time.time())),
        }
        (staging / "info.json").write_text(
            json.dumps(info, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        os.replace(staging, destination)
    except (OSError, ValueError) as exc:
        shutil.rmtree(staging, ignore_errors=True)
        error("cannot add track: " + str(exc))
        return

    close()
    send({"type": "rescan", "flush": True})
    send({"type": "status", "text": "added " + info["Title"], "flush": True})


def reset_form():
    global field_index, colon_pending
    field_index = 0
    colon_pending = False
    for _, name in FIELDS:
        values[name] = ""
    show()


def handle_key(key):
    global field_index, colon_pending
    if key in {"escape", "ctrl-c"}:
        close()
        return
    if key == ":":
        colon_pending = True
        return
    if colon_pending:
        colon_pending = False
        if key == "q":
            close()
            return
        if len(key) == 1:
            values[FIELDS[field_index][1]] += ":" + key
            show()
            return
    if key in {"tab", "down"}:
        field_index = (field_index + 1) % len(FIELDS)
    elif key == "up":
        field_index = (field_index - 1) % len(FIELDS)
    elif key == "backspace":
        name = FIELDS[field_index][1]
        values[name] = values[name][:-1]
    elif key == "enter":
        if field_index == len(FIELDS) - 1:
            add_song()
            return
        field_index += 1
    elif len(key) == 1 and key.isprintable():
        values[FIELDS[field_index][1]] += key
    show()


for line in sys.stdin:
    try:
        request = json.loads(line)
    except json.JSONDecodeError:
        continue
    request_type = request.get("type")
    if request_type == "command" and request.get("event") == "command:easyadd":
        reset_form()
    elif request_type == "input":
        handle_key(str(request.get("key", "")))
