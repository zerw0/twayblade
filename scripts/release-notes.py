#!/usr/bin/env python3
"""Draft release notes from the pull requests merged since the last release.

    scripts/release-notes.py --version 0.1.0-pre.1 --since v0.0.9
    scripts/release-notes.py --version 0.1.0-pre.1 --since 2026-11-01
    scripts/release-notes.py --version 0.1.0-pre.1 --since 2026-11-01 --input prs.json

Groups merged PRs as New, Improved and Fixed, and lists the "Not verified" /
"Limits" gaps their descriptions state, so a release never reads as more tested
than it is. The output is a draft for a person to review and reword for users;
it does not publish anything and reads nothing but merged-PR metadata.

`--since` is the previous tag or a date (YYYY-MM-DD). With a tag, a PR is in
the release when its merge commit is reachable from `--until` (default HEAD,
the new tag in CI) but not from the previous tag, so work merged into develop
after the release branch was cut stays out; release/* and sync/* pull requests
(develop to main and back) are left out too. With a date, PRs merged after it
are listed. The PR list
comes from the GitHub CLI (`gh`, or `nix shell nixpkgs#gh -c gh`), or from a JSON
file in the same shape with `--input`.
"""
import argparse
import json
import re
import shlex
import subprocess
import sys
from datetime import date, datetime, timezone

REPO = "zerw0/twayblade"
FIELDS = "number,title,mergedAt,labels,body,url,mergeCommit,headRefName"
# Branches that only move work between develop and main.
FLOW_BRANCHES = ("release/", "sync/")

NEW = ("add", "introduce", "support", "invite", "allow", "enable", "let")
FIXED = ("fix", "stop", "restart", "repair", "prevent", "correct", "resolve")
SKIP_LABELS = {"documentation"}
SKIP_PREFIXES = ("docs", "keep per-change progress", "move ", "record ", "decide ", "log ")
GAP_HEADINGS = ("not verified", "limits", "not included", "not fixed")


def clean_title(title: str) -> str:
    """Drop a trailing squash-merge "(#123)"."""
    return re.sub(r"\s*\(#\d+\)\s*$", "", title).strip()


def classify(title: str, labels: list[str]) -> str | None:
    """New, Improved, Fixed, or None for changes users never see."""
    lowered = clean_title(title).lower()
    names = {label.lower() for label in labels}
    if names & SKIP_LABELS or lowered.startswith(SKIP_PREFIXES):
        return None
    if "bug" in names or lowered.startswith(FIXED):
        return "Fixed"
    if lowered.startswith(NEW):
        return "New"
    return "Improved"


def gaps(body: str) -> list[str]:
    """Bullets under the "Not verified" style headings of a PR description."""
    found: list[str] = []
    active = False
    for line in (body or "").splitlines():
        heading = re.match(r"^#{1,6}\s*(.+?)\s*$", line)
        if heading:
            active = heading.group(1).lower().startswith(GAP_HEADINGS)
            continue
        text = line.strip()
        # Skip blanks, the "Closes #N" links and the generated-by footer.
        if not active or not text or text.startswith(("Closes", "Refs", "🤖")):
            continue
        text = re.sub(r"^[-*]\s+", "", text)
        # Keep the first sentence: release notes list gaps, they don't argue them.
        first = re.split(r"(?<=[.!?])\s", text, maxsplit=1)[0]
        if first and first not in found:
            found.append(first)
    return found


def merged_since(prs: list[dict], since: datetime) -> list[dict]:
    result = []
    for pr in prs:
        merged = pr.get("mergedAt")
        if not merged:
            continue
        stamp = datetime.fromisoformat(merged.replace("Z", "+00:00"))
        if stamp > since:
            result.append(pr)
    return sorted(result, key=lambda pr: pr["number"])


def in_release(prs: list[dict], commits: set[str]) -> list[dict]:
    """PRs whose merge commit is one of `commits`, without branch-flow PRs."""
    result = []
    for pr in prs:
        oid = (pr.get("mergeCommit") or {}).get("oid")
        if oid not in commits or pr.get("headRefName", "").startswith(FLOW_BRANCHES):
            continue
        result.append(pr)
    return sorted(result, key=lambda pr: pr["number"])


def commits_between(since: str, until: str) -> set[str]:
    out = subprocess.run(
        ["git", "rev-list", f"{since}..{until}"],
        capture_output=True, text=True, check=True,
    ).stdout
    return set(out.split())


def render(version: str, prs: list[dict], today: date | None = None) -> str:
    today = today or date.today()
    sections: dict[str, list[str]] = {"New": [], "Improved": [], "Fixed": []}
    known: list[str] = []
    for pr in prs:
        labels = [label["name"] for label in pr.get("labels", [])]
        kind = classify(pr["title"], labels)
        if kind is None:
            continue
        sections[kind].append(f"- {clean_title(pr['title'])} (#{pr['number']})")
        for gap in gaps(pr.get("body", "")):
            if gap not in known:
                known.append(gap)
    lines = [f"## Twayblade {version}", "", f"Draft, {today.isoformat()}. Review and reword for users before publishing.", ""]
    for name, items in sections.items():
        if items:
            lines += [f"### {name}", *items, ""]
    if not any(sections.values()):
        lines += ["No user-facing changes since the last release.", ""]
    if known:
        lines += ["### Known gaps", *(f"- {gap}" for gap in known), ""]
    return "\n".join(lines).rstrip("\n") + "\n"


def since_datetime(value: str) -> datetime:
    if re.fullmatch(r"\d{4}-\d{2}-\d{2}", value):
        return datetime.fromisoformat(value).replace(tzinfo=timezone.utc)
    out = subprocess.run(
        ["git", "log", "-1", "--format=%cI", value],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    if not out:
        raise SystemExit(f"release-notes: {value!r} is neither a date nor a known tag")
    return datetime.fromisoformat(out)


def fetch(repo: str, gh: str) -> list[dict]:
    command = [*shlex.split(gh), "pr", "list", "--repo", repo, "--state", "merged",
               "--limit", "300", "--json", FIELDS]
    return json.loads(subprocess.run(command, capture_output=True, text=True, check=True).stdout)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--version", required=True)
    parser.add_argument("--since", required=True, help="previous tag or YYYY-MM-DD")
    parser.add_argument("--until", default="HEAD", help="the release commit (with a tag --since)")
    parser.add_argument("--repo", default=REPO)
    parser.add_argument("--gh", default="nix shell nixpkgs#gh -c gh")
    parser.add_argument("--input", help="read PRs from this JSON file instead of GitHub")
    args = parser.parse_args()
    prs = json.load(open(args.input)) if args.input else fetch(args.repo, args.gh)
    if re.fullmatch(r"\d{4}-\d{2}-\d{2}", args.since):
        selected = merged_since(prs, since_datetime(args.since))
    else:
        selected = in_release(prs, commits_between(args.since, args.until))
    sys.stdout.write(render(args.version, selected))


if __name__ == "__main__":
    main()
