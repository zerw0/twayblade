"""Isolated Synapse fixture generation and registration; never print secrets."""
import datetime
import hashlib
import hmac
import ipaddress
import json
import os
from pathlib import Path
import secrets
import ssl
import sys
import urllib.request

ROOT = Path('/data')
FIXTURE = ROOT / 'fixture.json'
URL = 'https://127.0.0.1:18448'


def generate():
    from cryptography import x509
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import rsa
    from cryptography.x509.oid import NameOID, ExtendedKeyUsageOID

    if FIXTURE.exists() and (ROOT / 'ca.crt').exists():
        existing = x509.load_pem_x509_certificate((ROOT / 'tls.crt').read_bytes())
        if (existing.issuer != existing.subject and
                existing.not_valid_after_utc > datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(hours=1)):
            print('Existing isolated fixture retained.')
            return
    ROOT.mkdir(parents=True, exist_ok=True)
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, 'Matrix Client local test')])
    now = datetime.datetime.now(datetime.timezone.utc)
    ca_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    ca_name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, 'Matrix Client test CA')])
    ca_cert = (x509.CertificateBuilder().subject_name(ca_name).issuer_name(ca_name)
               .public_key(ca_key.public_key()).serial_number(x509.random_serial_number())
               .not_valid_before(now - datetime.timedelta(minutes=5))
               .not_valid_after(now + datetime.timedelta(days=7))
               .add_extension(x509.BasicConstraints(ca=True, path_length=0), critical=True)
               .add_extension(x509.KeyUsage(digital_signature=False, content_commitment=False,
                   key_encipherment=False, data_encipherment=False, key_agreement=False,
                   key_cert_sign=True, crl_sign=True, encipher_only=None, decipher_only=None), critical=True)
               .add_extension(x509.SubjectKeyIdentifier.from_public_key(ca_key.public_key()), critical=False)
               .sign(ca_key, hashes.SHA256()))
    cert = (x509.CertificateBuilder().subject_name(name).issuer_name(ca_cert.subject)
            .public_key(key.public_key()).serial_number(x509.random_serial_number())
            .not_valid_before(now - datetime.timedelta(minutes=5))
            .not_valid_after(now + datetime.timedelta(days=7))
            .add_extension(x509.SubjectAlternativeName([
                x509.IPAddress(ipaddress.ip_address('127.0.0.1')),
                x509.DNSName('localhost')]), critical=False)
            .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True)
            .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]), critical=False)
            .add_extension(x509.AuthorityKeyIdentifier.from_issuer_public_key(ca_key.public_key()), critical=False)
            .sign(ca_key, hashes.SHA256()))
    (ROOT / 'tls.key').write_bytes(key.private_bytes(serialization.Encoding.PEM,
        serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
    (ROOT / 'tls.key').chmod(0o600)
    (ROOT / 'tls.crt').write_bytes(cert.public_bytes(serialization.Encoding.PEM))
    (ROOT / 'ca.crt').write_bytes(ca_cert.public_bytes(serialization.Encoding.PEM))
    if FIXTURE.exists():
        print('Refreshed isolated TLS certificates; accounts and configuration retained.')
        return
    registration_secret = secrets.token_hex(32)
    (ROOT / 'homeserver.yaml').write_text(f'''server_name: matrixclient.test
public_baseurl: {URL}/
pid_file: /data/homeserver.pid
report_stats: false
listeners:
  - port: 8448
    type: http
    tls: true
    bind_addresses: ['0.0.0.0']
    resources:
      - names: [client]
        compress: false
tls_certificate_path: /data/tls.crt
tls_private_key_path: /data/tls.key
database:
  name: sqlite3
  args:
    database: /data/homeserver.db
media_store_path: /data/media
signing_key_path: /data/signing.key
log_config: /data/log.config
registration_shared_secret: {registration_secret}
macaroon_secret_key: {secrets.token_hex(32)}
form_secret: {secrets.token_hex(32)}
enable_registration: false
federation_domain_whitelist: []
url_preview_enabled: false
rc_message:
  per_second: 100
  burst_count: 1000
rc_login:
  address:
    per_second: 100
    burst_count: 1000
  account:
    per_second: 100
    burst_count: 1000
''')
    (ROOT / 'log.config').write_text('''version: 1
formatters:
  brief:
    format: '%(levelname)s %(name)s: %(message)s'
handlers:
  console:
    class: logging.StreamHandler
    formatter: brief
root:
  level: WARNING
  handlers: [console]
disable_existing_loggers: false
''')
    FIXTURE.write_text(json.dumps({'homeserver': URL, 'registration_secret': registration_secret,
        'alice': secrets.token_urlsafe(32), 'bob': secrets.token_urlsafe(32)}))
    for name in ['fixture.json', 'homeserver.yaml', 'tls.key']:
        (ROOT / name).chmod(0o600)
    print('Generated isolated TLS fixture. No host trust settings changed.')


def request(path, payload=None):
    # Only connect inside the isolated container. Trust exactly its test certificate.
    context = ssl.create_default_context(cafile=str(ROOT / 'ca.crt'))
    data = json.dumps(payload).encode() if payload is not None else None
    req = urllib.request.Request('https://127.0.0.1:8448' + path, data=data,
                                 headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, context=context, timeout=5) as response:
        return json.load(response)


def register():
    fixture = json.loads(FIXTURE.read_text())
    for user in ['alice', 'bob']:
        nonce = request('/_synapse/admin/v1/register')['nonce']
        digest = hmac.new(fixture['registration_secret'].encode(), digestmod=hashlib.sha1)
        for field in [nonce, user, fixture[user], 'notadmin']:
            if field != nonce:
                digest.update(b'\x00')
            digest.update(field.encode())
        try:
            request('/_synapse/admin/v1/register', {'nonce': nonce, 'username': user,
                'password': fixture[user], 'admin': False, 'mac': digest.hexdigest()})
        except urllib.error.HTTPError as error:
            if error.code != 400 or json.loads(error.read()).get('errcode') != 'M_USER_IN_USE':
                raise RuntimeError('Isolated test registration failed') from None
    print('Isolated test accounts ready.')


if __name__ == '__main__':
    try:
        action = sys.argv[1]
        if action == 'generate':
            generate()
        elif action == 'health':
            request('/_matrix/client/versions')
        elif action == 'register':
            register()
        else:
            raise ValueError('Unknown fixture command')
    except Exception:
        # Never dump HTTP response bodies, passwords or signing material.
        sys.exit('Local fixture operation failed; inspect configuration without sharing secrets.')
