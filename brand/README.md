# The Open World Format mark

A sheet folded twice, lit from the right, inside a world. The crease is a
W. The format's one invariant is the fold — state at any tip is a fold of
the log over the base — and the mark draws it.

| File | What it is | Where it goes |
|---|---|---|
| `mark.svg` | the mark in colour, a 64-unit square | the site's favicon and header, READMEs, anywhere on light or dark |
| `mark-mono.svg` | one colour (`currentColor`): the fold cut out of the world | print, a single ink, a text-coloured icon |
| `tile.svg` | the mark on a rounded square | app icons, the `.world` file icon, store listings |
| `lockup.svg`, `lockup-white.svg`, `lockup-mono.svg` | mark and wordmark side by side, for light grounds, dark grounds, one colour | READMEs, slides, documents |
| `lockup-stacked.svg` | mark over wordmark, centred | square-ish spaces |
| `mark-512.png` | the mark at 512 px, transparent | the GitHub avatar (its circle crop loses nothing) |
| `tile-512.png` | the tile at 512 px | app-store and launcher icons |
| `apple-touch-icon.png` | the fold on a full-bleed square, 180 px | iOS home screens (iOS rounds it) |
| `favicon-32.png`, `favicon-16.png` | PNG fallbacks for the favicon | browsers without SVG favicons |
| `social-preview.svg`, `social-preview.png` | 1280 × 640: the mark, the name, the tagline, the domain on the dark ground | GitHub's social preview, Open Graph and Twitter cards |

The site copies what it needs from here at build time
(`website/scripts/assemble.mjs`, into the gitignored `website/static/brand/`).

## Drawing

On a 64-unit square: a disc of radius 30 centred at (32, 32); a band 16
deep folded into four faces 15 wide, its peaks at y = 16, its valleys at
y = 32, running edge to edge and clipped by the disc. The faces alternate
the shaded blue and white, so the sheet reads as lit from the right. The
one-colour version keeps the disc and cuts the whole band out of it.

Colours: world blue `#0B62C4`, the shaded face `#9CC9FF`, the lit face
`#FFFFFF`; ink `#1F2430`; the dark ground `#171A21`, with `#5AA9FF` for
blue text on it. These are the website's own tokens.

Clear space: one face width (15/64 of the mark) on every side. Smallest
size: 16 px. Do not outline the disc, put it inside another circle,
rotate it, recolour the faces, or set the wordmark in another face.

## Wordmark

"Open World Format" in [Manrope](https://github.com/sharanda/manrope)
(SIL Open Font License) at weight 700, tracked 1.5 % tighter; the
package name `openworldformat` at weight 600. The lockup files carry the
letters as outlines, so they need no font installed.

## Regenerating

`mark.svg`, `mark-mono.svg` and `tile.svg` are the sources, written by
hand. Everything else is built from them and the typeface by `build.py`:

```bash
# Manrope's variable font, from https://github.com/google/fonts/tree/main/ofl/manrope
uvx --with fonttools --with uharfbuzz python3 brand/build.py "Manrope[wght].ttf"
```

which rewrites the lockups and the social preview, and rasterises the
PNGs with `rsvg-convert` (librsvg).
