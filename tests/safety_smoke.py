#!/usr/bin/env python3
"""Account-free CLI/real-PTY safety smoke. Stdlib only; Python 3.9+.

Usage: python3 tests/safety_smoke.py /absolute/textorium /new/receipt-directory
Receipt directories must not exist. No installed binary or real config is used.
The small VT cell projection handles the CSI output used by ratatui; raw ANSI
is retained separately. This is interaction evidence, not native font QA.
"""
import codecs
import errno
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import re
import select
import struct
import subprocess
import sys
import termios
import time

ROWS, COLS = 42, 180
IGNORED = ['.git', 'node_modules', 'vendor', 'target', 'dist', 'public', '_site', '_output', '.next', '.astro', '.textorium']

class Screen:
    def __init__(self):
        self.cells = [[' '] * COLS for _ in range(ROWS)]
        self.x = self.y = 0
        self.pending = ''
        self.decoder = codecs.getincrementaldecoder('utf-8')('replace')

    def feed(self, data):
        text = self.pending + self.decoder.decode(data)
        self.pending = ''
        i = 0
        while i < len(text):
            ch = text[i]
            if ch == '\x1b':
                if i + 1 == len(text): self.pending = text[i:]; break
                if text[i + 1] == '[':
                    m = re.match(r'\x1b\[([0-9;?]*)([ -/]*)([@-~])', text[i:])
                    if not m: self.pending = text[i:]; break
                    raw, _, command = m.groups()
                    private = raw.startswith('?')
                    args = [int(v or 0) for v in raw.lstrip('?').split(';')]
                    n = args[0] or 1
                    if command in 'Hf':
                        self.y = min(ROWS - 1, max(0, n - 1))
                        self.x = min(COLS - 1, max(0, (args[1] if len(args) > 1 else 1) - 1))
                    elif command == 'G': self.x = min(COLS - 1, n - 1)
                    elif command == 'd': self.y = min(ROWS - 1, n - 1)
                    elif command == 'A': self.y = max(0, self.y - n)
                    elif command == 'B': self.y = min(ROWS - 1, self.y + n)
                    elif command == 'C': self.x = min(COLS - 1, self.x + n)
                    elif command == 'D': self.x = max(0, self.x - n)
                    elif command == 'J' and args[0] in (2, 3): self.cells = [[' '] * COLS for _ in range(ROWS)]
                    elif command == 'K':
                        start, end = (0, COLS) if args[0] == 2 else ((0, self.x + 1) if args[0] == 1 else (self.x, COLS))
                        self.cells[self.y][start:end] = [' '] * (end - start)
                    elif command == 'h' and private and 1049 in args:
                        self.cells = [[' '] * COLS for _ in range(ROWS)]
                        self.x = self.y = 0
                    i += len(m.group(0)); continue
                if text[i + 1] == ']':
                    end = text.find('\x07', i + 2)
                    if end < 0: self.pending = text[i:]; break
                    i = end + 1; continue
                i += 2; continue
            if ch == '\r': self.x = 0
            elif ch == '\n': self.y = min(ROWS - 1, self.y + 1)
            elif ch == '\b': self.x = max(0, self.x - 1)
            elif ord(ch) >= 32:
                if self.x < COLS:
                    self.cells[self.y][self.x] = ch
                    self.x += 1
            i += 1

    def text(self): return '\n'.join(''.join(row) for row in self.cells)

class Session:
    def __init__(self, binary, folder, source):
        self.folder = folder
        self.folder.mkdir()
        self.home = folder / 'home'
        self.site = folder / 'site'
        self.path = self.site / 'content' / 'post.md'
        self.path.parent.mkdir(parents=True)
        (self.site / 'hugo.toml').write_text('')
        self.path.write_bytes(source)
        self.home.mkdir()
        self.env = {'HOME': str(self.home), 'XDG_CONFIG_HOME': str(self.home / '.config'), 'TMPDIR': str(folder), 'PATH': '/opt/homebrew/bin:/usr/bin:/bin', 'TERM': 'xterm-256color', 'LC_ALL': 'en_US.UTF-8'}
        setup = subprocess.run([str(binary), 'use', str(self.site)], env=self.env, capture_output=True, text=True, timeout=10)
        (folder / 'setup.log').write_text(setup.stdout + setup.stderr)
        if setup.returncode: raise RuntimeError('CLI use failed')
        self.fd, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', ROWS, COLS, 0, 0))
        self.child = subprocess.Popen([str(binary)], env=self.env, cwd=self.site, stdin=slave, stdout=slave, stderr=slave, close_fds=True, start_new_session=True)
        os.close(slave)
        self.screen = Screen()
        self.raw = bytearray()
        self.capture = 0
        self.wait('Original')
        self.snapshot('startup')

    def drain(self, seconds=0.15):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            ready, _, _ = select.select([self.fd], [], [], max(0, deadline - time.monotonic()))
            if not ready: break
            try: data = os.read(self.fd, 65536)
            except OSError as e:
                if e.errno == errno.EIO: break
                raise
            if not data: break
            self.raw.extend(data)
            self.screen.feed(data)

    def wait(self, text, timeout=5):
        deadline = time.monotonic() + timeout
        while text not in self.screen.text() and time.monotonic() < deadline and self.child.poll() is None:
            self.drain(0.1)
        if text not in self.screen.text(): raise RuntimeError('PTY state not reached: ' + text)

    def key(self, data):
        try:
            os.write(self.fd, data)
        except OSError as error:
            # macOS may close the slave before poll() observes the clean child exit.
            if error.errno != errno.EIO: raise
            self.child.wait(timeout=1)
        self.drain()

    def snapshot(self, label):
        self.drain()
        self.capture += 1
        (self.folder / ('%02d-%s.screen.txt' % (self.capture, label))).write_text(self.screen.text())
        (self.folder / 'terminal.ansi').write_bytes(self.raw)
        return self.screen.text()

    def close(self):
        if self.child.poll() is None:
            self.key(b'q')
            if self.child.poll() is None: self.key(b'q')
        deadline = time.monotonic() + 5
        while self.child.poll() is None and time.monotonic() < deadline: self.drain(0.1)
        if self.child.poll() is None:
            self.child.terminate()
            self.drain(0.5)
            if self.child.poll() is None: self.child.kill()
        self.child.wait(timeout=5)
        self.drain()
        (self.folder / 'terminal.ansi').write_bytes(self.raw)
        os.close(self.fd)
        return self.child.returncode


def main():
    binary = Path(sys.argv[1]).resolve(strict=True)
    output = Path(sys.argv[2]).absolute()
    output.mkdir(parents=True, exist_ok=False)
    checks = []
    def check(name, passed): checks.append({'name': name, 'passed': bool(passed)})
    source = b'---\r\ntitle: Original\r\ndraft: false\r\n---\r\n\r\n\r\n"prose" -- ...\r\n~~~rust\r\n"raw" -- ...\r\n~~~\r\n\r\n'
    def scenario(name, action, fixture_source=source):
        session = None
        try:
            session = Session(binary, output / name, fixture_source)
            action(session)
        except Exception as error:
            check(name + ': orchestration', False)
            (output / (name + '-error.txt')).write_text(repr(error))
        finally:
            if session is not None:
                session.snapshot('final')
                try:
                    check(name + ': clean exit', session.close() == 0)
                except Exception as error:
                    check(name + ': cleanup', False)
                    (output / (name + '-cleanup-error.txt')).write_text(repr(error))
            (output / 'summary.json').write_text(json.dumps({'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'checks': checks, 'passed': all(c['passed'] for c in checks)}, indent=2) + '\n')

    def metadata(s):
        s.key(b'\t')  # sorted draft then title
        s.key(b'j\r')
        s.key(b'\x7f' * len('Original') + b'Unsaved\r')
        screen = s.snapshot('metadata-edited')
        check('metadata staged', 'Unsaved' in screen and s.path.read_bytes() == source)
        s.key(b'r')
        screen = s.snapshot('refresh-refused')
        check('dirty metadata refresh refuses and retains', 'Refresh refused' in screen and 'Unsaved' in screen)
        s.key(b'\x1b')
        check('cancel retains dirty metadata', 'Unsaved' in s.snapshot('cancel'))
        s.key(b'\x13')
        saved = s.path.read_bytes()
        check('metadata saved title', b'title: Unsaved' in saved)
        check('metadata save exact body bytes', saved.split(b'---', 2)[2] == source.split(b'---', 2)[2])
        s.key(b'r')
        check('clean refresh', 'Refreshed' in s.snapshot('clean-refresh'))

    def body(s):
        s.key(b'\t\tQ')
        check('body edit staged', s.path.read_bytes() == source)
        s.key(b'r')
        screen = s.snapshot('body-refresh-refused')
        check('dirty body refresh refuses', 'Refresh refused' in screen)
        s.key(b'\x1b')
        s.key(b'\x13')
        saved = s.path.read_bytes()
        expected = source.replace(b'"prose" -- ...', '“prose” — …'.encode())
        check('body edits retained after refresh/cancel', saved == expected)
        check('tilde fenced code literal', b'"raw" -- ...' in saved)

    def conflict(s):
        s.key(b'\t\tQ')
        external = b'external bytes\r\n'
        s.path.write_bytes(external)
        s.key(b'\x13')
        screen = s.snapshot('external-save-refused')
        check('external bytes retained', s.path.read_bytes() == external)
        check('external conflict visibly refused', 'externally' in screen and 'error' in screen)
        check('external conflict retains unsaved edit', 'unsaved' in screen)

    def batch(s):
        s.key(b'\t\tQ') # preexisting dirty body
        s.key(b'\t bjjj\r')
        s.snapshot('batch-confirmation')
        s.key(b'y')
        screen = s.snapshot('batch-staged')
        check('batch staged without disk write', s.path.read_bytes() == source and 'staged' in screen)
        s.key(b'u')
        screen = s.snapshot('batch-undone')
        check('undo before save does not write', s.path.read_bytes() == source)
        s.key(b'\x13')
        check('undo retains preexisting dirty body', s.path.read_bytes() == source.replace(b'"prose" -- ...', '“prose” — …'.encode()))
        # repeat batch, explicitly save, then stage inverse and save it separately
        s.key(b' bjjj\r')
        s.key(b'y\x13')
        saved = s.path.read_bytes()
        check('batch explicit save', b'draft: true' in saved)
        s.key(b'u')
        check('undo after save is in memory', s.path.read_bytes() == saved)
        s.key(b'\x13')
        check('undo inverse explicit save', b'draft: false' in s.path.read_bytes())

    scenario('metadata-refresh-fidelity', metadata)
    scenario('body-refresh-code', body)
    scenario('external-conflict', conflict)
    scenario('batch-undo', batch)

    # Qualify code-significant context through real Q / Ctrl+S / quit keys.
    def literal_code(s):
        original = s.path.read_bytes()
        s.key(b'\t\tQ')
        check(s.folder.name + ': staged', s.path.read_bytes() == original)
        s.key(b'\x13')
        expected = original.replace(b'"prose" -- ...', '“prose” — …'.encode())
        check(s.folder.name + ': exact saved code and prose bytes', s.path.read_bytes() == expected)
        check(s.folder.name + ': visible save', 'Saved' in s.snapshot('code-preserved-save'))

    for name, header in [('first-indented-yaml', b'---\r\ntitle: Original\r\n---'), ('first-indented-toml', b'+++\r\ntitle = "Original"\r\n+++')]:
        scenario(name, literal_code, header + b'\r\n\r\n    "raw" -- ...\r\n\r\n"prose" -- ...  \r\n\r\n')
    scenario('tab-blockquote-fence', literal_code, b'---\ntitle: Original\n---\n\n>\t~~~rust\n>\t"raw" -- ...\n>\t~~~\n\n"prose" -- ...\n')

    cases = json.loads((Path(__file__).parent / 'fixtures' / 'discovery.json').read_text())
    discovery = output / 'discovery'
    discovery.mkdir()
    for case in cases:
        site = discovery / case['name']
        site.mkdir()
        home = site / 'home'
        home.mkdir()
        if 'marker' in case: (site / case['marker']).write_text('')
        if 'package' in case: (site / 'package.json').write_text(json.dumps(case['package']))
        for relative in case['posts'] + case.get('excluded', []):
            path = site / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('---\ntitle: Fixture\n---\nbody\n')
        root = site / case['root']
        for ignored in IGNORED:
            path = root / ignored / 'excluded.md'
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('excluded')
        env = {'HOME': str(home), 'TMPDIR': str(discovery), 'PATH': '/opt/homebrew/bin:/usr/bin:/bin', 'LC_ALL': 'en_US.UTF-8'}
        use = subprocess.run([str(binary), 'use', str(site)], env=env, capture_output=True, text=True, timeout=10)
        listing = subprocess.run([str(binary), 'list', '--json'], env=env, capture_output=True, text=True, timeout=10)
        (site / 'use.log').write_text(use.stdout + use.stderr)
        (site / 'list.json').write_text(listing.stdout + listing.stderr)
        check(case['name'] + ': CLI exits', use.returncode == listing.returncode == 0)
        try:
            posts = json.loads(listing.stdout)
            actual = sorted(str(Path(post['path']).relative_to(site.resolve())) for post in posts)
            check(case['name'] + ': exact discovered paths', actual == sorted(case['posts']))
        except Exception as error:
            check(case['name'] + ': parse output', False)
            (site / 'parse-error.txt').write_text(repr(error))
    summary = {'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'checks': checks, 'passed': bool(checks) and all(c['passed'] for c in checks)}
    (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))
    return 0 if summary['passed'] else 1

if __name__ == '__main__': sys.exit(main())
