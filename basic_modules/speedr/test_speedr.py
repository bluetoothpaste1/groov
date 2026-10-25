import json
import os
import socket
import tempfile
import threading
import unittest
from contextlib import redirect_stdout
from io import StringIO
from unittest.mock import patch

import speedr


class SpeedrTests(unittest.TestCase):
    def test_speed_range_is_centered_on_normal_playback(self):
        self.assertAlmostEqual(speedr.speed_factor(0), 0.5)
        self.assertAlmostEqual(speedr.speed_factor(50), 1.0)
        self.assertAlmostEqual(speedr.speed_factor(100), 2.0)

    def test_pitch_range_is_centered_on_zero_semitones(self):
        self.assertAlmostEqual(speedr.pitch_semitones(0), -12.0)
        self.assertAlmostEqual(speedr.pitch_semitones(50), 0.0)
        self.assertAlmostEqual(speedr.pitch_semitones(100), 12.0)

    def test_values_are_clamped(self):
        self.assertEqual(speedr.clamp(-1), 0)
        self.assertEqual(speedr.clamp(101), 100)

    def test_neutral_controls_clear_the_audio_filter(self):
        self.assertIsNone(speedr.audio_filter(50, 50, 44100))

    def test_filter_keeps_pitch_correction_separate_from_tempo(self):
        rendered = speedr.audio_filter(0, 100, 44100)
        self.assertEqual(
            rendered,
            "asetrate=88200,aresample=44100:filter_size=64:cutoff=0.97,atempo=0.500000,atempo=0.500000,aresample=async=1:first_pts=0,aecho=0.8:0.88:80:0.18",
        )

    def test_rpc_view_starts_at_neutral_values_and_emits_playback(self):
        speedr.values = {field: speedr.DEFAULT_VALUE for field in speedr.FIELDS}
        speedr.field_index = 0
        speedr.current_song = None
        speedr.view_open = False
        output = StringIO()
        with redirect_stdout(output):
            speedr.handle(
                {"type": "command", "event": "command:speedr", "args": [], "song": None}
            )
            speedr.handle({"type": "input", "key": "right"})
            speedr.flush_playback()
        messages = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertEqual(messages[0]["root"]["children"][0]["value"], 50)
        playback_message = next(message for message in messages if message["type"] == "playback")
        self.assertEqual(
            playback_message["audio_filter"],
            "atempo=1.071773,aresample=async=1:first_pts=0,aecho=0.8:0.88:80:0.18",
        )

    def test_step_command_and_selected_marker_are_visible(self):
        speedr.values = {field: speedr.DEFAULT_VALUE for field in speedr.FIELDS}
        speedr.field_index = 0
        speedr.step_size = speedr.DEFAULT_STEP
        speedr.view_open = False
        output = StringIO()
        with redirect_stdout(output):
            speedr.handle(
                {
                    "type": "command",
                    "event": "command:speedr",
                    "args": ["step", "2"],
                    "song": None,
                }
            )
        messages = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertEqual(speedr.step_size, 2)
        self.assertTrue(messages[-1]["root"]["children"][0]["title"].startswith("▸"))

    def test_mpv_path_updates_properties_without_restarting(self):
        speedr.values = {"speed": 55, "pitch": 45}
        speedr.reverb_active = False
        with patch.object(speedr, "send_mpv", return_value=True) as send:
            self.assertTrue(speedr.mpv_playback())
        commands = send.call_args.args[0]
        self.assertEqual(commands[0], ["set_property", "audio-pitch-correction", True])
        self.assertEqual(commands[1][0:2], ["set_property", "speed"])
        self.assertEqual(commands[2][0:2], ["set_property", "pitch"])
        self.assertTrue(any(command[0:2] == ["af", "add"] for command in commands))

    def test_ipc_commands_wait_for_backend_acknowledgements(self):
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, "player.sock")
            server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            server.bind(path)
            server.listen(1)

            def acknowledge():
                connection, _ = server.accept()
                with connection:
                    for line in connection.makefile():
                        request = json.loads(line)
                        connection.sendall(
                            (
                                json.dumps(
                                    {"request_id": request["request_id"], "error": "success"}
                                )
                                + "\n"
                            ).encode()
                        )

            thread = threading.Thread(target=acknowledge, daemon=True)
            thread.start()
            with patch.object(speedr, "PLAYER_IPC", path):
                self.assertTrue(speedr.send_mpv([["set_property", "speed", 0.5]]))
            server.close()


if __name__ == "__main__":
    unittest.main()
