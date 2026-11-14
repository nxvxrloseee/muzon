#!/usr/bin/env python3
"""Validate portable launcher argument handling without running the player."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class LauncherTest(unittest.TestCase):
    def test_profile_selection_and_argument_boundaries(self):
        with tempfile.TemporaryDirectory(prefix="muzon launcher ") as scratch:
            root = Path(scratch)
            (root / "bin").mkdir()
            (root / "libexec").mkdir()
            launcher = root / "bin/muzon-native"
            shutil.copyfile(Path(__file__).with_name("muzon-native"), launcher)
            launcher.chmod(0o755)
            stub = root / "libexec/muzon-native"
            stub.write_text('#!/usr/bin/env python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n')
            stub.chmod(0o755)
            env = dict(os.environ, HOME=str(root / "home"), XDG_DATA_HOME=str(root / "data with spaces"))

            def run(*args):
                result = subprocess.run([str(launcher), *args], env=env, check=True, capture_output=True, text=True)
                return json.loads(result.stdout)

            self.assertEqual(run(), ["--profile", str(root / "data with spaces/muzon-native")])
            self.assertEqual(run("--profile", "/tmp/a profile"), ["--profile", "/tmp/a profile"])
            self.assertEqual(run("--demo"), ["--demo"])
            self.assertEqual(run("--help"), ["--help"])
            env.pop("XDG_DATA_HOME")
            self.assertEqual(run(), ["--profile", str(root / "home/.local/share/muzon-native")])


if __name__ == "__main__":
    unittest.main()
