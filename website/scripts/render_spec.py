#!/usr/bin/env python3
"""Copy ../spec/*.md into content/spec/ as Zola pages.

One source of truth: the spec markdown lives at the repository root; the
website renders copies. For each page this script

- prepends front matter (title from the first `# ` heading, which it
  strips from the body; description from the first paragraph),
- sets an explicit reading-order weight,
- rewrites links: one spec page to another becomes a Zola internal link
  (`@/spec/<page>.md`, checked at build time, anchors included), and
  everything else relative — the schema, the conformance worlds, the
  extensions, the RFCs — points at the repository on GitHub.

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
ORDER = [
    "README.md",
    "world.md",
    "package.md",
    "session.md",
    "state.md",
    "profiles.md",
    "versioning.md",
    "security.md",
]
# The pages the site renders: a link to one stays on the site.
SITE_PAGES = set(ORDER)
REPO_URL = "https://github.com/openworldformat/openworldformat"


def github_url(target: str) -> str:
    target = target.rstrip("/")
    kind = "tree" if "." not in target.rsplit("/", 1)[-1] else "blob"
    return f"{REPO_URL}/{kind}/main/{target}"


def rewrite_links(body: str) -> str:
    return re.sub(r"\]\(([^)]+)\)", lambda m: repl_link(m, github_url), body)


def repl_link(m, github_url):
    url = m.group(1)
    if url.startswith(("http://", "https://", "#", "mailto:")):
        return m.group(0)
    if url.startswith("../"):
        return "](" + github_url(url[3:]) + ")"
    path, _, anchor = url.partition("#")
    if path in SITE_PAGES:
        target = "_index.md" if path == "README.md" else path
        return "](@/spec/" + target + ("#" + anchor if anchor else "") + ")"
    # Anything else relative — a page the site does not render, a schema
    # or registry file, a folder — lives in the repository.
    return "](" + github_url("spec/" + url) + ")"


def front_matter(title: str, description: str, weight: int | None, source: str) -> str:
    desc = description.replace('"', "'")
    lines = ["+++", f'title = "{title}"', f'description = "{desc}"']
    if weight is not None:
        lines.append(f"weight = {weight}")
    # The file this page is rendered from; the page links to it.
    lines += ["[extra]", f'source = "{source}"', "+++"]
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

        page = f"{front_matter(title, first_paragraph(body), weight, 'spec/' + name)}\n\n{body}\n"
        (DEST / name).write_text(page)

    description = first_paragraph(section_body or "").replace('"', "'")
    index = f'+++\ntitle = "Specification"\ndescription = "{description}"\nsort_by = "weight"\n+++\n\n'
    index += section_body or ""
    (DEST / "_index.md").write_text(index)
    print(f"rendered {len(ORDER) - 1} spec pages into {DEST}")


if __name__ == "__main__":
    main()
