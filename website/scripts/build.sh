#!/bin/sh
# Build openworldformat.org: render the spec pages in, assemble the live
# demo's inputs, then zola build. (npm run build does the same.)
set -e
cd "$(dirname "$0")/.."
python3 scripts/render_spec.py
node scripts/assemble.mjs
zola build
