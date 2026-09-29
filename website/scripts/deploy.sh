#!/bin/sh
# Build and deploy to Cloudflare (needs `npx wrangler login` once).
set -e
cd "$(dirname "$0")/.."
python3 scripts/render_spec.py
zola build
npx wrangler deploy
