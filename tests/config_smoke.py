#!/usr/bin/env python3
"""Synthetic config CLI/PTY acceptance, Python 3.9+, stdlib only.

Usage: python3 tests/config_smoke.py /absolute/binary /new/receipts [--negative]
--negative runs the same comma and missing-root assertions against an old binary.
No installed binary, real HOME, GUI editor, or real site is used.
"""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import struct
import subprocess
import sys
import termios

import safety_smoke as vt


class Session(vt.Session):
    def __init__(self, binary, folder, config='legacy', size=(120, 40), editor=None):
        self.folder = folder
        folder.mkdir()
        self.home = folder / 'home'
        self.site = folder / 'site'
        self.path = self.site / 'content' / 'post.md'
        self.path.parent.mkdir(parents=True)
        self.path.write_text('---\ntitle: Original\ndraft: false\n---\n"prose" -- ...\n')
        self.original = self.path.read_bytes()
        self.config_path = self.home / '.config/textorium/config.json'
        self.config_path.parent.mkdir(parents=True)
        self.env = {'HOME': str(self.home), 'PATH': '/opt/homebrew/bin:/usr/bin:/bin',
                    'TMPDIR': str(folder), 'TERM': 'xterm-256color', 'LC_ALL': 'en_US.UTF-8'}
        self.script = folder / 'editor script.py'
        self.script.write_text('''import json, os, sys, termios
from pathlib import Path
root = Path(__file__).parent
path = Path(sys.argv[-1])
(root / 'editor-log.json').write_text(json.dumps({'argv':sys.argv[1:], 'tty':os.isatty(0), 'canonical':bool(termios.tcgetattr(0)[3] & termios.ICANON), 'before':path.read_text()}))
if (root / 'replacement').exists(): path.write_bytes((root / 'replacement').read_bytes())
if (root / 'exit-code').exists(): sys.exit(int((root / 'exit-code').read_text()))
''')
        self.command = '/usr/bin/python3 "{}" --wait "two words" \'$HOME;$(touch nope)\''.format(self.script)
        self.env['VISUAL'] = self.command
        self.env['EDITOR'] = '/absent-editor-must-not-win'
        self.value = {'site_name':'fixture', 'site_path':str(self.site), 'content_dir':'content', 'ssg':'hugo', 'unknown':{'keep':True}}
        if editor == 'configured':
            self.value['editor'] = self.command
            self.env['VISUAL'] = '/absent-visual-must-not-win'
        elif editor is not None: self.value['editor'] = editor
        if config == 'stale': self.value['site_path'] = str(folder / 'missing-site')
        if config == 'missing-content': self.value['content_dir'] = 'missing-content'
        if config == 'empty':
            self.value['content_dir'] = 'empty'
            (self.site / 'empty').mkdir()
        if config == 'multi':
            self.value = {'sites':[{'name':'fixture', 'path':str(self.site), 'content_dir':'content', 'ssg':'hugo', 'unknown_site':42}], 'active_site':'fixture', 'unknown':42}
        if config == 'invalid': self.config_path.write_text('{broken')
        elif config != 'absent': self.config_path.write_text(json.dumps(self.value))
        self.before_config = self.config_path.read_bytes() if self.config_path.exists() else None
        vt.COLS, vt.ROWS = size
        self.fd, self.slave = pty.openpty()
        self.original_termios = termios.tcgetattr(self.slave)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', vt.ROWS, vt.COLS, 0, 0))
        self.child = subprocess.Popen([str(binary)], env=self.env, cwd=self.site, stdin=self.slave, stdout=self.slave, stderr=self.slave, close_fds=True, start_new_session=True)
        self.screen = vt.Screen()
        self.raw = bytearray()
        self.capture = 0
        self.drain(0.5)

    def snapshot(self, label):
        text = super().snapshot(label)
        (self.folder / ('%02d-%s.ansi' % (self.capture, label))).write_bytes(self.raw)
        return text

    def editor_log(self): return json.loads((self.folder / 'editor-log.json').read_text())
    def replace(self, value):
        (self.folder / 'replacement').write_text(value if isinstance(value, str) else json.dumps(value))
    def close(self):
        if self.child.poll() is None:
            self.key(b'\x1b')
            self.key(b'\x1b')
        code = super().close()
        self.restored = termios.tcgetattr(self.slave) == self.original_termios
        os.close(self.slave)
        return code


def main():
    binary = Path(sys.argv[1]).resolve(strict=True)
    output = Path(sys.argv[2]).absolute()
    output.mkdir(parents=True, exist_ok=False)
    negative = '--negative' in sys.argv[3:]
    checks = []
    def check(name, value): checks.append({'name':name, 'passed':bool(value)})
    def scenario(name, action, **kwargs):
        session = None
        try:
            session = Session(binary, output / name, **kwargs)
            session.wait('Posts')
            session.snapshot('startup')
            action(session)
        except Exception as error:
            check(name + ': orchestration', False)
            (output / (name + '-error.txt')).write_text(repr(error))
        finally:
            if session is not None:
                session.snapshot('final')
                check(name + ': clean exit', session.close() == 0)
                check(name + ': terminal restored on quit', session.restored)
                check(name + ': original content unchanged', session.path.read_bytes() == session.original)

    def comma(s):
        s.key(b',')
        screen = s.snapshot('comma')
        check('comma opens configuration', 'Configuration' in screen and 'e:edit r:reload' in screen)
        if not negative:
            s.wait('Configuration')
            check('active site displayed', 'Active site: fixture' in screen)
            check('site root displayed', 'Site root:' in screen)
            check('config path displayed', 'Config file:' in screen)
            check('preferred editor displayed', 'Editor:' in screen)
            s.key(b'\x1b')
            s.wait('Posts')

    scenario('comma', comma)
    # Missing-root list must error, not emit a misleading successful JSON array.
    cli = output / 'cli'
    cli.mkdir()
    home = cli / 'home'
    config = home / '.config/textorium/config.json'
    config.parent.mkdir(parents=True)
    value = {'site_name':'missing', 'site_path':str(cli / 'absent'), 'content_dir':'content', 'ssg':'hugo'}
    env = {'HOME':str(home), 'PATH':'/usr/bin:/bin', 'TMPDIR':str(cli)}
    def run_cli(name, args, ok, diagnostic=None):
        result = subprocess.run([str(binary)] + args, env=env, capture_output=True, text=True, timeout=10)
        (cli / (name + '.json')).write_text(json.dumps({'exit':result.returncode, 'stdout':result.stdout, 'stderr':result.stderr}, indent=2))
        check(name, (result.returncode == 0) == ok and (ok or not result.stdout) and (diagnostic is None or diagnostic in result.stderr))
        return result
    config.write_text(json.dumps(value))
    for args in [['list'], ['list', '--json']]: run_cli('missing-site-' + str(len(args)), args, False, 'Site path')
    if not negative:
        value['site_path'] = str(cli)
        config.write_text(json.dumps(value))
        run_cli('missing-content', ['list', '--json'], False, 'Content directory')
        (cli / 'content').mkdir()
        result = run_cli('valid-empty', ['list', '--json'], True)
        check('valid-empty is JSON array', json.loads(result.stdout) == [])
        config.write_text('{broken')
        run_cli('malformed-json', ['list', '--json'], False, 'parse')
        config.unlink()
        run_cli('unconfigured', ['list', '--json'], False, 'No site configured')

        def edit(s):
            s.wait(',:config ?:help')
            s.key(b',')
            s.wait('e:edit')
            changed = dict(s.value)
            if 'sites' in changed:
                changed['sites'] = [dict(changed['sites'][0], name='Reloaded')]
                changed['active_site'] = 'Reloaded'
            else:
                changed['site_name'] = 'Reloaded'
                changed['site_path'] = str(s.site)
                changed['content_dir'] = 'content'
            s.replace(changed)
            s.key(b'e')
            s.wait('Configuration reloaded')
            screen = s.snapshot('reloaded')
            check('edit reload active site', 'Active site: Reloaded' in screen)
            log = s.editor_log()
            check('quoted arguments literal and wait flag', log['argv'][:-1] == ['--wait', 'two words', '$HOME;$(touch nope)'])
            check('same-terminal cooked editor', log['tty'] and log['canonical'])
            check('config edited directly without serialization', s.config_path.read_bytes() == (s.folder / 'replacement').read_bytes())
            check('unknown field preserved', 'unknown' in json.loads(s.config_path.read_text()))
            check('raw terminal restored after editor', not termios.tcgetattr(s.slave)[3] & termios.ICANON)
            s.key(b'\x1b')
            s.wait('Original')
        scenario('legacy-edit', edit)
        scenario('multi-edit', edit, config='multi')
        scenario('configured-override', edit, editor='configured')
        scenario('stale-legacy-repair', edit, config='stale')
        scenario('missing-content-repair', edit, config='missing-content')

        def first_run(s):
            s.wait(',:config ?:help')
            s.key(b',')
            s.wait('e:edit')
            check('first run view does not create config', not s.config_path.exists())
            s.replace(dict(s.value, site_name='SetUp'))
            s.key(b'e')
            s.wait('Configuration reloaded')
            before = json.loads(s.editor_log()['before'])
            check('first-run scaffold no guessed path', before['site_path'] == '' and before['content_dir'] == 'content')
            s.key(b'\x1b')
            s.wait('Original')
        scenario('first-run', first_run, config='absent')
        scenario('malformed-startup-repair', edit, config='invalid')

        def invalid_reload(s):
            s.wait('Original')
            s.key(b',')
            s.replace('{broken')
            s.key(b'e')
            s.wait('Failed to parse')
            s.snapshot('invalid-json-retained')
            check('invalid JSON retained for re-edit', s.config_path.read_text() == '{broken')
            s.replace(dict(s.value, site_path=str(s.folder / 'missing')))
            s.key(b'e')
            s.wait('Site path not found')
            s.snapshot('invalid-path-retained')
            check('bad path never created', not (s.folder / 'missing').exists())
            s.key(b'\x1b')
            s.wait('Original')
            check('old working collection retained', 'Original' in s.snapshot('old-collection'))
            s.key(b',')
            s.replace(dict(s.value, site_name='Fixed'))
            s.key(b'e')
            s.wait('Active site: Fixed')
        scenario('invalid-reedit', invalid_reload)

        def failed_editor(s):
            s.wait('Original')
            s.key(b',')
            s.key(b'e')
            s.wait('Config change failed')
            check('failed editor diagnostic', 'Could not start' in s.snapshot('spawn-failure') or 'Invalid editor command' in s.screen.text())
            check('failed editor config unchanged', s.config_path.read_bytes() == s.before_config)
            check('failure returns raw mode', not termios.tcgetattr(s.slave)[3] & termios.ICANON)
            s.key(b'\x1b')
            s.wait('Original')
            # Subsequent real input remains responsive.
            s.key(b'f')
            s.wait('DRAFTS ONLY')
        scenario('spawn-failure', failed_editor, editor='/definitely-absent-editor --wait')
        scenario('bad-quoting', failed_editor, editor="editor 'unfinished")

        def nonzero(s):
            s.wait('Original')
            s.key(b',')
            s.replace(dict(s.value, site_name='MustNotReload'))
            (s.folder / 'exit-code').write_text('7')
            s.key(b'e')
            s.wait('Editor exited unsuccessfully')
            text = s.snapshot('nonzero-retained')
            check('nonzero never reloads edited config', 'Active site: fixture' in text and 'Active site: MustNotReload' not in text)
            check('nonzero disk edits retained', json.loads(s.config_path.read_text())['site_name'] == 'MustNotReload')
            check('nonzero returns raw mode', not termios.tcgetattr(s.slave)[3] & termios.ICANON)
        scenario('nonzero-write', nonzero)

        def dirty(s):
            s.wait('Original')
            s.key(b'\t\tQ')
            s.wait('Smart quotes')
            s.key(b',')
            s.key(b'e')
            s.wait('Unsaved edits retained')
            check('dirty config edit does not start editor', not (s.folder / 'editor-log.json').exists())
            s.key(b'r')
            s.wait('Unsaved edits retained')
            check('dirty config edit leaves disk untouched', s.config_path.read_bytes() == s.before_config)
            s.snapshot('dirty-refusal')
        scenario('dirty-guard', dirty)

        def empty(s):
            s.wait('Valid empty site')
            check('valid empty startup distinguished', 'Valid empty site' in s.snapshot('empty'))
        scenario('valid-empty-tui', empty, config='empty')

        def filters(s):
            s.wait('Original')
            s.key(b'/no-match\r')
            s.wait('No posts match')
            check('search zero results distinguished', 'No posts match' in s.snapshot('zero-search'))
            s.key(b'\x1b')
            s.wait('Original')
            s.key(b'f')
            s.wait('No draft posts found')
            check('draft zero results distinguished', 'No draft posts found' in s.snapshot('zero-drafts'))
        scenario('zero-filters', filters)

        def narrow(s):
            s.wait(',:config ?:help')
            s.key(b',')
            s.wait('Configuration')
            check('narrow fixed edit and escape actions', 'e:edit r:reload' in s.screen.text() and 'Esc:close' in s.screen.text())
            s.snapshot('narrow-top')
            # Paths include the caller's receipt root; do not assume a fixed wrapped height.
            for _ in range(100):
                if 'CLI alternative' in s.screen.text() or 'textorium sites' in s.screen.text():
                    break
                s.key(b'j')
            check('narrow scroll reaches setup details', 'CLI alternative' in s.screen.text() or 'textorium sites' in s.screen.text())
            s.snapshot('narrow-scrolled')
            s.key(b'?')
            s.wait('Help')
            check('narrow help advertises comma', 'Configuration' in s.screen.text())
            s.snapshot('narrow-help')
        scenario('narrow', narrow, size=(40, 12))

    receipt = {'binary':str(binary), 'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(), 'negative_control':negative, 'checks':checks,
               'passed':all(c['passed'] for c in checks), 'count':len(checks)}
    (output / 'receipt.json').write_text(json.dumps(receipt, indent=2))
    print(json.dumps(receipt, indent=2))
    return 0 if receipt['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
