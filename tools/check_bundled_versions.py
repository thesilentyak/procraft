"""Checks that each plugin the app comes with whose packed file changed has a newer version:

    python tools/check_bundled_versions.py [--since REF]

The app puts a bundled plugin's file in place of the one a device has only when its version is
newer (see docs/plugins.md, "The plugins that come with the app"), so a file packed again with new
contents under the same version never reaches anyone who has the old one. This compares each
crates/core/bundled-plugins/*.procraft-plugin with the same file at REF (the last release's tag,
v-something, unless --since says another), and fails, naming each, for one whose bytes differ while
its version is no newer. A plugin the app didn't come with at REF is new to everyone and passes.
Run it before a release, and after packing with --bundle.
"""

import argparse
import hashlib
import io
import json
import os
import subprocess
import sys
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUNDLED = 'crates/core/bundled-plugins'
MANIFEST = 'craft-extension.json'


def git(*args, binary=False):
    """What `git args` prints (bytes with `binary`), from the repository's top; None if it fails."""
    done = subprocess.run(['git', *args], cwd=ROOT, capture_output=True)
    if done.returncode != 0:
        return None
    return done.stdout if binary else done.stdout.decode('utf-8').strip()


def last_release():
    """The newest release tag by its version (v0.0.7-beta, say), on whatever branch it was made."""
    tags = git('tag', '--list', 'v*', '--sort=-version:refname')
    if not tags:
        sys.exit('no release tag (v...) to compare with: pass --since')
    return tags.splitlines()[0]


def version_of(packed):
    """The version a packed plugin's manifest says."""
    with zipfile.ZipFile(io.BytesIO(packed)) as z:
        return json.loads(z.read(MANIFEST).decode('utf-8'))['version']


def version_key(version):
    """A version as numbers to compare by (1.2.10 after 1.2.9); what isn't a number counts as 0."""
    key = []
    for part in version.split('-')[0].split('.'):
        try:
            key.append(int(part))
        except ValueError:
            key.append(0)
    return tuple(key)


def stale(since):
    """Each bundled plugin whose packed file changed since `since` with no newer version: (file,
    version then, version now)."""
    found = []
    folder = os.path.join(ROOT, BUNDLED)
    for name in sorted(os.listdir(folder)):
        if not name.endswith('.procraft-plugin'):
            continue
        with open(os.path.join(folder, name), 'rb') as f:
            now = f.read()
        then = git('show', f'{since}:{BUNDLED}/{name}', binary=True)
        if then is None or hashlib.sha256(then).digest() == hashlib.sha256(now).digest():
            continue
        was, is_now = version_of(then), version_of(now)
        if version_key(is_now) <= version_key(was):
            found.append((name, was, is_now))
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--since', help='the tag or commit to compare with (the last release, unless given)')
    since = parser.parse_args().since or last_release()
    found = stale(since)
    for name, was, now in found:
        print(f'{name}: packed anew since {since} but still {now} (was {was}): give it a newer version')
    if found:
        sys.exit(1)
    print(f'every bundled plugin changed since {since} has a newer version')


if __name__ == '__main__':
    main()
