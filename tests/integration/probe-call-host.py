"""Credential-free WebKit API probe; opens one window, never requests media.

Run with the packaged helper executable path to compare bundle declarations.
This is runtime evidence only, not Matrix integration or call acceptance.
"""
import json
import select
import struct
import subprocess
import sys
import time

executable = sys.argv[1] if len(sys.argv) > 1 else 'target/debug/twayblade-call-host'
process = subprocess.Popen([executable], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                           stderr=subprocess.DEVNULL)
try:
    url = ('twayblade-call://localhost/index.html#?widgetId=runtime-probe'
           '&parentUrl=twayblade-call%3A%2F%2Flocalhost&roomId=!probe%3Atwayblade.test'
           '&userId=%40probe%3Atwayblade.test&deviceId=PROBE'
           '&baseUrl=https%3A%2F%2F127.0.0.1%3A18448&perParticipantE2EE=true')
    value = {'type': 'configure', 'data': {'url': url, 'jwt_service': 'https://127.0.0.1:22449/jwt'}}
    data = json.dumps(value).encode()
    process.stdin.write(struct.pack('>I', len(data)) + data)
    process.stdin.flush()
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if select.select([process.stdout], [], [], 1)[0]:
            header = process.stdout.read(4)
            if len(header) != 4:
                raise SystemExit('Runtime exited before environment report')
            length = struct.unpack('>I', header)[0]
            if length > 256 * 1024:
                raise SystemExit('Runtime frame cap exceeded')
            frame = json.loads(process.stdout.read(length))
            if frame['type'] in ['ready', 'environment', 'failed']:
                print(json.dumps(frame), flush=True)
            if frame['type'] == 'environment':
                break
    else:
        raise SystemExit('Runtime environment report timed out')
finally:
    process.terminate()
    process.wait(timeout=5)
