from core import *

TEXT = {
 "ja": dict(wordbook="よく使う言葉を登録。認識と表記がそろう。", models="ローカルもクラウドも、自由に選べる。", history="話した内容は、いつでも見返せる。", trans="会議の音声を、その場で書き起こし。", chat="ローカルのモデルと、安心してチャット。",
   chips=[("えーと",150,176,1),("来週の水曜日に",330,228,0),("あの",640,190,1),("お打ち合わせの",230,314,0),("時間を",520,330,0),("いただけますか",690,270,0)],
   clean="来週の水曜日に、お打ち合わせの時間をいただけますか。"),
 "en": dict(wordbook="Add your own words. Recognition follows.", models="Local or cloud, your choice.", history="Everything you said stays here.", trans="Meetings, transcribed live.", chat="Chat with a local model.",
   chips=[("Um",150,176,1),("could we",320,228,0),("find a time",560,186,0),("uh",760,252,1),("to meet",260,314,0),("next Wednesday",470,332,0)],
   clean="Could we find a time to meet next Wednesday?"),
 "ko": dict(wordbook="자주 쓰는 단어를 등록하세요. 인식이 따라옵니다.", models="로컬도 클라우드도, 자유롭게 고르세요.", history="말한 내용은 언제든 다시 볼 수 있어요.", trans="회의 음성을 그 자리에서 받아 적습니다.", chat="로컬 모델과 안심하고 대화하세요.",
   chips=[("음",150,176,1),("다음 주",320,228,0),("수요일에",560,186,0),("저기",760,252,1),("회의 시간을",260,314,0),("잡을 수 있을까요?",470,332,0)],
   clean="다음 주 수요일에 회의 시간을 잡을 수 있을까요?"),
 "zh": dict(wordbook="添加常用词，识别更准确。", models="本地或云端，自由选择。", history="说过的话，随时可回看。", trans="会议内容，实时转写。", chat="与本地模型安心聊天。",
   chips=[("呃",150,176,1),("下周三",330,228,0),("那个",640,190,1),("方便",250,314,0),("抽出时间",430,332,0),("开个会吗",660,290,0)],
   clean="下周三方便抽出时间开个会吗？"),
}[LANG]

D, C = hud_seq("dictation"), hud_seq("command")
DE, CE = ed_seq("dictation"), ed_seq("command")
TM = Seq(f"{REAL}/tools_{LANG}/meeting-*.png"); TC = Seq(f"{REAL}/tools_{LANG}/chat-*.png")
SW = Seq(f"{REAL}/intro_{LANG}/wordbook-??????.png"); SM = Seq(f"{REAL}/intro_{LANG}/models-??????.png"); SH = Seq(f"{REAL}/intro_{LANG}/history-??????.png")
# The offscreen capture is one device pixel taller than the live window, which centres the page half a point lower.
WELCOME = Image.open(f"{REAL}/intro_{LANG}/welcome-dark.png").convert("RGB").crop((0, 1, DW * S, DH * S + 1))

# ---- welcome sprites cut from the real capture, alpha measured against the flat window colour
def _sprite(box, color, bg=BG):
    reg = np.asarray(WELCOME.crop(box), np.float32)
    diff = np.abs(reg - np.array(bg, np.float32)).max(axis=2)
    ref = np.abs(np.array(color, np.float32) - np.array(bg, np.float32)).max()
    a = np.clip(diff / ref, 0, 1)
    im = Image.new("RGBA", (box[2] - box[0], box[3] - box[1]), color + (255,)); im.putalpha(Image.fromarray((a * 255).astype(np.uint8), "L")); return im
def _find_boxes():
    arr = np.asarray(WELCOME, np.float32); d = np.abs(arr - np.array(BG, np.float32)).max(axis=2) > 24
    return d
_B = lambda *b: tuple(v * S for v in b)
SP_TEXT = _sprite(_B(400, 268, 607, 336), INK)
SP_DOT = _sprite(_B(607, 296, 644, 333), (0x28, 0xA7, 0x88))
SP_TAG = _sprite(_B(380, 348, 660, 382), MUTED)
_wa = np.asarray(WELCOME, np.float32); _wd = (np.abs(_wa - np.array(BG, np.float32)).max(axis=2) > 20)[420 * S:500 * S]
_wc = np.where(_wd.any(axis=0))[0]; _wr = np.where(_wd.any(axis=1))[0]
BX0, BX1, BY0, BY1 = int(_wc[0]), int(_wc[-1]), 420 * S + int(_wr[0]), 420 * S + int(_wr[-1]); BW, BH = BX1 - BX0 + 1, BY1 - BY0 + 1
_btn = WELCOME.crop((BX0 - 6 * S, BY0 - 6 * S, BX1 + 7 * S, BY1 + 7 * S)).convert("RGBA"); _m = Image.new("L", _btn.size, 0)
ImageDraw.Draw(_m).rounded_rectangle((6 * S, 6 * S, 6 * S + BW - 1, 6 * S + BH - 1), BH // 2, fill=255); _btn.putalpha(_m.filter(ImageFilter.GaussianBlur(0.4 * S))); SP_BTN = _btn
XT, YT = 400, 268; XD, YD = 607, 296; XG, YG = 380, 348; XB, YB = BX0 / S - 6, BY0 / S - 6

_bm = Image.new("L", (W, H), 0); ImageDraw.Draw(_bm).rounded_rectangle((BX0, BY0, BX1, BY1), BH // 2, fill=255); _BM = np.asarray(_bm.filter(ImageFilter.GaussianBlur(0.5 * S)), np.float32) / 255
def sweep_button(cv, p):
    if p <= 0 or p >= 1: return
    arr = np.asarray(cv.convert("RGB"), np.float32).copy()
    x0, x1 = BX0 - 40 * S, BX1 + 40 * S; xc = lerp(x0, x1, ease(p))
    yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
    band = np.exp(-(((xx - xc) + (yy - (BY0 + BH / 2)) * 0.55) / (26.0 * S)) ** 2)
    lift = math.sin(math.pi * p) ** 2
    fill = np.clip((arr.mean(axis=2) - 150) / 40, 0, 1)
    w = _BM * fill * (0.34 * band + 0.07 * lift)
    arr = arr + (255 - arr) * w[..., None]
    cv.paste(Image.fromarray(np.clip(arr + 0.5, 0, 255).astype(np.uint8), "RGB").convert("RGBA"))

def pingpong(x, a, b):
    p = (b - a); u = x % (2 * p); return a + (u if u < p else 2 * p - u)

def hud_place(cv, im, xc, yc, s=1.0, alpha=1.0):
    k = HUDCAP / 4   # crop pixels per design pixel
    dw, dh = im.width / k, im.height / k
    place(cv, im, (xc - dw * s / 2) * S, (yc - dh * s / 2) * S, s * S / k, alpha)

def blend_pair(seqA, tA, seqB_t):  # not used
    pass

def win_at(cv, img, x, y, s, r=20, alpha=1.0, m=1.0, dy=0.0):
    iw, ih = img.width / CAP, img.height / CAP
    cx, cy = x + iw * s / 2, y + ih * s / 2
    sm = s * m
    window(cv, img, cx - iw * sm / 2, cy - ih * sm / 2 + dy, sm, r, alpha)

EVT = {"ja": dict(stop=14985, end=16920, chat0=629, settle=4344), "en": dict(stop=13646, end=15474, chat0=2079, settle=8890), "zh": dict(stop=13376, end=15310, chat0=596, settle=3839), "ko": dict(stop=14415, end=16300, chat0=876, settle=5049)}[LANG]
M0 = EVT["stop"] - 1500
CS = {"ko": 1546}.get(LANG, 0)   # the Korean command takes about 1.5 s longer before the text is replaced
CRATE = min(1.6, max(1.0, (EVT["settle"] - EVT["chat0"]) / 4000))
def chat_src(): return min(EVT["settle"] + 300, EVT["chat0"] + max(0, TN - to_new(19.3)) * 1000 * CRATE)

def hist_src(t):
    if t < 30.1: return 1200 + (t - 28.0) * 1000
    return min(4900, 3300 + (TN - to_new(30.1)) * 1000)

def rise(t, t0, d=0.5):
    k = eout(ramp(t, t0, t0 + d)); return ease(ramp(t, t0, t0 + d * 0.8)), lerp(0.975, 1.0, k), lerp(12, 0, k)
def away(t, t0, d=0.4):
    k = ease(ramp(t, t0, t0 + d)); return 1 - k, lerp(1.0, 0.97, k), lerp(0, 8, k)

def cam(t, keys):
    if t <= keys[0][0]: return keys[0][1:]
    for (t0, *a), (t1, *b) in zip(keys, keys[1:]):
        if t <= t1:
            x = min(1, max(0, (t - t0) / (t1 - t0))) if t1 > t0 else 1; k = x * x * x * (x * (x * 6 - 15) + 10)
            return tuple(lerp(p, q, k) for p, q in zip(a, b))
    return keys[-1][1:]

def pieces(t, plist):
    # plist: (t0, t1, s0, s1); returns [(src, weight)] with 0.1 s cross-fades at cuts
    out = []
    for i, (t0, t1, s0, s1) in enumerate(plist):
        if t0 <= t < t1:
            src = s0 + (t - t0) / (t1 - t0) * (s1 - s0); w = 1.0
            if i + 1 < len(plist) and t > t1 - 0.1:
                nt0, _, ns0, ns1 = plist[i + 1]; k = (t - (t1 - 0.1)) / 0.1
                return [(src, 1 - k), (ns0, k)]
            return [(src, 1.0)]
    last = plist[-1]; return [(last[3], 1.0)]

def mixed(seq, srcs):
    imgs = [(seq.img(s), w) for s, w in srcs]
    if len(imgs) == 1: return imgs[0][0]
    return Image.blend(imgs[0][0], imgs[1][0], imgs[1][1])

def mixed_hud(seq, srcs):
    if len(srcs) == 1: return [(seq.hud(srcs[0][0]), 1.0)]
    return [(seq.hud(s), w) for s, w in srcs]

MODEL_PIECES = [(25.0, 25.8, 300, 1100), (25.8, 26.8, 1500, 2900), (26.8, 28.0, 3200, 5300)]
PILL = [(16.0, 19.0, "trans"), (19.4, 22.4, "chat"), (22.8, 25.0, "wordbook"), (25.2, 28.0, "models"), (28.2, 30.5, "history")]
def draw_pill(cv, t):
    for t0, t1, k in PILL:
        if t0 - 0.2 <= t <= t1:
            full = TEXT[k]; shown = int(max(0, (t - t0)) / 0.040)
            al = ramp(t, t0 - 0.2, t0 - 0.05) * (1 - ramp(t, t1 - 0.25, t1))
            pill(cv, full, min(len(full), shown), al, t_blink=t); return

SET_KEYS = [(22.5,72,24,.8),(23.3,72,24,.8),(24.3,-157,25,1),(25.0,-157,25,1),(26.1,-157,80,1),(26.8,-157,80,1),(27.5,-157,60,1),(27.8,-157,60,1),(28.9,-157,-20,1),(30.2,-157,-20,1),(31.0,72,24,.8)]
TR_KEYS = [(15.5,88,30,.9),(15.9,88,30,.9),(16.9,40,-132,1.0),(18.2,40,-132,1.0),(18.9,88,30,.9)]
CH_KEYS = [(19.0,88,30,.9),(19.8,88,30,.9),(20.9,40,130,1.0),(22.0,40,130,1.0),(22.7,88,30,.9)]

def render(t):
    a = ramp(t, 1.0, 1.8) * (1 - ramp(t, 32.0, 33.6))
    cv = stage(a)
    # ---------- Hook: caret, key hint
    if t < 3.6:
        wind = ease(ramp(t, 3.0, 3.5)); vis = (1.0 <= t) and ((t - 1.0) % 1.0 < 0.5) or t >= 3.0
        if t >= 1.0 and vis:
            al = 1.0 - ramp(t, 3.42, 3.56)
            caret(cv, 520, 360, h=72 + 26 * wind, w=6 - 1.0 * wind, alpha=al)
    # key
    if 2.0 <= t < 8.3:
        if t < 3.5: kal = 0.55 * ramp(t, 2.0, 2.4); pr = False
        else:
            kal = lerp(0.55, 1.0, ramp(t, 3.4, 3.6)) * (1 - ramp(t, 8.0, 8.3)); pr = t < 3.62
        keycap(cv, 78, 500, pressed=pr, alpha=kal, s=0.94 if pr else 1.0)
    # ---------- dictation chips and clean line
    if 5.5 <= t < 10.1:
        kept = []
        for i, (txt, x, y, f) in enumerate(TEXT["chips"]):
            tn_in = SPEAK_NEW0 + 0.05 + CHIP_T[i]; k = eout(ramp(TN, tn_in, tn_in + 0.26)); al = ease(ramp(TN, tn_in, tn_in + 0.18)) * (0.78 if f else 1.0)
            ft = font("MPLUS1p-Regular.ttf", 22)
            if f:
                t_out = 8.5 + [0, 2, 7, 8, 3, 1][0] * 0  # fillers retract one per beat from 8.5
                order = [i2 for i2, c in enumerate(TEXT["chips"]) if c[3]].index(i)
                t_out = 8.5 + order * 0.2; al *= 1 - ease(ramp(t, t_out, t_out + 0.2))
                if al > 0.01: chip(cv, txt, x, y + 8 * (1 - k), al, True)
            else: kept.append((i, txt, x, y, k, al))
        row = [(txt, int(text_font("cjk", 22).getlength(txt)) + 36) for _, txt, *_ in kept]
        tot = sum(w for _, w in row) + 14 * (len(row) - 1); x0 = (DW - tot) / 2; cx = x0
        snap = eout(ramp(t, 8.9, 9.25)); fade = 1 - ramp(t, 9.45, 9.65)
        for (i, txt, x, y, k, al_k), (_, w) in zip(kept, row):
            tx, ty = lerp(x, cx, snap), lerp(y, 300, snap); cx += w + 14
            chip(cv, txt, tx, ty + 8 * (1 - k), al_k * fade, False)
        if t >= 9.6:
            f30 = text_font("cjk", 30 * S); line = TEXT["clean"]
            im = Image.new("RGBA", (int(f30.getlength(line)) + 8 * S, 48 * S), (0, 0, 0, 0)); ImageDraw.Draw(im).text((4 * S, 24 * S), line, font=f30, fill=INK + (255,), anchor="lm")
            put(cv, im, (DW - im.width / S) / 2, 286, ramp(t, 9.6, 9.9) * (1 - ramp(t, 10.0, 10.2)))
    # ---------- dictation HUD
    if 3.5 <= t < 12.5:
        if t < 5.5: src = 540 + (t - 3.5) * 1000; pos = (520, 360, 1.0)
        elif t < 8.0: src = pingpong((TN - SPEAK_NEW0) * 1000, 1100, 3300); mv = ease(ramp(t, 5.5, 5.85)); pos = (520, lerp(360, 520, mv), 1.0)
        elif t < 10.0: src = 3600 + (t - 8.0) * 1000; mv = ease(ramp(t, 8.0, 8.3)); pos = (520, lerp(520, 500, mv), 1.0)
        else: src = 5600 + (t - 10.0) * 1000; mv = ease(ramp(t, 10.0, 11.0)); pos = (520, lerp(500, 585, mv), lerp(1.0, 0.65, mv))
        hud_place(cv, D.hud(src), *pos)
    # ---------- dictation editor (Insert)
    if 10.0 <= t < 12.6:
        al, m, dy = rise(t, 10.0); al2, m2, dy2 = away(t, 12.25)
        img = DE.img(5600 + (t - 10.0) * 1000).convert("RGB")
        win_at(cv, img, 160, 150, 1.0, 16, alpha=al * al2, m=m * m2, dy=dy + dy2)
    # ---------- Command
    if 12.5 <= t < 15.7:
        plist = [(12.5, 13.5, 1000, 2000), (13.5, 14.5, 3900 + CS, 4900 + CS), (14.5, 15.7, 5300 + CS, 6500 + CS)]
        srcs = pieces(t, plist)
        if t >= 14.5: srcs = [(min(5300 + CS + (TN - to_new(14.5)) * 1000, 6858 + CS), 1.0)]
        al, m, dy = rise(t, 12.5); al2, m2, dy2 = away(t, 15.4)
        eimg = mixed(CE, srcs).convert("RGB"); win_at(cv, eimg, 50, 190, 0.9, 16, alpha=al * al2, m=m * m2, dy=dy + dy2)
        for src, w in mixed_hud(C, srcs): hud_place(cv, src, 820, 330, 0.8, alpha=w * al2)
    # ---------- Tools: transcription
    if 15.5 <= t < 19.15:
        al, m, dy = rise(t, 15.5); al2, m2, dy2 = away(t, 18.8)
        srcs = pieces(t, [(15.5, 17.4, 4400, 5900), (17.4, 18.2, M0 - 800, M0), (18.2, 19.2, M0, M0 + 1000)])
        if t >= 18.2: srcs = [(min(M0 + (TN - to_new(18.2)) * 1000, EVT["end"]), 1.0)]
        img = mixed(TM, srcs).convert("RGB"); x, y, s = cam(t, TR_KEYS)
        win_at(cv, img, x, y, s, 20, alpha=al * al2, m=m * m2, dy=dy + dy2)
    # ---------- Tools: chat
    if 19.0 <= t < 22.65:
        al, m, dy = rise(t, 19.0); al2, m2, dy2 = away(t, 22.3)
        src = chat_src()
        img = TC.img(src).convert("RGB"); x, y, s = cam(t, CH_KEYS)
        win_at(cv, img, x, y, s, 20, alpha=al * al2, m=m * m2, dy=dy + dy2)
    # ---------- Settings tour
    if 22.5 <= t < 31.3:
        al, m, dy = rise(t, 22.5); al2, m2, dy2 = away(t, 30.5, 0.7)
        if t < 25.0: srcs = [(SW.path(400 + (t - 22.5) * 1000), 1.0)]; imgs = [(SW.img(400 + (t - 22.5) * 1000), 1.0)]
        elif t < 28.0:
            sp = pieces(t, MODEL_PIECES); imgs = [(SM.img(s_), w) for s_, w in sp]
        else: imgs = [(SH.img(hist_src(t)), 1.0)]
        # page cross-fade at 25.0 and 28.0
        def page_img(tt):
            if tt < 25.0: return SW.img(400 + (tt - 22.5) * 1000)
            if tt < 28.0: return mixed(SM, pieces(tt, MODEL_PIECES))
            return SH.img(hist_src(tt))
        img = page_img(t)
        for edge, other in ((25.0, lambda: SW.img(2900)), (28.0, lambda: SM.img(5300))):
            if edge <= t < edge + 0.25:
                img = Image.blend(other(), img, ease((t - edge) / 0.25))
        x, y, s = cam(t, SET_KEYS)
        win_at(cv, img.convert("RGB"), x, y, s, 20, alpha=al * al2, m=m * m2, dy=dy + dy2)
    draw_pill(cv, t)
    # ---------- Close: wordmark hand-over
    if t >= 31.7:
        k1 = ramp(t, 31.7, 32.1)
        shift = 12 * (1 - eout(ramp(t, 32.5, 32.78)))
        put(cv, SP_TEXT, XT + shift, YT, k1)
        if t >= 32.5:
            p = ramp(t, 32.5, 32.85); sc = 1.0 + (0.18 * math.sin(p * math.pi) if p < 1 else 0) ; sc *= eout(min(1, p * 1.6))
            place(cv, SP_DOT, XD * S + SP_DOT.width * (1 - sc) / 2, YD * S + SP_DOT.height * (1 - sc) / 2, max(0.05, sc), 1.0)
        kt = ease(ramp(t, 33.0, 33.35)); put(cv, SP_TAG, XG, YG + 6 * (1 - kt), kt)
        kb = ease(ramp(t, 33.4, 33.75)); put(cv, SP_BTN, XB, YB + 6 * (1 - kb), kb)
        kf = ramp(t, 34.2, 34.4)
        if kf > 0: put(cv, WELCOME.convert("RGBA"), 0, 0, kf)
        sweep_button(cv, ramp(t, 34.6, 35.2))
    return cv.convert("RGB")

# ---------- retiming: output time -> script time. Equal script values are holds on settled frames.
import json as _json
_VOICE = _json.load(open(os.path.join(OUT_DIR, "voice.json")))[LANG]
SPEAK_NEW0 = 6.4
CHIP_T = _json.load(open(os.path.join(OUT_DIR, "voice_words.json")))[LANG]
S_END = SPEAK_NEW0 + _VOICE["A"] + 0.3
_head = [(0, 0), (4.5, 3.5), (SPEAK_NEW0, 5.5), (S_END, 8.0), (S_END + 0.4, 8.4), (S_END + 0.9, 8.4)]
_E = S_END + 0.9
_mid = [(_E + 1.55, 9.95), (_E + 2.45, 9.95), (_E + 2.5, 10.0), (_E + 4.1, 11.6), (_E + 5.7, 11.6), (_E + 6.6, 12.5)]
_DELTA = _E + 6.6 - 15.9
_tail = [(19.2, 14.9), (21.0, 14.9), (21.6, 15.5), (25.75, 18.2), (26.35, 18.6), (30.05, 18.6), (30.25, 18.8), (30.75, 19.3),
         (34.75, 21.9), (35.65, 21.9), (36.05, 22.3), (36.25, 22.5),
         (38.55, 24.2), (39.25, 24.2), (40.25, 25.0), (43.25, 26.8), (44.85, 27.8), (45.65, 27.8), (45.85, 28.0),
         (48.75, 30.1), (51.15, 30.1), (51.55, 30.5), (56.0, 34.5), (56.7, 35.2), (57.5, 35.2)]
KNOTS = _head + _mid + [(n + _DELTA, o) for n, o in _tail]
DURATION = KNOTS[-1][0]
_NEW = [k[0] for k in KNOTS]
def warp(tn):
    if tn >= _NEW[-1]: return KNOTS[-1][1]
    i = bisect.bisect_right(_NEW, tn) - 1
    (n0, o0), (n1, o1) = KNOTS[i], KNOTS[i + 1]
    return o0 + (o1 - o0) * (tn - n0) / (n1 - n0)
def to_new(t_old, first=True):
    pts = [n for n, o in KNOTS if abs(o - t_old) < 1e-9]
    if pts: return pts[0] if first else pts[-1]
    for (n0, o0), (n1, o1) in zip(KNOTS, KNOTS[1:]):
        if o0 < t_old < o1: return n0 + (n1 - n0) * (t_old - o0) / (o1 - o0)
TN = 0.0
render_old = render
HOLDS = [(a[0], b[0]) for a, b in zip(KNOTS, KNOTS[1:]) if a[1] == b[1] and b[0] - a[0] > 0.4 and b[0] < DURATION - 1.0]
def drift(tn, amp=0.0):
    best = 0.0
    for n0, n1 in HOLDS:
        if tn < n0: b = 0.0
        elif tn <= n1: b = ease((tn - n0) / (n1 - n0))
        else: b = max(0.0, 1 - ease((tn - n1) / 0.8))
        best = max(best, b)
    return 1 + amp * best
def dither(im, tn):
    t = warp(tn); a = ramp(t, 1.0, 1.8) * (1 - ramp(t, 31.8, 33.2))
    if a <= 0.01: return im
    rg = np.random.default_rng(int(round(tn * 1000)) + 11)
    n = (rg.random((H, W), dtype=np.float32) + rg.random((H, W), dtype=np.float32) - 1.0) * (0.6 * a)
    arr = np.asarray(im, np.float32) + n[..., None]
    return Image.fromarray(np.clip(arr + 0.5, 0, 255).astype(np.uint8), "RGB")
def render(tn):
    global TN
    TN = tn; im = render_old(warp(tn)); z = 1.0
    if False:
        w, h = W / z, H / z; im = im.resize((W, H), Image.BICUBIC, box=((W - w) / 2, (H - h) / 2, (W + w) / 2, (H + h) / 2))
    return dither(im, tn)
