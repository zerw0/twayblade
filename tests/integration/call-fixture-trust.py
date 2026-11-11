"""Fixture CA validation and certificate-specific Chromium SPKI pin; no keychain writes."""
import base64
import hashlib
import json
from pathlib import Path
import ssl
import subprocess
import urllib.request


def trust():
    root = Path(__file__).resolve().parents[2] / 'test-data/local/integration'
    public = subprocess.run(['openssl', 'x509', '-in', str(root / 'tls.crt'), '-pubkey', '-noout'],
                            check=True, capture_output=True).stdout
    der = subprocess.run(['openssl', 'pkey', '-pubin', '-outform', 'DER'], input=public,
                         check=True, capture_output=True).stdout
    context = ssl.create_default_context(cafile=str(root / 'ca.crt'))
    with urllib.request.urlopen('https://127.0.0.1:18448/_matrix/client/versions', context=context, timeout=5) as response:
        assert response.status == 200
    return {'spki': base64.b64encode(hashlib.sha256(der).digest()).decode()}


if __name__ == '__main__':
    print(json.dumps(trust()))
