"""LiveKit API evidence only. No UI, JWTs, room names or identities in output."""
import base64
import hashlib
import hmac
import json
from pathlib import Path
import ssl
import sys
import time
import urllib.request


def encoded(value):
    return base64.urlsafe_b64encode(json.dumps(value, separators=(',', ':')).encode()).rstrip(b'=')


def evidence(expected):
    root = Path(__file__).resolve().parents[2] / 'test-data/local/integration'
    def jwt(room=None):
        header = encoded({'alg': 'HS256', 'typ': 'JWT'})
        grants = {'roomList': True} if room is None else {'roomAdmin': True, 'room': room}
        payload = encoded({'iss': 'isolated-call-fixture', 'nbf': int(time.time()) - 5,
                           'exp': int(time.time()) + 30, 'video': grants})
        unsigned = header + b'.' + payload
        return (unsigned + b'.' + base64.urlsafe_b64encode(hmac.new(
            b'isolated-call-fixture-only-never-use-outside-tests', unsigned, hashlib.sha256).digest()).rstrip(b'=')).decode()

    context = ssl.create_default_context(cafile=str(root / 'ca.crt'))

    def api(method, body):
        req = urllib.request.Request('https://127.0.0.1:22451/twirp/livekit.RoomService/' + method,
            data=json.dumps(body).encode(), headers={'Content-Type': 'application/json',
            'Authorization': 'Bearer ' + jwt(body.get('room'))})
        with urllib.request.urlopen(req, context=context, timeout=5) as response:
            return json.load(response)

    rooms = api('ListRooms', {}).get('rooms', [])
    sizes = []
    matches = []
    for room in rooms:
        participants = api('ListParticipants', {'room': room['name']}).get('participants', [])
        identities = {p['identity'] for p in participants}
        sizes.append(len(identities))
        matches.append(len(identities.intersection(expected)))
        if len(identities) == 2 and identities == set(expected):
            return {'livekit_participants': 2}
    print('LiveKit counts: rooms=' + str(len(rooms)) + ' sizes=' + str(sizes) + ' expected_matches=' + str(matches), file=sys.stderr)
    raise RuntimeError('Expected LiveKit participant pair not present')


if __name__ == '__main__':
    try:
        print(json.dumps(evidence(json.load(sys.stdin))))
    except Exception as error:
        # urllib exceptions can contain URLs. Never print them or response data.
        print('LiveKit API participant assertion failed: ' + type(error).__name__ + ' status=' + str(getattr(error, 'code', 'none')), file=sys.stderr)
        sys.exit(1)
