#!/usr/bin/env python3
"""Resolve the catalog's inline documentation links against this checkout."""

from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[2]
REPO = "https://github.com/sformisano/type-history/blob/main/"


def anchors(text):
    result = set()
    counts = {}
    in_fence = False
    for line in text.splitlines():
        if line.startswith("```"):
            in_fence = not in_fence
        if in_fence or not re.match(r"^#{1,6} ", line):
            continue
        title = re.sub(r"^#+ | +#+$", "", line).lower()
        slug = re.sub(r"[^\w\- ]", "", title).replace(" ", "-")
        count = counts.get(slug, 0)
        counts[slug] = count + 1
        result.add(f"{slug}-{count}" if count else slug)
    return result


def check(path):
    text = path.read_text()
    errors = []
    if re.search(r"^\s*\[[^\]]+\]:|\]\[", text, re.MULTILINE):
        errors.append("reference-style links are not supported by this check")
    if len(re.findall(r"^```.*$", text, re.MULTILINE)) % 2:
        errors.append("unclosed code fence")
    for link in re.findall(r"\[[^\]\n]+\]\(([^)\s]+)\)", text):
        parsed = urlsplit(link)
        if parsed.scheme:
            if not link.startswith(REPO):
                errors.append(f"unsupported external link: {link}")
                continue
            target = ROOT / unquote(urlsplit(link[len(REPO):]).path)
        else:
            target = path.parent / unquote(parsed.path) if parsed.path else path
        target = target.resolve()
        if not target.is_relative_to(ROOT) or not target.exists():
            errors.append(f"missing or outside-repository target: {link}")
        elif parsed.fragment and (
            not target.is_file()
            or unquote(parsed.fragment) not in anchors(target.read_text())
        ):
            errors.append(f"missing heading: {link}")
    return errors


def main():
    skills = sorted((ROOT / ".skillcatalog/catalog/skills").glob("*/SKILL.md"))
    if not skills:
        sys.exit("No skills found")
    failures = [(path, error) for path in skills for error in check(path)]
    for path, error in failures:
        print(f"{path.relative_to(ROOT)}: {error}", file=sys.stderr)
    if failures:
        return 1
    print(f"Checked code fences and repository links in {len(skills)} skills")
    return 0


if __name__ == "__main__":
    sys.exit(main())
