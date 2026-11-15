"""Tests for release-notes.py: `python3 scripts/test_release_notes.py`."""
import importlib.util
import unittest
from datetime import date, datetime, timezone
from pathlib import Path

spec = importlib.util.spec_from_file_location("release_notes", Path(__file__).with_name("release-notes.py"))
notes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notes)


def pr(number, title, merged="2026-11-14T10:00:00Z", labels=(), body=""):
    return {"number": number, "title": title, "mergedAt": merged, "labels": [{"name": n} for n in labels], "body": body}


class ReleaseNotes(unittest.TestCase):
    def test_squash_suffix_is_dropped(self):
        self.assertEqual(notes.clean_title("Invite people to a room (#62)"), "Invite people to a room")

    def test_classification(self):
        self.assertEqual(notes.classify("Add a From filter to message search", []), "New")
        self.assertEqual(notes.classify("Stop the app aborting on quit", []), "Fixed")
        self.assertEqual(notes.classify("Call view: expanded layout", []), "Improved")
        self.assertEqual(notes.classify("Anything", ["bug"]), "Fixed")
        self.assertIsNone(notes.classify("Record the first live call", ["documentation"]))
        self.assertIsNone(notes.classify("docs: tidy", []))

    def test_gaps_come_from_not_verified_sections_only(self):
        body = "## What\nA thing.\n\n## Verified\n- It compiles.\n\n## Not verified\n- A real server, light mode. More words.\nVoiceOver.\n\n## Limits\n- Sixty people at most."
        self.assertEqual(notes.gaps(body), ["A real server, light mode.", "VoiceOver.", "Sixty people at most."])

    def test_footer_and_links_are_not_gaps(self):
        body = "## Not verified\n- VoiceOver.\n\nCloses #5\n\n\U0001F916 Generated with [Claude Code](https://claude.com/claude-code)"
        self.assertEqual(notes.gaps(body), ["VoiceOver."])
        self.assertIsNone(notes.classify("Record the first live call with Element", []))

    def test_only_prs_merged_after_the_previous_release(self):
        since = datetime(2026, 11, 10, tzinfo=timezone.utc)
        prs = [pr(1, "Old", merged="2026-11-01T00:00:00Z"), pr(3, "New one", merged="2026-11-12T00:00:00Z"),
               pr(2, "Newer", merged="2026-11-11T00:00:00Z"), {"number": 4, "title": "Unmerged", "mergedAt": None}]
        self.assertEqual([p["number"] for p in notes.merged_since(prs, since)], [2, 3])

    def test_render_groups_and_lists_known_gaps_once(self):
        prs = [pr(10, "Add a formatting toolbar (#10)", body="## Not verified\n- VoiceOver."),
               pr(11, "Stop the room list freezing", body="## Not verified\n- VoiceOver."),
               pr(12, "Call view: full screen"), pr(13, "Docs only", labels=["documentation"])]
        text = notes.render("0.1.0-pre.1", prs, date(2026, 11, 15))
        self.assertIn("## Twayblade 0.1.0-pre.1", text)
        self.assertIn("### New\n- Add a formatting toolbar (#10)", text)
        self.assertIn("### Improved\n- Call view: full screen (#12)", text)
        self.assertIn("### Fixed\n- Stop the room list freezing (#11)", text)
        self.assertNotIn("Docs only", text)
        self.assertEqual(text.count("- VoiceOver."), 1)
        self.assertIn("### Known gaps", text)

    def test_empty_release_says_so(self):
        self.assertIn("No user-facing changes", notes.render("1", [], date(2026, 11, 15)))


if __name__ == "__main__":
    unittest.main()
