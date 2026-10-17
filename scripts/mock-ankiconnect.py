#!/usr/bin/env python3
"""A tiny stand-in for AnkiConnect, used by the end-to-end checks.

Listens on 127.0.0.1:<port> (default 8765), answers the actions the app uses and appends every request
body (one JSON per line) to <log file>.

Usage: mock-ankiconnect.py <log file> [port]
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

LOG = sys.argv[1]
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8765
NEXT_NOTE_ID = [1_700_000_000_000]


class Handler(BaseHTTPRequestHandler):
    def do_POST(self) -> None:
        length = int(self.headers.get("Content-Length", "0"))
        request = json.loads(self.rfile.read(length) or b"{}")
        with open(LOG, "a", encoding="utf-8") as log:
            log.write(json.dumps(request, ensure_ascii=False) + "\n")

        action = request.get("action")
        result = None
        if action == "version":
            result = 6
        elif action == "modelNames":
            result = ["AnkiMining"]
        elif action == "deckNames":
            result = ["Default", "Mining"]
        elif action == "modelFieldNames":
            # A note type made by an older version of the app: no field for the word audio.
            result = ["Expression", "Reading", "Meaning", "Sentence", "SentenceAudio", "Picture", "Frequency", "Pitch", "Source"]
        elif action == "modelFieldAdd":
            result = None
        elif action == "createModel":
            result = {"id": 1, "name": "AnkiMining"}
        elif action == "canAddNotes":
            result = [True for _ in request.get("params", {}).get("notes", [])]
        elif action == "addNote":
            NEXT_NOTE_ID[0] += 1
            result = NEXT_NOTE_ID[0]
        elif action == "storeMediaFile":
            result = request.get("params", {}).get("filename")

        body = json.dumps({"result": result, "error": None}).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args) -> None:
        pass


HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
