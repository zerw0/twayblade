"""HTTP form driver for isolated MAS only; no URLs, credentials or bodies in output."""
import html.parser
import http.cookiejar
import json
from pathlib import Path
import ssl
import sys
import urllib.parse
import urllib.request

ORIGIN = 'https://127.0.0.1:19449'
STAGE = 'input'

class Form(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.action = None
        self.fields = {}
        self.buttons = []
        self.forms = []
        self.current = None
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == 'form':
            self.current = {'action': attrs.get('action', ''), 'class': attrs.get('class', ''), 'fields': {}, 'buttons': []}
        if self.current is None:
            return
        if tag == 'input' and attrs.get('name'):
            self.current['fields'][attrs['name']] = attrs.get('value', '')
        if tag == 'button' and attrs.get('name'):
            self.current['buttons'].append((attrs['name'], attrs.get('value', '')))
    def handle_endtag(self, tag):
        if tag == 'form' and self.current is not None:
            self.forms.append(self.current)
            self.current = None
    def feed(self, page):
        super().feed(page)
        # Ignore the navbar's sign-out form; submit the page's primary form.
        if self.forms:
            form = next((form for form in self.forms if 'cpd-form-root' in form['class']), self.forms[0])
            self.action, self.fields, self.buttons = form['action'], form['fields'], form['buttons']


def main():
    global STAGE
    data = json.load(sys.stdin)
    auth = urllib.parse.urlparse(data['url'])
    sso = data.get('sso', False)
    auth_origin = 'https://127.0.0.1:20448' if sso else ORIGIN
    if f'{auth.scheme}://{auth.netloc}' != auth_origin:
        raise ValueError('fixture issuer required')
    redirect = urllib.parse.parse_qs(auth.query)['redirectUrl' if sso else 'redirect_uri'][0]
    expected = urllib.parse.urlparse(redirect)
    if expected.hostname != '127.0.0.1' or expected.scheme != 'http' or expected.path != '/':
        raise ValueError('fixture callback required')
    class Redirects(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            target = urllib.parse.urlparse(newurl)
            if f'{target.scheme}://{target.netloc}' not in [ORIGIN, auth_origin] and (target.scheme, target.netloc, target.path) != (expected.scheme, expected.netloc, expected.path):
                raise ValueError('foreign redirect')
            return super().redirect_request(req, fp, code, msg, headers, newurl)
    context = ssl.create_default_context(cafile=Path(__file__).resolve().parents[2] / 'test-data/local/oauth/ca.crt')
    if sso:
        context.load_verify_locations(cafile=Path(__file__).resolve().parents[2] / 'test-data/local/sso/ca.crt')
    opener = urllib.request.build_opener(Redirects(), urllib.request.HTTPSHandler(context=context), urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
    def load(url, fields=None):
        body = None if fields is None else urllib.parse.urlencode(fields).encode()
        with opener.open(url, body, timeout=30) as response:
            page = response.read().decode()
            return response.url, page
    STAGE = 'registration-get'
    url, page = load(ORIGIN + '/register')
    form = Form(); form.feed(page)
    fields = form.fields
    fields.update(username=data['username'], password=data['password'], password_confirm=data['password'])
    STAGE = 'registration-post'
    url, page = load(urllib.parse.urljoin(url, form.action or ''), fields)
    for step in range(6):
        if not urllib.parse.urlparse(url).path.startswith('/register/'):
            break
        STAGE = f'registration-step-{step}'
        form = Form(); form.feed(page)
        if form.action is None:
            raise RuntimeError('registration step missing form')
        fields = form.fields
        for name in ['display_name', 'displayname']:
            if name in fields:
                fields[name] = 'Fixture user'
        for name, value in form.buttons:
            if value not in ['cancel', 'deny']:
                fields[name] = value
                break
        url, page = load(urllib.parse.urljoin(url, form.action), fields)
    STAGE = 'authorization-get'
    url, page = load(data['url'])
    for step in range(8):
        STAGE = f'consent-{step}'
        if urllib.parse.urlparse(url).netloc == expected.netloc:
            return
        form = Form(); form.feed(page)
        if form.action is None:
            raise RuntimeError('no consent form')
        fields = form.fields
        if 'username' in fields:
            fields.update(username=data['username'], password=data['password'])
        # MAS consent uses the affirmative submit button.
        for name, value in form.buttons:
            if value not in ['deny', 'reject', 'cancel']:
                fields[name] = value
                break
        url, page = load(urllib.parse.urljoin(url, form.action), fields)
    raise RuntimeError('form limit exceeded')

if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print('MAS test stage:', STAGE, 'error kind:', type(error).__name__, 'HTTP status:', getattr(error, 'code', 'none'), file=sys.stderr)
        sys.exit('Isolated MAS browser form driver failed (sensitive details suppressed).')
