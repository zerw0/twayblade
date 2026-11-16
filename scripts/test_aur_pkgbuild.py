"""Tests for aur-pkgbuild.py: `python3 scripts/test_aur_pkgbuild.py`."""
import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("aur_pkgbuild", Path(__file__).with_name("aur-pkgbuild.py"))
aur = importlib.util.module_from_spec(spec)
spec.loader.exec_module(aur)

HASH = "ab" * 32


class AurPkgbuild(unittest.TestCase):
    def template(self):
        return aur.TEMPLATE.read_text()

    def test_pre_releases_get_an_arch_version_and_keep_the_tag(self):
        sums = f"{'cd' * 32}  Twayblade-0.1.1-rc.1-arm64.dmg\n{HASH}  twayblade-linux-x86_64.tar.gz\n"
        out = aur.render(self.template(), "0.1.1-rc.1", sums)
        self.assertIn("pkgver=0.1.1rc.1\n", out)
        self.assertIn("_tag=0.1.1-rc.1\n", out)
        self.assertIn(f"sha256sums=('{HASH}')", out)
        self.assertIn("releases/download/v${_tag}/", out)
        self.assertNotIn("pkgver=PKGVER", out)

    def test_stable_versions_stay_as_they_are(self):
        out = aur.render(self.template(), "1.0.0", f"{HASH} *twayblade-linux-x86_64.tar.gz\n")
        self.assertIn("pkgver=1.0.0\n", out)

    def test_refuses_missing_tarball_bad_hash_and_bad_versions(self):
        with self.assertRaises(ValueError):
            aur.render(self.template(), "1.0.0", f"{HASH}  Twayblade-1.0.0-arm64.dmg\n")
        with self.assertRaises(ValueError):
            aur.render(self.template(), "1.0.0", "xyz  twayblade-linux-x86_64.tar.gz\n")
        for version in ["v1.0.0", "1.0", "1.0.0-rc 1", "1.0.0;rm"]:
            with self.assertRaises(ValueError):
                aur.render(self.template(), version, f"{HASH}  twayblade-linux-x86_64.tar.gz\n")


if __name__ == "__main__":
    unittest.main()
