#!/usr/bin/env python3
"""Loopback-only TLS fault proxy for generated Synapse accounts. No HTTP logging.

The control file contains a mode (normal/transient/limited/stall/drop) and an optional
route (encryption/sync). Stats contain counts only, never paths or payloads.
"""
import http.server
import json
from pathlib import Path
import ssl
import sys
import threading
import time
import urllib.error
import urllib.request

fixture, work = map(Path, sys.argv[1:])
lock = threading.Lock()
stats = {"encryption": 0, "faults": 0, "sync": 0}
upstream_tls = ssl.create_default_context(cafile=str(fixture / "ca.crt"))


def plan():
    try:
        return json.loads((work / "control.json").read_text())
    except (FileNotFoundError, ValueError):
        return {"mode": "normal"}


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_GET(self):
        self.forward()

    def do_POST(self):
        self.forward()

    def do_PUT(self):
        self.forward()

    def do_DELETE(self):
        self.forward()

    def forward(self):
        route = "encryption" if "/state/m.room.encryption" in self.path else "sync" if "/sync" in self.path else "other"
        with lock:
            control = plan()
            if route in stats:
                stats[route] += 1
            mode = control.get("mode", "normal") if route == control.get("route", "encryption") else "normal"
            if mode != "normal":
                stats["faults"] += 1
            (work / "stats.json").write_text(json.dumps(stats))
        if mode in ("transient", "limited"):
            payload = (b'{"errcode":"M_LIMIT_EXCEEDED","retry_after_ms":2000,"error":"isolated limit"}'
                       if mode == "limited" else b'{"errcode":"M_UNKNOWN","error":"isolated transient response"}')
            self.send_response(429 if mode == "limited" else 503)
            self.send_header("Content-Length", str(len(payload)))
            self.send_header("Retry-After", "2" if mode == "limited" else "60")
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(payload)
            return
        if mode == "drop":
            self.close_connection = True
            return
        if mode == "stall":
            deadline = time.monotonic() + 45
            while time.monotonic() < deadline and plan().get("mode") == "stall":
                time.sleep(0.1)
        try:
            body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            headers = {k: v for k, v in self.headers.items() if k.lower() not in ("host", "connection", "accept-encoding")}
            request = urllib.request.Request("https://127.0.0.1:18448" + self.path, data=body if body else None, headers=headers, method=self.command)
            try:
                response = urllib.request.urlopen(request, context=upstream_tls, timeout=45)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                payload = response.read()
                self.send_response(response.status)
                for key in ("Content-Type", "Retry-After"):
                    if value := response.headers.get(key):
                        self.send_header(key, value)
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
        except (OSError, ValueError):
            self.close_connection = True


class Server(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def handle_error(self, *_args):
        pass  # No stack/error/URL/body logging, including disconnects.


server = Server(("127.0.0.1", 0), Handler)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain(str(fixture / "tls.crt"), str(fixture / "tls.key"))
server.socket = context.wrap_socket(server.socket, server_side=True)
(work / "port").write_text(str(server.server_port))
server.serve_forever()
