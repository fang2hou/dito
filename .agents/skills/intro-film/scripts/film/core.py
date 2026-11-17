import os, re, glob, math, bisect, functools
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

WORK = os.environ.get("FILM_WORK", "film-work")   # captures live in $FILM_WORK/real/{hud,intro,tools}_<lang>
REAL = f"{WORK}/real"
OUT_DIR = os.environ.get("FILM_OUT", "film-out")   # voice.json, voice_words.json and the renders
FONTS = os.environ.get("FILM_FONTS", "crates/dito-app/assets/fonts")
DW, DH, FPS = 1040, 720, 60
S = 2          # master pixels per design unit
CAP = 2        # window captures: pixels per design unit
HUDCAP = 8     # HUD captures: pixels per real HUD point (HUD_SCALE 4 times display scale 2)
W, H = DW * S, DH * S
BG = (0x44, 0x41, 0x39)
INK = (0xF7, 0xF5, 0xF0)
MUTED = (0xC2, 0xBD, 0xB2)
GOLD = (0xE0, 0xC4, 0x7D)
CARD = (0x4D, 0x49, 0x3F)
BORDER = (0x5A, 0x56, 0x4B)
LANG = os.environ.get("FILM_LANG", "ja")

def ease(x): x = min(1, max(0, x)); return x * x * (3 - 2 * x)
def eout(x): x = min(1, max(0, x)); return 1 - (1 - x) ** 3
def lerp(a, b, t): return a + (b - a) * t
def ramp(t, a, b): return min(1.0, max(0.0, (t - a) / (b - a))) if b > a else (1.0 if t >= a else 0.0)

@functools.lru_cache(maxsize=None)
def font(name, size): return ImageFont.truetype(f"{FONTS}/{name}", size)

@functools.lru_cache(maxsize=None)
def korean_font(size): return ImageFont.truetype("/System/Library/Fonts/AppleSDGothicNeo.ttc", size, index=2)

def text_font(kind, size):
    if LANG == "ko": return korean_font(size)
    if LANG == "zh" and kind != "latin": return font("IBMPlexSansSC-Medium.ttf", size)
    if LANG == "en" or kind == "latin": return font("Outfit-Medium.ttf", size)
    return font("MPLUS1p-Medium.ttf", size)

# ---------- sequences
class Seq:
    def __init__(self, pattern):
        fs = []
        for f in glob.glob(pattern):
            m = re.search(r"-(\d{6})", os.path.basename(f))
            if m: fs.append((int(m.group(1)), f))
        fs.sort(); self.ms = [a for a, _ in fs]; self.fs = [b for _, b in fs]
    def path(self, t):
        i = bisect.bisect_left(self.ms, t)
        if i == 0: return self.fs[0]
        if i >= len(self.ms): return self.fs[-1]
        return self.fs[i] if abs(self.ms[i] - t) < abs(self.ms[i - 1] - t) else self.fs[i - 1]
    def _pair(self, t):
        i = bisect.bisect_left(self.ms, t)
        if i == 0: return 0, 0, 0.0
        if i >= len(self.ms): return len(self.ms) - 1, len(self.ms) - 1, 0.0
        a, b = self.ms[i - 1], self.ms[i]
        return i - 1, i, (t - a) / (b - a) if b > a else 0.0
    def img(self, t):
        i, j, k = self._pair(t)
        if i == j or k < 0.04: return load(self.fs[i])
        if k > 0.96: return load(self.fs[j])
        return gated_blend(self.fs[i], self.fs[j], round(k * 6) / 6, False)
    def hud(self, t):
        i, j, k = self._pair(t)
        if i == j or k < 0.04: return hud_crop(self.fs[i])
        if k > 0.96: return hud_crop(self.fs[j])
        return gated_blend(self.fs[i], self.fs[j], round(k * 6) / 6, True)

@functools.lru_cache(maxsize=96)
def gated_blend(pa, pb, k, hud):
    """Frame interpolation for captures taken at about 20 fps: pixels that change a little (glows, fades, progress)
    are blended, pixels that change a lot (text appearing, state switches) keep the nearer frame, so no ghost text."""
    a = hud_crop(pa) if hud else load(pa); b = hud_crop(pb) if hud else load(pb)
    fa, fb = np.asarray(a, np.float32), np.asarray(b, np.float32)
    d = np.abs(fa - fb).max(axis=2); s = np.clip((d - 14) / 30, 0, 1); s = s * s * (3 - 2 * s)
    w = (k * (1 - s))[..., None]
    if hud:
        pa_, pb_ = fa[..., :3] * fa[..., 3:4] / 255, fb[..., :3] * fb[..., 3:4] / 255
        al = fa[..., 3:4] + (fb[..., 3:4] - fa[..., 3:4]) * w; pm = pa_ + (pb_ - pa_) * w
        rgb = np.where(al > 0.5, pm * 255 / np.maximum(al, 0.5), fa[..., :3])
        out = np.concatenate([rgb, al], -1)
    else:
        out = fa + (fb - fa) * w
        out[..., 3] = 255
    return Image.fromarray(np.clip(out + 0.5, 0, 255).astype(np.uint8), "RGBA")

@functools.lru_cache(maxsize=256)
def load(path): return Image.open(path).convert("RGBA")

def hud_seq(scene): return Seq(f"{REAL}/hud_{LANG}/{scene}-??????.png")
def ed_seq(scene): return Seq(f"{REAL}/hud_{LANG}/{scene}-??????-ed.png")
CROP = tuple(v * (HUDCAP // 4) for v in (488, 1360, 952, 1565))
@functools.lru_cache(maxsize=512)
def hud_crop(path): return Image.open(path).convert("RGBA").crop(CROP)

# ---------- drawing helpers
@functools.lru_cache(maxsize=256)
def rmask(w, h, r):
    s = 3
    m = Image.new("L", (w * s, h * s), 0)
    ImageDraw.Draw(m).rounded_rectangle((0, 0, w * s - 1, h * s - 1), r * s, fill=255)
    return m.resize((w, h), Image.LANCZOS)

def _premult(im):
    arr = np.asarray(im.convert("RGBA"), np.float32); al = arr[..., 3:4] / 255
    return Image.fromarray(np.concatenate([arr[..., :3] * al, arr[..., 3:4]], -1).astype(np.uint8), "RGBA")

def place(canvas, im, x, y, s=1.0, alpha=1.0, r=None):
    """Sub-pixel placement and scaling: exact float geometry, premultiplied bicubic, supersampled rounded mask."""
    if alpha <= 0: return
    w, h = im.size; sw, shh = w * s, h * s
    ox, oy = int(math.floor(x)) - 1, int(math.floor(y)) - 1
    tw, th = int(math.ceil(sw)) + 3, int(math.ceil(shh)) + 3
    cx0, cy0, cx1, cy1 = max(0, ox), max(0, oy), min(W, ox + tw), min(H, oy + th)
    if cx1 <= cx0 or cy1 <= cy0: return
    opaque = im.mode == "RGB"
    src = im if opaque else _premult(im)
    if s < 0.95:
        src = src.resize((max(2, int(round(w * s))), max(2, int(round(h * s)))), Image.LANCZOS)
    ax, ay = src.width / sw, src.height / shh
    tile = src.transform((tw, th), Image.AFFINE, (ax, 0, (ox - x) * ax, 0, ay, (oy - y) * ay), Image.BICUBIC if opaque else Image.BILINEAR)
    t = np.asarray(tile.crop((cx0 - ox, cy0 - oy, cx1 - ox, cy1 - oy)), np.float32)
    dst = np.asarray(canvas.crop((cx0, cy0, cx1, cy1)), np.float32)
    mk = None
    if r is not None:
        ms = 3; m = Image.new("L", (tw * ms, th * ms), 0)
        ImageDraw.Draw(m).rounded_rectangle(((x - ox) * ms, (y - oy) * ms, (x - ox + sw) * ms - 1, (y - oy + shh) * ms - 1), r * ms, fill=255)
        mk = np.asarray(m.reduce(ms).crop((cx0 - ox, cy0 - oy, cx1 - ox, cy1 - oy)), np.float32)[..., None] / 255
    if opaque:
        cov = (mk if mk is not None else np.ones(t.shape[:2] + (1,), np.float32)) * alpha
        out_rgb = t[..., :3] * cov + dst[..., :3] * (1 - cov); out_a = cov * 255 + dst[..., 3:4] * (1 - cov)
    else:
        a = np.clip(t[..., 3:4], 0, 255) / 255; rgb = np.minimum(np.clip(t[..., :3], 0, 255), a * 255)
        if mk is not None: a = a * mk; rgb = rgb * mk
        a = a * alpha; rgb = rgb * alpha
        out_rgb = rgb + dst[..., :3] * (1 - a); out_a = a * 255 + dst[..., 3:4] * (1 - a)
    out = np.concatenate([out_rgb, out_a], -1)
    canvas.paste(Image.fromarray(np.clip(out + 0.5, 0, 255).astype(np.uint8), "RGBA"), (cx0, cy0))

def _put_px(canvas, im, x, y, alpha=1.0):
    if alpha <= 0: return
    if abs(x - round(x)) > 0.02 or abs(y - round(y)) > 0.02:
        place(canvas, im, x, y, 1.0, alpha); return
    x, y = int(round(x)), int(round(y))
    w, h = im.size
    x0, y0, x1, y1 = max(0, x), max(0, y), min(W, x + w), min(H, y + h)
    if x1 <= x0 or y1 <= y0: return
    part = im.crop((x0 - x, y0 - y, x1 - x, y1 - y))
    if alpha < 1:
        a = part.getchannel("A").point(lambda v: int(v * alpha)); part.putalpha(a)
    canvas.alpha_composite(part, (x0, y0))

def put(canvas, im, x, y, alpha=1.0):
    """x and y in design units; the sprite is already in master pixels."""
    _put_px(canvas, im, x * S, y * S, alpha)

def shadow(canvas, w, h, x, y, r, blur=28, off=20, alpha=0.45):
    """All geometry in design units."""
    wp, hp, rp = max(2, int(round(w * S))), max(2, int(round(h * S))), max(2, int(round(r * S)))
    bl = blur * S; m = int(3 * bl)
    sh = Image.new("L", (wp + 2 * m, hp + 2 * m), 0)
    sh.paste(rmask(wp, hp, rp), (m, m))
    sh = sh.filter(ImageFilter.GaussianBlur(bl / 2)).point(lambda v: int(v * alpha))
    layer = Image.new("RGBA", sh.size, (0, 0, 0, 255)); layer.putalpha(sh)
    _put_px(canvas, layer, round(x * S) - m, round(y * S) - m + off * S)

def window(canvas, img, x, y, s, r=20, alpha=1.0, sh=True, resample=None):
    """x, y, s, r in design units; img is a window capture (CAP pixels per design unit)."""
    ps = s * S / CAP; w, h = img.width * ps, img.height * ps
    if sh and alpha > 0.02: shadow(canvas, w / S, h / S, x, y, r * s, alpha=0.45 * alpha)
    place(canvas, img if img.mode == "RGB" else img.convert("RGB"), x * S, y * S, ps, alpha, r=r * s * S)

# ---------- stage
def _stage_full():
    xs = np.arange(W)[None, :].astype(np.float32); ys = np.arange(H)[:, None].astype(np.float32)
    base = np.zeros((H, W, 3), np.float32); base[:] = BG
    d = np.sqrt(((xs - 0.5 * W) / (0.7 * W)) ** 2 + ((ys - 0.3 * H) / (0.8 * H)) ** 2)
    lift = 0.05 * np.clip(1 - d, 0, 1)[..., None]
    base = base * (1 - lift) + 255 * lift
    g = np.zeros((H, W), np.float32)
    for k in range(1, DW // 48 + 1):
        if 48 * k * S + S <= W: g[:, 48 * k * S:48 * k * S + S] = 0.06 if k % 4 == 0 else 0.03
    for k in range(1, DH // 48 + 1):
        if 48 * k * S + S <= H: g[48 * k * S:48 * k * S + S, :] = np.maximum(g[48 * k * S:48 * k * S + S, :], 0.06 if k % 4 == 0 else 0.03)
    base = base * (1 - g[..., None]) + 255 * g[..., None]
    r = np.sqrt(((xs - 0.5 * W) / (0.5 * W)) ** 2 + ((ys - 0.5 * H) / (0.5 * H)) ** 2) / math.sqrt(2)
    v = 0.7 * np.clip((r - 0.25) / 0.75, 0, 1) ** 1.6
    base = base * (1 - v[..., None]) + np.array([0x2F, 0x2C, 0x26], np.float32) * v[..., None]
    return base
_FULL = _stage_full()
_FLAT = np.zeros((H, W, 3), np.float32); _FLAT[:] = BG
_SRNG = np.random.default_rng(5)
def stage(a):
    base = _FLAT * (1 - a) + _FULL * a
    if 0.01 < a:
        n = (_SRNG.random(base.shape[:2], dtype=np.float32) + _SRNG.random(base.shape[:2], dtype=np.float32) - 1.0) * 1.2
        base = base + (n[..., None] if base.ndim == 3 else n) * min(1.0, a * 1.5)
    return Image.fromarray(np.clip(base + 0.5, 0, 255).astype(np.uint8), "RGB").convert("RGBA")

# ---------- sprites
def glow_rect(w, h, color, r, blur, galpha):
    pad = blur * 3
    im = Image.new("RGBA", (w + 2 * pad, h + 2 * pad), (0, 0, 0, 0))
    g = Image.new("L", im.size, 0); ImageDraw.Draw(g).rounded_rectangle((pad, pad, pad + w, pad + h), r, fill=255)
    g = g.filter(ImageFilter.GaussianBlur(blur)).point(lambda v: int(v * galpha))
    gl = Image.new("RGBA", im.size, color + (255,)); gl.putalpha(g); im.alpha_composite(gl)
    core = Image.new("RGBA", (w, h), color + (255,)); core.putalpha(rmask(w, h, r)); im.alpha_composite(core, (pad, pad))
    return im, pad
def caret(canvas, cx, cy, h=72, w=6, alpha=1.0):
    im, pad = glow_rect(max(2, int(w * S)), int(h * S), GOLD, max(1, int(w * S // 2)), 10 * S, 0.45)
    put(canvas, im, cx - im.width / S / 2, cy - im.height / S / 2, alpha)

@functools.lru_cache(maxsize=8)
def symbol_font(size):
    for p in ("/System/Library/Fonts/Supplemental/Arial Unicode.ttf", "/System/Library/Fonts/Apple Symbols.ttf", "/System/Library/Fonts/Helvetica.ttc"):
        try:
            f = ImageFont.truetype(p, size)
            if f.getmask("⌘").getbbox(): return f
        except Exception: pass
    return font("Outfit-Medium.ttf", size)
def keycap(canvas, x, y, pressed=False, alpha=1.0, s=1.0):
    PX = int(132 * s * S); im = Image.new("RGBA", (PX, PX), (0, 0, 0, 0)); d = ImageDraw.Draw(im)
    fill = INK if pressed else CARD
    d.rounded_rectangle((0, 0, PX - 1, PX - 1), int(28 * s * S), fill=fill + (255,), outline=None if pressed else BORDER + (255,), width=2 * S)
    ink = BG if pressed else INK
    lab = {"ja": "右", "zh": "右", "en": "R", "ko": "오른쪽"}[LANG]
    f1 = text_font("cjk", int(22 * s * S)); d.text((PX / 2, 36 * s * S), lab, font=f1, fill=(ink if pressed else MUTED) + (255,), anchor="mm")
    d.text((PX / 2, 84 * s * S), "⌘", font=symbol_font(int(52 * s * S)), fill=ink + (255,), anchor="mm")
    shadow(canvas, PX / S, PX / S, x, y, 28 * s, blur=24, off=10, alpha=0.35 * alpha)
    put(canvas, im, x, y, alpha)

def rounded_sprite(w, h, r, fill, outline=None, ow=1, SS=4, ringmix=0.55):
    big = Image.new("L", (w * SS, h * SS), 0); ImageDraw.Draw(big).rounded_rectangle((0, 0, w * SS - 1, h * SS - 1), r * SS, fill=255)
    mask = np.asarray(big.resize((w, h), Image.LANCZOS), np.float32) / 255
    rgb = np.zeros((h, w, 3), np.float32); rgb[:] = fill
    if outline is not None:
        inn = Image.new("L", (w * SS, h * SS), 0); ImageDraw.Draw(inn).rounded_rectangle((ow * SS, ow * SS, (w - ow) * SS - 1, (h - ow) * SS - 1), max(1, r - ow) * SS, fill=255)
        inner = np.asarray(inn.resize((w, h), Image.LANCZOS), np.float32) / 255
        ring = np.clip((mask - inner) / np.maximum(mask, 1e-3), 0, 1)[..., None] * ringmix
        rgb = rgb * (1 - ring) + np.array(outline, np.float32) * ring
    out = np.concatenate([rgb, mask[..., None] * 255], -1)
    return Image.fromarray(np.clip(out + 0.5, 0, 255).astype(np.uint8), "RGBA")

def chip(canvas, text, x, y, alpha=1.0, filler=False, size=22):
    f = text_font("cjk", size * S); tw = int(f.getlength(text)); w, h = tw + 36 * S, 44 * S
    im = rounded_sprite(w, h, 22 * S, CARD, BORDER, ow=S); d = ImageDraw.Draw(im)
    d.text((w / 2, h / 2 + S), text, font=f, fill=(MUTED if filler else INK) + (255,), anchor="mm")
    put(canvas, im, x, y, alpha); return w / S, h / S

def pill(canvas, full, shown, alpha, t_blink=0.0, y=626):
    f = text_font("cjk", 20 * S); tw = int(f.getlength(full)); w, h = tw + (18 + 8 + 10 + 22) * S, 48 * S
    im = rounded_sprite(w, h, 24 * S, BG, BORDER, ow=S); d = ImageDraw.Draw(im)
    d.ellipse((18 * S, 20 * S, 26 * S, 28 * S), fill=GOLD + (255,))
    txt = full[:shown]; d.text((36 * S, 24 * S), txt, font=f, fill=INK + (255,), anchor="lm")
    if shown < len(full) or int(t_blink * 2) % 2 == 0:
        cx = 36 * S + f.getlength(txt) + 2 * S; d.rectangle((cx, 14 * S, cx + 2 * S, 34 * S), fill=GOLD + (255,))
    x = (DW - w / S) / 2
    shadow(canvas, w / S, h / S, x, y, 24, blur=20, off=6, alpha=0.4 * alpha)
    put(canvas, im, x, y, alpha)
