# Draws an original top-down GMMK 3 HE 75% illustration from the app's KEYBED layout.
import colorsys, re, sys, html
src = open(sys.argv[1]).read()
block = src[src.index("const KEYBED"):src.index("];", src.index("const KEYBED"))]
keys = [(n, l.encode().decode('unicode_escape') if '\\\\' in l else l, float(x), float(y), float(u))
        for n, l, x, y, u in re.findall(r'\("(\w+)", "((?:[^"\\]|\\.)*)", ([\d.]+), ([\d.]+), ([\d.]+)\)', block)]
keys = [(n, l.replace('\\\\', '\\'), x, y, u) for n, l, x, y, u in keys]
W, H, s = 600, 400, 0.483
ox, oy = 300 - 466 * s, 200 - 193 * s
def glow(x):  # rainbow left to right, red to pink
    r, g, b = colorsys.hls_to_rgb((x / 932) * 0.85, 0.6, 1)
    return f"#{int(r*255):02x}{int(g*255):02x}{int(b*255):02x}"
def R(x, y, w, h): return ox + x * s, oy + y * s, w * s, h * s
out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
       '<defs><filter id="g" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="3.2"/></filter>',
       '<linearGradient id="case" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#262626"/><stop offset="0.06" stop-color="#141414"/><stop offset="1" stop-color="#0c0c0c"/></linearGradient>',
       '<linearGradient id="cap" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#232323"/><stop offset="1" stop-color="#151515"/></linearGradient></defs>']
cx, cy, cw, ch = R(0, 0, 932, 386)
out.append(f'<rect x="{cx-4:.1f}" y="{cy+6:.1f}" width="{cw+8:.1f}" height="{ch:.1f}" rx="12" fill="#000" opacity="0.35" filter="url(#g)"/>')
out.append(f'<rect x="{cx:.1f}" y="{cy:.1f}" width="{cw:.1f}" height="{ch:.1f}" rx="10" fill="url(#case)" stroke="#333" stroke-width="1"/>')
glows, caps = [], []
for n, l, x, y, u in keys:
    kx, ky, kw, kh = R(x, y, u * 56 - 4, 52)
    c = glow(x + u * 28)
    glows.append(f'<rect x="{kx:.1f}" y="{ky:.1f}" width="{kw:.1f}" height="{kh:.1f}" rx="3" fill="{c}"/>')
    caps.append(f'<rect x="{kx+1.2:.1f}" y="{ky+1:.1f}" width="{kw-2.4:.1f}" height="{kh-2.6:.1f}" rx="2.5" fill="url(#cap)" stroke="{c}" stroke-opacity="0.85" stroke-width="0.9"/>')
    if l:
        caps.append(f'<text x="{kx+kw/2:.1f}" y="{ky+kh/2+2.3:.1f}" font-family="Work Sans, sans-serif" font-weight="600" font-size="{6.5 if len(l) < 3 else 5.2}" fill="{c}" text-anchor="middle">{html.escape(l)}</text>')
out.append('<g filter="url(#g)" opacity="0.9">' + ''.join(glows) + '</g>')
out += caps
bx, by, bw, bh = R(790, 20, 20, 52)
out.append(f'<rect x="{bx:.1f}" y="{by:.1f}" width="{bw:.1f}" height="{bh:.1f}" rx="2" fill="#f2a23a"/>')
out.append(f'<path d="M{bx+2.5:.1f} {by+bh*0.62:.1f} L{bx+bw-2.5:.1f} {by+bh*0.38:.1f}" stroke="#1a1a1a" stroke-width="1.6" stroke-linecap="round"/>')
kx, ky, kw, kh = R(860, 20, 52, 52)
out.append(f'<circle cx="{kx+kw/2:.1f}" cy="{ky+kh/2:.1f}" r="{kw/2:.1f}" fill="#1b1b1b" stroke="#3a3a3a" stroke-width="1.2"/>')
out.append(f'<circle cx="{kx+kw/2:.1f}" cy="{ky+kh/2:.1f}" r="{kw/2-3:.1f}" fill="none" stroke="#2a2a2a" stroke-width="0.8"/>')
out.append('</svg>')
print('\n'.join(out))
