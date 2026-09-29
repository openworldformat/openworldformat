#!/usr/bin/env python3
"""Copy ../spec/*.md into content/spec/ as Zola pages.

One source of truth: the spec markdown lives at the repository root; the
website renders copies. For each page this script

- prepends front matter (title from the first `# ` heading, which it
  strips from the body; description from the first paragraph),
- sets an explicit reading-order weight,
- rewrites relative links to the repository on GitHub, so no page-
  relative ambiguity exists on the site (the section listing navigates
  between spec pages).

Run from website/ via scripts/build.sh.
"""

import re
import shutil
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
SPEC = REPO / "spec"
DEST = REPO / "website" / "content" / "spec"

# Reading order; README becomes the section index.
ORDER = ["README.md", "world.md", "package.md", "session.md", "state.md", "profiles.md", "versioning.md"]
REPO_URL = "https://github.com/openworldformat/openworldformat"


def github_url(target: str) -> str:
    target = target.rstrip("/")
    kind = "tree" if "." not in target.rsplit("/", 1)[-1] else "blob"
    return f"{REPO_URL}/{kind}/main/{target}"


def rewrite_links(body: str) -> str:
    return re.sub(r"\]\(([^)]+)\)", lambda m: repl_link(m, github_url), body)


def repl_link(m, github_url):
    url = m.group(1)
    if url.startswith("../"):
        return "](" + github_url(url[3:]) + ")"
    if url.startswith(("http://", "https://", "#", "mailto:")):
        return m.group(0)
    if url.endswith(".md"):
        return "](" + github_url("spec/" + url) + ")"
    return m.group(0)


def front_matter(title: str, description: str, weight: int | None) -> str:
    desc = description.replace('"', "'")
    lines = ["+++", f'title = "{title}"', f'description = "{desc}"']
    if weight is not None:
        lines.append(f"weight = {weight}")
    lines.append("+++")
    return "\n".join(lines)


def first_paragraph(body: str) -> str:
    for block in body.split("\n\n"):
        text = " ".join(block.split())
        if text and not text.startswith(("#", "```", "|", ">", "1.", "2.", "3.", "4.", "5.")):
            return text[:180].rstrip()
    return ""


def main() -> None:
    if DEST.exists():
        shutil.rmtree(DEST)
    DEST.mkdir(parents=True)

    section_body = None
    for weight, name in enumerate(ORDER):
        src = SPEC / name
        if not src.exists():
            sys.exit(f"missing spec page: {src}")
        text = src.read_text()
        match = re.match(r"^# (.+?)\n+", text)
        if not match:
            sys.exit(f"{src} has no `# ` title")
        title = match.group(1)
        body = text[match.end():].lstrip("\n")
        body = rewrite_links(body)

        if name == "README.md":
            section_body = body
            continue

        page = f"{front_matter(title, first_paragraph(body), weight)}\n\n{body}\n"
        (DEST / name).write_text(page)

    index = "+++\ntitle = \"Specification\"\nsort_by = \"weight\"\n+++\n\n"
    index += section_body or ""
    index = index.replace(
        "1. [The world document](world.md)",
        "1. [The world document](world/)",
    ).replace("2. [The package](package.md)", "2. [The package](package/)").replace(
        "3. [The session log](session.md)", "3. [The session log](session/)"
    ).replace(
        "4. [Profiles and extensions](profiles.md)", "4. [Profiles and extensions](profiles/)"
    ).replace("5. [Versioning policy](versioning.md)", "5. [Versioning policy](versioning/)")
    (DEST / "_index.md").write_text(index)
    print(f"rendered {len(ORDER) - 1} spec pages into {DEST}")


if __name__ == "__main__":
    main()
