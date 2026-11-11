"""Fetch exactly the pinned embedded package; verify archive and every asset.

Run inside nix develop. Output is ignored build data, never source assets.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile
import urllib.request

VERSION = '0.17.0'
SHA256 = 'ba0f3f3b04b62577587faed603c3768465b1f9db33b15583473265531ca00121'
URL = f'https://registry.npmjs.org/@element-hq/element-call-embedded/-/element-call-embedded-{VERSION}.tgz'
parser = argparse.ArgumentParser()
parser.add_argument('--archive', type=Path)
parser.add_argument('--dest', type=Path, default=Path('target/element-call/package/dist'))
args = parser.parse_args()
if args.archive:
    data = args.archive.read_bytes()
else:
    with urllib.request.urlopen(URL, timeout=60) as response:
        data = response.read(64 * 1024 * 1024)
if hashlib.sha256(data).hexdigest() != SHA256:
    raise SystemExit('Embedded archive checksum mismatch')
manifest = json.loads(Path(__file__).with_name('assets.manifest.json').read_text())
with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
    for path, expected in manifest.items():
        member = archive.getmember('package/dist/' + path)
        if not member.isfile() or member.size > 64 * 1024 * 1024:
            raise SystemExit('Invalid embedded archive member')
        content = archive.extractfile(member).read()
        if hashlib.sha256(content).hexdigest() != expected:
            raise SystemExit('Embedded asset checksum mismatch')
        destination = args.dest / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(content)
    for name in ['LICENSE-AGPL-3.0', 'LICENSE-COMMERCIAL']:
        content = archive.extractfile('package/' + name).read()
        (args.dest / name).write_bytes(content)
print(f'Prepared verified Element Call embedded {VERSION}: {len(manifest)} assets')
