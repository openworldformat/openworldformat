# openworldformat.org

A [Zola](https://www.getzola.org/) 0.22 site with no theme. The spec
pages are **rendered in** from `../spec` at build time — the markdown has
one source of truth at the repository root, and the site never carries a
spec copy that can drift (a lesson the format's lineage paid for once
with a 185-lines-behind viewer copy).

## Run

```bash
npm install && npx playwright install chromium   # once
npm run check         # build + render every conformance world headless
./scripts/build.sh    # build only
zola serve            # http://127.0.0.1:1111 (after a build)
```

## Deploy

```bash
./scripts/deploy.sh   # build + wrangler deploy (needs `npx wrangler login` once)
```

Cloudflare static-assets Worker (`wrangler.toml`); no Worker script.

## Layout

| Path | What it is |
|---|---|
| `config.toml` | site config (named for Zola 0.22.0, which CI installs) |
| `templates/` | hand-written base/index/section/page templates |
| `static/style.css` | one stylesheet, light/dark via `light-dark()` |
| `content/_index.md` | the front page |
| `content/spec/` | **generated** by `scripts/render_spec.py` — never edit; edit `../spec` |
| `static/world.html` | the live demo: any manifest, `world.html?src=…` |
| `static/conformance/`, `static/viewer/` | **generated** by `scripts/assemble.mjs` — edit `../conformance`, `../examples`, `../viewer/src/render.js` |
| `static/vendor/three/` | three.js (MIT), committed and self-hosted so the site makes no third-party requests |
| `scripts/` | `render_spec.py`, `assemble.mjs`, `check.mjs` (the headless render check), `build.sh`, `deploy.sh` |
