#!/usr/bin/env python3
import json
import sys

for line in sys.stdin:
    request = json.loads(line)
    song = request.get("song") or {}
    if request.get("type") == "input":
        response = {
            "type": "status",
            "text": "RPC key: " + request.get("key", "?"),
        }
    elif request.get("type") == "event":
        response = {
            "type": "status",
            "text": "RPC: " + song.get("title", "no track"),
        }
    else:
        response = {
            "type": "paragraph",
            "title": "RPC request",
            "text": json.dumps(request, indent=2),
        }
    print(json.dumps(response), flush=True)
