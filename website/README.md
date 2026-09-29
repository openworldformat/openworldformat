# openworldformat.org

A [Zola](https://www.getzola.org/) 0.22 site with no theme. The spec
pages are **rendered in** from `../spec` at build time — the markdown has
one source of truth at the repository root, and the site never carries a
spec copy that can drift (a lesson the format's lineage paid for once
with a 185-lines-behind viewer copy).

## Run

```bash
./scripts/build.sh    # render spec pages in, then zola build
zola serve            # http://127.0.0.1:1111 (after a build, or run render_spec.py once)
```

## Deploy

```bash
./scripts/deploy.sh   # build + wrangler deploy (needs `npx wrangler login` once)
```

Cloudflare static-assets Worker (`wrangler.toml`); no Worker script.

## Layout

| Path | What it is |
|---|---|
| `zola.toml` | site config |
| `templates/` | hand-written base/index/section/page templates |
| `static/style.css` | one stylesheet, light/dark via `light-dark()` |
| `content/_index.md` | the front page |
| `content/spec/` | **generated** by `scripts/render_spec.py` — never edit; edit `../spec` |
| `scripts/` | `render_spec.py`, `build.sh`, `deploy.sh` |
