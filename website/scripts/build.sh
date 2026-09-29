#!/bin/sh
# Build openworldformat.org: render the spec pages in, then zola build.
set -e
cd "$(dirname "$0")/.."
python3 scripts/render_spec.py
zola build
