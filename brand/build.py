#!/usr/bin/env python3
"""Build the derived brand files from the three hand-written marks and
the Manrope typeface: the lockups (wordmark as outlines), the social
preview, and the PNGs. See README.md in this folder.

    uvx --with fonttools --with uharfbuzz python3 brand/build.py "Manrope[wght].ttf"

Needs `rsvg-convert` (librsvg) on the path for the PNGs.
"""

import io
import subprocess
import sys
from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
import uharfbuzz as hb

HERE = Path(__file__).resolve().parent
BLUE, LIGHT, WHITE, INK = "#0B62C4", "#9CC9FF", "#FFFFFF", "#1F2430"
DARK, MUTED_DARK, ACCENT_DARK = "#171A21", "#9AA3B2", "#5AA9FF"

# The same numbers as mark.svg: four faces of a band 16 deep, peaks at 16,
# valleys at 32, and the band as one polygon for the one-colour cut-out.
FACETS = ["M2 16 17 32 17 48 2 32Z", "M17 32 32 16 32 32 17 48Z", "M32 16 47 32 47 48 32 32Z", "M47 32 62 16 62 32 47 48Z"]
BAND = "M2 16 17 32 32 16 47 32 62 16V32L47 48 32 32 17 48 2 32Z"


def load_fonts(path: Path):
    fonts = {}
    for weight in (700, 600):
        f = instancer.instantiateVariableFont(TTFont(path), {"wght": weight})
        buf = io.BytesIO()
        f.save(buf)
        font = hb.Font(hb.Face(buf.getvalue()))
        fonts[weight] = (f, font, f.getGlyphSet(), f["head"].unitsPerEm, f["OS/2"].sCapHeight)
    return fonts


def outline(fonts, text, weight, tracking_em=0.0):
    """The text shaped by HarfBuzz and drawn as one SVG path, in font units, y down."""
    f, font, glyphs, upem, cap = fonts[weight]
    b = hb.Buffer()
    b.add_str(text)
    b.guess_segment_properties()
    hb.shape(font, b, {"kern": True, "liga": True})
    x, d = 0, []
    for info, pos in zip(b.glyph_infos, b.glyph_positions):
        pen = SVGPathPen(glyphs, ntos=lambda v: f"{v:.1f}".rstrip("0").rstrip("."))
        glyphs[f.getGlyphName(info.codepoint)].draw(TransformPen(pen, (1, 0, 0, -1, x + pos.x_offset, pos.y_offset)))
        if cmds := pen.getCommands():
            d.append(cmds)
        x += pos.x_advance + tracking_em * upem
    return " ".join(d), x, upem, cap


def text_path(fonts, text, weight, size, x, y_center, fill, tracking_em=0.0, anchor="start"):
    """Outlined text `size` px tall (em), its cap height centred on y_center."""
    d, adv, upem, cap = outline(fonts, text, weight, tracking_em)
    s = size / upem
    width = adv * s
    x0 = x - width / 2 if anchor == "middle" else x
    return f'<path transform="translate({x0:.2f} {y_center + cap * s / 2:.2f}) scale({s:.5f})" fill="{fill}" d="{d}"/>', width


def mark(x, y, size, uid, mono=None):
    s = size / 64
    if mono:
        return (f'<g transform="translate({x} {y}) scale({s:.5f})"><mask id="m{uid}"><circle cx="32" cy="32" r="30" fill="#fff"/>'
                f'<path d="{BAND}" fill="#000"/></mask><circle cx="32" cy="32" r="30" fill="{mono}" mask="url(#m{uid})"/></g>')
    faces = "".join(f'<path d="{d}" fill="{LIGHT if i % 2 == 0 else WHITE}"/>' for i, d in enumerate(FACETS))
    return (f'<g transform="translate({x} {y}) scale({s:.5f})"><clipPath id="c{uid}"><circle cx="32" cy="32" r="30"/></clipPath>'
            f'<circle cx="32" cy="32" r="30" fill="{BLUE}"/><g clip-path="url(#c{uid})">{faces}</g></g>')


def svg(w, h, body, extra=""):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:g} {h:g}" width="{w:g}" height="{h:g}"{extra}>\n{body}\n</svg>\n'


def main(font_path: str) -> None:
    fonts = load_fonts(Path(font_path))
    label = ' role="img" aria-label="Open World Format"'

    def lockup(ink, mono=None, uid="lockup"):
        p, width = text_path(fonts, "Open World Format", 700, 42, 82, 32, ink, tracking_em=-0.015)
        return svg(round(82 + width + 0.5), 64, mark(0, 0, 64, uid, mono) + "\n" + p, label)

    (HERE / "lockup.svg").write_text(lockup(INK))
    (HERE / "lockup-white.svg").write_text(lockup(WHITE, uid="lockupw"))
    (HERE / "lockup-mono.svg").write_text(lockup("currentColor", mono="currentColor", uid="lockupm"))

    d, adv, upem, cap = outline(fonts, "Open World Format", 700, -0.015)
    s = 36 / upem
    width = max(adv * s, 96) + 16
    stacked = mark((width - 96) / 2, 0, 96, "stack") + f'\n<path transform="translate({(width - adv * s) / 2:.2f} {96 + 20 + cap * s:.2f}) scale({s:.5f})" fill="{INK}" d="{d}"/>'
    (HERE / "lockup-stacked.svg").write_text(svg(round(width + 0.5), 96 + 20 + 36, stacked, label))

    body = [f'<rect width="1280" height="640" fill="{DARK}"/>', mark(560, 118, 160, "social"),
            text_path(fonts, "Open World Format", 700, 64, 640, 372, WHITE, tracking_em=-0.015, anchor="middle")[0],
            text_path(fonts, "An open format for .world packages", 600, 28, 640, 436, MUTED_DARK, anchor="middle")[0],
            text_path(fonts, "openworldformat.org", 600, 22, 640, 560, ACCENT_DARK, anchor="middle")[0]]
    (HERE / "social-preview.svg").write_text(svg(1280, 640, "\n".join(body)))

    faces = "".join(f'<path d="{d}" fill="{LIGHT if i % 2 == 0 else WHITE}"/>' for i, d in enumerate(FACETS))
    touch = HERE / "touch-icon.svg"
    touch.write_text(svg(64, 64, f'<rect width="64" height="64" fill="{BLUE}"/>{faces}'))

    def png(src, out, w, h=None):
        subprocess.run(["rsvg-convert", "-w", str(w), *(["-h", str(h)] if h else []), str(src), "-o", str(out)], check=True)

    png(HERE / "social-preview.svg", HERE / "social-preview.png", 1280)
    png(HERE / "mark.svg", HERE / "mark-512.png", 512, 512)
    png(HERE / "tile.svg", HERE / "tile-512.png", 512, 512)
    png(touch, HERE / "apple-touch-icon.png", 180, 180)
    png(HERE / "mark.svg", HERE / "favicon-32.png", 32, 32)
    png(HERE / "mark.svg", HERE / "favicon-16.png", 16, 16)
    touch.unlink()
    print("built the lockups, the social preview and the PNGs in", HERE)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
