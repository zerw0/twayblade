"""Tests for check-release-version.py: `python3 scripts/test_check_release_version.py`."""
import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("check-release-version.py")
spec = importlib.util.spec_from_file_location("check_release_version", SCRIPT)
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


def run(version):
    return subprocess.run([sys.executable, str(SCRIPT), version], capture_output=True, text=True)


class ReleaseVersion(unittest.TestCase):
    def test_the_workspace_version_passes(self):
        version = check.workspace_version(SCRIPT.parent.parent)
        self.assertEqual(run(version).returncode, 0)

    def test_a_tag_that_differs_from_cargo_fails(self):
        version = check.workspace_version(SCRIPT.parent.parent)
        result = run(version + "-rc.99")
        self.assertEqual(result.returncode, 1)
        self.assertIn("does not match", result.stderr)


if __name__ == "__main__":
    unittest.main()
