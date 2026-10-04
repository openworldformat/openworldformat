"""Dataset tooling: fold a corpus of recordings, compute one number.

The notebook pitch, as a module: a folder of `.world` packages is a
dataset — each ``ops.jsonl`` a recording, each fold a row.
:func:`summarize` computes the row; ``python -m openworldformat.dataset``
prints the table (walk a directory tree for recordings, or pass
explicit ones). Scale is the only difference from the demo: the fold
is linear in log length, the format is JSON all the way down, and
there is no engine to boot.

The branches column counts tips — recordings whose authors forked the
history. Fold each tip with :func:`openworldformat.fold_path` and the
counterfactuals come with the corpus.
"""

from __future__ import annotations

import sys
from pathlib import Path

from . import build_history, fold_log, parse_log_line, read_package
from .eval import entry_metrics

__all__ = ["summarize", "main"]

#: The columns the report prints, in order.
COLUMNS = ("world", "entities", "edits", "ops", "entries", "authors", "tips", "span_s")


def _entries_of(world: Path) -> list:
    log = world / "ops.jsonl"
    if not log.exists():
        return []
    return [parse_log_line(l) for l in log.read_text().splitlines() if l.strip()]


def summarize(world_dir) -> dict:
    """One recording's row: fold it, measure the trajectory.

    :param world_dir: a `.world` package directory (``manifest.json``
        plus ``ops.jsonl`` when the world was recorded)
    :return: ``{world, entities, edits, ops, entries, authors, tips,
        span_s, revision, span_ms}``
    """
    world = Path(world_dir)
    manifest, entries, _head, _package = read_package(world)
    state = fold_log(manifest, entries)
    row = entry_metrics(entries, state["applied_edits"])
    row["span_s"] = round(row["span_ms"] / 1000, 3)
    row["world"] = world.name
    row["entities"] = len(state["entities"])
    row["tips"] = len(build_history(entries)["tips"])
    return row


def main(argv: list | None = None) -> int:
    """The CLI: report on a corpus. Arguments are roots to walk for
    ``ops.jsonl`` (default: the current directory)."""
    argv = sys.argv[1:] if argv is None else argv
    roots = [Path(a) for a in argv] or [Path(".")]
    dirs = sorted({log.parent for root in roots for log in root.rglob("ops.jsonl")})
    if not dirs:
        print("no recordings found (no ops.jsonl under the given roots)", file=sys.stderr)
        return 1
    rows = [summarize(d) for d in dirs]

    def cell(row, column):
        v = row[column]
        return ", ".join(v) if isinstance(v, list) else str(v)

    widths = {c: max(len(c), *(len(cell(r, c)) for r in rows)) for c in COLUMNS}
    print("  ".join(c.ljust(widths[c]) for c in COLUMNS))
    for r in rows:
        print("  ".join(cell(r, c).ljust(widths[c]) for c in COLUMNS))
    edits = sum(r["edits"] for r in rows)
    tips = sum(r["tips"] for r in rows)
    print(f"\n{len(rows)} recordings · {edits} edits · {tips} tips "
          f"(branches included in the count)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
