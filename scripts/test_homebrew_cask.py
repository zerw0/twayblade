"""Tests for homebrew-cask.py: `python3 scripts/test_homebrew_cask.py`."""
import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("homebrew_cask", Path(__file__).with_name("homebrew-cask.py"))
cask = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cask)

HASH = "ab" * 32


class HomebrewCask(unittest.TestCase):
    def template(self):
        return cask.TEMPLATE.read_text()

    def test_fills_version_and_the_dmg_checksum(self):
        sums = f"{'cd' * 32}  Twayblade-0.2.0-x86_64.msi\n{HASH}  Twayblade-0.2.0-arm64.dmg\n"
        out = cask.render(self.template(), "0.2.0", sums)
        self.assertIn('version "0.2.0"', out)
        self.assertIn(f'sha256 "{HASH}"', out)
        self.assertNotIn('"VERSION"', out)
        self.assertNotIn('"SHA256"', out)

    def test_accepts_binary_mode_lines_and_pre_releases(self):
        out = cask.render(self.template(), "0.2.0-rc.1", f"{HASH} *Twayblade-0.2.0-rc.1-arm64.dmg\n")
        self.assertIn('version "0.2.0-rc.1"', out)

    def test_refuses_missing_dmg_bad_hash_and_bad_versions(self):
        with self.assertRaises(ValueError):
            cask.render(self.template(), "0.2.0", f"{HASH}  Twayblade-0.2.0-x86_64.msi\n")
        with self.assertRaises(ValueError):
            cask.render(self.template(), "0.2.0", "short  Twayblade-0.2.0-arm64.dmg\n")
        for version in ["v0.2.0", "0.2", "0.2.0; rm -rf /", ""]:
            with self.assertRaises(ValueError):
                cask.render(self.template(), version, f"{HASH}  Twayblade-{version}-arm64.dmg\n")


if __name__ == "__main__":
    unittest.main()
