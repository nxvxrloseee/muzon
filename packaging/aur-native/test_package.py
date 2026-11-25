#!/usr/bin/env python3
"""Inspect an owned makepkg artifact without installing or opening a profile."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main():
    if len(sys.argv) != 2:
        raise SystemExit('Usage: python test_package.py /path/to/muzon-native.pkg.tar.zst')
    package = Path(sys.argv[1]).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='muzon-package-check-') as scratch:
        root = Path(scratch)
        subprocess.run(['bsdtar', '-xf', str(package), '-C', str(root)], check=True)
        required = [
            'usr/bin/muzon-native', 'usr/lib/muzon-native/muzon-native',
            'usr/share/applications/muzon-native.desktop',
            'usr/share/icons/hicolor/128x128/apps/muzon-native.png',
            'usr/share/icons/hicolor/256x256/apps/muzon-native.png',
            'usr/share/muzon-native/tools/import_profile.py',
            'usr/share/licenses/muzon-native/LICENSE',
        ]
        for name in required:
            assert (root / name).is_file(), name
        assert not (root / 'usr/bin/muzon').exists(), 'conflicts with the old shell'
        metadata = (root / '.PKGINFO').read_text()
        assert 'pkgname = muzon-native\n' in metadata
        assert not any('depend = ' + name + '\n' in metadata for name in ['nodejs', 'npm', 'webkit2gtk-4.1', 'tauri'])
        binary = root / 'usr/lib/muzon-native/muzon-native'
        launcher = root / 'usr/bin/muzon-native'
        env = dict(os.environ, HOME=str(root / 'home'), XDG_DATA_HOME=str(root / 'data with spaces'))
        # --help exits before GStreamer/core/profile initialization.
        help_result = subprocess.run([str(launcher), '--help'], env=env, check=True, capture_output=True, text=True)
        assert 'muzon-native --profile' in help_result.stdout
        desktop = root / 'usr/share/applications/muzon-native.desktop'
        assert 'StartupWMClass=muzon-native\n' in desktop.read_text()
        if shutil.which('desktop-file-validate'):
            subprocess.run(['desktop-file-validate', str(desktop)], check=True)
        # Replace only this extracted copy to verify paths and argv boundaries.
        binary.write_text('#!/usr/bin/env python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n')
        binary.chmod(0o755)
        def run(*args):
            result = subprocess.run([str(launcher), *args], env=env, check=True, capture_output=True, text=True)
            return json.loads(result.stdout)
        assert run() == ['--profile', str(root / 'data with spaces/muzon-native')]
        assert run('--demo') == ['--demo']
        assert run('--profile', '/tmp/explicit profile') == ['--profile', '/tmp/explicit profile']
        assert not (root / 'data with spaces').exists(), 'QA opened a real profile'
    print('PASS: package layout, executable/launcher, desktop entry, isolated default profile, arguments, coexistence')


if __name__ == '__main__':
    main()
