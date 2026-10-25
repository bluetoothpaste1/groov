#!/usr/bin/env python3
"""Interactive live speed and pitch controls for groov."""

import json
import os
import socket
import subprocess
import sys
import threading
import time
from pathlib import Path


FIELDS = ("speed", "pitch")
DEFAULT_VALUE = 50
DEFAULT_STEP = 5
MAX_STEP = 25
APPLY_DELAY = 0.25
REVERB_FILTER = "aecho=0.8:0.88:80:0.18"
ROOT = Path(__file__).resolve().parent
PLAYER_IPC = os.environ.get("GROOV_PLAYER_IPC", "")
values = {field: DEFAULT_VALUE for field in FIELDS}
field_index = 0
step_size = DEFAULT_STEP
current_song = None
view_open = False
sample_rates = {}
output_lock = threading.Lock()
playback_timer = None
playback_generation = 0
reverb_active = False


def send(message):
    with output_lock:
        print(json.dumps(message, separators=(",", ":")), flush=True)


def load_step():
    try:
        config = json.loads((ROOT / "speedr.json").read_text(encoding="utf-8"))
        return max(1, min(MAX_STEP, int(config.get("step", DEFAULT_STEP))))
    except (OSError, TypeError, ValueError, json.JSONDecodeError):
        return DEFAULT_STEP


def cancel_playback_timer():
    global playback_generation, playback_timer
    playback_generation += 1
    if playback_timer is not None:
        playback_timer.cancel()
        playback_timer = None


def run_scheduled_playback(generation):
    global playback_timer
    if generation != playback_generation:
        return
    playback_timer = None
    playback()


def schedule_playback():
    """Coalesce rapid key presses into one backend restart."""

    global playback_generation, playback_timer
    cancel_playback_timer()
    generation = playback_generation
    playback_timer = threading.Timer(
        APPLY_DELAY, run_scheduled_playback, args=(generation,)
    )
    playback_timer.daemon = True
    playback_timer.start()


def flush_playback():
    """Apply a pending value immediately; useful when a command needs certainty."""

    cancel_playback_timer()
    playback()


def clamp(value):
    return max(0, min(100, int(value)))


def speed_factor(value):
    """Map the 0..100 speed control to a 0.5x..2x playback factor."""

    return 0.5 * (4.0 ** (clamp(value) / 100.0))


def pitch_semitones(value):
    """Map the 0..100 pitch control to -12..+12 semitones."""

    return (clamp(value) - 50) / 50.0 * 12.0


def audio_filter(speed, pitch, native_rate):
    """Build an ffmpeg filter while keeping speed and pitch independent."""

    speed = clamp(speed)
    pitch = clamp(pitch)
    tempo = speed_factor(speed)
    pitch_ratio = 2.0 ** (pitch_semitones(pitch) / 12.0)
    filters = []
    if pitch != DEFAULT_VALUE:
        native_rate = max(8000, min(384000, int(native_rate)))
        shifted_rate = round(native_rate * pitch_ratio)
        filters.extend(
            [
                f"asetrate={shifted_rate}",
                f"aresample={native_rate}:filter_size=64:cutoff=0.97",
                f"atempo={1.0 / pitch_ratio:.6f}",
            ]
        )
    if speed != DEFAULT_VALUE:
        filters.append(f"atempo={tempo:.6f}")
    if filters:
        filters.append("aresample=async=1:first_pts=0")
        filters.append(REVERB_FILTER)
    return ",".join(filters) if filters else None


def send_mpv(commands):
    """Send live property/filter commands to an mpv JSON IPC endpoint."""

    if not PLAYER_IPC or os.name == "nt":
        return False
    for attempt in range(3):
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
                connection.settimeout(0.15)
                connection.connect(PLAYER_IPC)
                for command in commands:
                    connection.sendall(
                        (json.dumps({"command": command}, separators=(",", ":")) + "\n").encode()
                    )
            return True
        except OSError:
            if attempt < 2:
                time.sleep(0.05)
    return False


def mpv_playback():
    """Apply speed and pitch without restarting mpv when IPC is available."""

    global reverb_active
    speed = speed_factor(values["speed"])
    pitch = 2.0 ** (pitch_semitones(values["pitch"]) / 12.0)
    non_neutral = values["speed"] != DEFAULT_VALUE or values["pitch"] != DEFAULT_VALUE
    commands = [
        ["set_property", "audio-pitch-correction", True],
        ["set_property", "speed", speed],
        ["set_property", "pitch", pitch],
    ]
    if non_neutral and not reverb_active:
        commands.append(["af", "add", f"@speedr-reverb:lavfi=[{REVERB_FILTER}]"])
    elif not non_neutral and reverb_active:
        commands.append(["af", "remove", "@speedr-reverb"])
    if not send_mpv(commands):
        return False
    reverb_active = non_neutral
    return True


def sample_rate(song_file):
    """Read a track's native sample rate for the host's pitch-preserving filter."""

    if not song_file:
        return 44100
    song_file = str(song_file)
    if song_file in sample_rates:
        return sample_rates[song_file]
    rate = 44100
    try:
        result = subprocess.run(
            [
                "ffprobe",
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=sample_rate",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
                song_file,
            ],
            capture_output=True,
            text=True,
            timeout=2,
            check=False,
        )
        candidate = int(result.stdout.strip().splitlines()[0])
        if 8000 <= candidate <= 384000:
            rate = candidate
    except (OSError, ValueError, IndexError, subprocess.TimeoutExpired):
        pass
    sample_rates[song_file] = rate
    return rate


def set_song(song):
    global current_song
    if isinstance(song, dict):
        song_id = song.get("id")
        if song_id:
            current_song = {
                "id": str(song_id),
                "file": song.get("file", ""),
            }


def playback():
    if mpv_playback():
        return
    needs_pitch_rate = values["pitch"] != DEFAULT_VALUE
    current_rate = (
        sample_rate(current_song.get("file"))
        if current_song and needs_pitch_rate
        else 44100
    )
    message = {
        "type": "playback",
        "action": "restart",
        "audio_filter": audio_filter(values["speed"], values["pitch"], current_rate),
    }
    if current_song:
        message["song_id"] = current_song["id"]
    send(message)


def show():
    speed = values["speed"]
    pitch = values["pitch"]
    speed_title = ("▸ " if field_index == 0 else "  ") + "Speed"
    pitch_title = ("▸ " if field_index == 1 else "  ") + "Pitch"
    children = [
        {
            "kind": "gauge",
            "title": speed_title,
            "value": speed,
            "max": 100,
            "text": f"{speed:3d}/100  {speed_factor(speed):.2f}x",
            "flex": 2,
        },
        {
            "kind": "gauge",
            "title": pitch_title,
            "value": pitch,
            "max": 100,
            "text": f"{pitch:3d}/100  {pitch_semitones(pitch):+.1f} st",
            "flex": 2,
        },
        {
            "kind": "paragraph",
            "text": f"j/k select   h/l adjust   step {step_size}   r reset   :q close",
            "flex": 1,
        },
    ]
    send(
        {
            "type": "view",
            "flush": True,
            "root": {
                "kind": "column",
                "title": "Speedr",
                "children": children,
            },
        }
    )


def close():
    global view_open
    cancel_playback_timer()
    view_open = False
    send({"type": "close", "flush": True})


def reset_values():
    for field in FIELDS:
        values[field] = DEFAULT_VALUE


def handle_command(request):
    global step_size, view_open
    if current_song is None:
        set_song(request.get("song"))
    args = request.get("args") or []
    action = str(args[0]).lower() if args else "open"
    if action == "step":
        if len(args) != 2:
            send({"type": "error", "text": "usage: :speedr step N", "flush": True})
        else:
            try:
                step_size = max(1, min(MAX_STEP, int(args[1])))
            except ValueError:
                send({"type": "error", "text": "speedr step must be 1..25", "flush": True})
            else:
                send({"type": "status", "text": f"speedr step: {step_size}", "flush": True})
        view_open = True
        show()
        return
    if action in {"reset", "neutral"}:
        reset_values()
        flush_playback()
    if action in {"close", "off"}:
        close()
        return
    view_open = True
    show()


def handle_key(key):
    global field_index
    if key in {"escape", "ctrl-c"}:
        close()
        return
    if key in {"up", "k"}:
        field_index = (field_index - 1) % len(FIELDS)
        show()
        return
    if key in {"down", "j", "tab"}:
        field_index = (field_index + 1) % len(FIELDS)
        show()
        return
    if key in {"left", "h"}:
        field = FIELDS[field_index]
        values[field] = clamp(values[field] - step_size)
        schedule_playback()
        show()
        return
    if key in {"right", "l"}:
        field = FIELDS[field_index]
        values[field] = clamp(values[field] + step_size)
        schedule_playback()
        show()
        return
    if key == "r":
        reset_values()
        schedule_playback()
        show()


def handle(request):
    global current_song, reverb_active
    request_type = request.get("type")
    event = request.get("event")
    if request_type == "command" and event == "command:speedr":
        handle_command(request)
    elif request_type == "input":
        handle_key(str(request.get("key", "")))
    elif request_type == "event" and event == "play":
        reverb_active = False
        set_song(request.get("song"))
        if values["speed"] != DEFAULT_VALUE or values["pitch"] != DEFAULT_VALUE:
            schedule_playback()
        if view_open:
            show()
    elif request_type == "event" and event == "stop":
        cancel_playback_timer()
        reverb_active = False
        current_song = None
        if view_open:
            show()


def run(stream):
    for line in stream:
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(request, dict):
            handle(request)


step_size = load_step()


if __name__ == "__main__":
    run(sys.stdin)
