import sys, os, json, subprocess, math
import numpy as np
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from scenes import *   # LANG, KNOTS, DURATION, to_new, SPEAK_NEW0, TEXT, PILL

SR = 48000
AUD = os.path.join(WORK, "audio")
OUT = OUT_DIR
rng = np.random.default_rng(7)
N = int(DURATION * SR) + SR
sfx = np.zeros((N, 2), np.float32); mus = np.zeros((N, 2), np.float32); voc = np.zeros((N, 2), np.float32)

def put(buf, y, t, gain=1.0, pan=0.0):
    i = int(t * SR); y = np.asarray(y, np.float32)
    if i < 0 or i >= N: return
    y = y[: N - i]; l = math.cos((pan + 1) * math.pi / 4); r = math.sin((pan + 1) * math.pi / 4)
    buf[i:i + len(y), 0] += y * gain * l * 1.414; buf[i:i + len(y), 1] += y * gain * r * 1.414

def tt(d): return np.arange(int(d * SR)) / SR
def marimba(f, d=0.6):
    t = tt(d); return (np.sin(2 * np.pi * f * t) * np.exp(-t / 0.16) + 0.35 * np.sin(2 * np.pi * 4 * f * t) * np.exp(-t / 0.035) + 0.12 * np.sin(2 * np.pi * 9.2 * f * t) * np.exp(-t / 0.012)) * (1 - np.exp(-t / 0.002))
def glass(f, d=1.2):
    t = tt(d); y = sum(a * np.sin(2 * np.pi * f * r * t) * np.exp(-t / tau) for r, a, tau in ((1, 1, 0.45), (2.76, 0.35, 0.18), (5.4, 0.15, 0.08)))
    return y * (1 - np.exp(-t / 0.0015))
def thump(d=0.28):
    t = tt(d); f = 52 + 70 * np.exp(-t / 0.03); ph = 2 * np.pi * np.cumsum(f) / SR
    return np.sin(ph) * np.exp(-t / 0.09) + 0.25 * np.convolve(rng.standard_normal(len(t)), np.ones(30) / 30, "same") * np.exp(-t / 0.012)
def click(d=0.03, bright=1.0):
    t = tt(d); n = rng.standard_normal(len(t)); n = n - np.convolve(n, np.ones(int(6 / bright) + 1) / (int(6 / bright) + 1), "same")
    return n * np.exp(-t / 0.004)
def felt(d=0.09):
    t = tt(d); return np.sin(2 * np.pi * 180 * t) * np.exp(-t / 0.02) + 0.3 * click(d, 0.5)
def whoosh(d=0.5, rev=False, bright=0.5):
    n = rng.standard_normal(int(d * SR) + 200); dark = np.convolve(n, np.ones(60) / 60, "same")[:int(d * SR)]; lite = np.convolve(n, np.ones(10) / 10, "same")[:int(d * SR)]
    t = np.linspace(0, 1, int(d * SR)); env = np.sin(np.pi * t ** (0.6 if not rev else 1.6)) ** 2; k = (t if not rev else 1 - t) * bright
    y = (dark * (1 - k) + lite * k) * env; return y / (np.abs(y).max() + 1e-6)
def whoosh2(d=0.75, bright=0.6):
    n = rng.standard_normal(int(d * SR) + 400); x = np.linspace(0, 1, int(d * SR))
    dark = np.convolve(n, np.ones(110) / 110, "same")[:len(x)]; mid = np.convolve(n, np.ones(24) / 24, "same")[:len(x)]
    k = np.sin(np.pi * x) * bright; y = (dark * (1 - k) + mid * k) * np.sin(np.pi * x) ** 3
    return y / (np.abs(y).max() + 1e-6)
def tone(d, f):
    x = tt(d); return np.sin(2 * np.pi * f * x) * np.sin(np.pi * x / d) ** 2
def swell(d, f=293.66):
    t = tt(d); env = np.minimum(1, t / (d * 0.7)) ** 2 * np.minimum(1, (d - t) / 0.4)
    return sum(a * np.sin(2 * np.pi * f * r * t) for r, a in ((1, 1), (1.5, 0.5), (2, 0.4))) * env
PENT = [587.33, 659.25, 739.99, 880.0, 987.77, 1174.66]
D = lambda old, first=True: to_new(old, first)

# ---- cues (script time -> output time through the same warp as the picture)
put(sfx, thump(), D(3.5), 0.55); put(sfx, felt(), D(3.5), 0.5)                       # key press
n = len(TEXT["chips"]); kept = 0
for i, (_, _, _, f) in enumerate(TEXT["chips"]):
    t_in = SPEAK_NEW0 + 0.05 + CHIP_T[i]
    if f: put(sfx, marimba(PENT[1] * 0.97 * 0.5, 0.35), t_in, 0.10, rng.uniform(-0.8, 0.8))   # filler: dull, detuned, scattered
    else: put(sfx, marimba(PENT[[0, 2, 3, 4, 5, 3][kept % 6]]), t_in, 0.22, 0.0); kept += 1
fi = [i for i, c in enumerate(TEXT["chips"]) if c[3]]
for k in range(len(fi)): put(sfx, marimba(440 - 60 * k, 0.25), D(8.5 + k * 0.2, False), 0.08, 0.5 - k)    # fillers retract
put(sfx, felt(), D(8.95, False), 0.14); put(sfx, felt(), D(9.2, False), 0.18)                       # snap into a row
put(sfx, glass(PENT[3]), D(9.6, False), 0.20)                                                                      # clean line
put(sfx, glass(PENT[3]), D(10.45), 0.22); put(sfx, glass(PENT[5]), D(10.45) + 0.11, 0.2)             # check mark
for t0, f in zip((10.0, 12.5, 15.5, 19.0, 22.5), (293.66, 329.63, 369.99, 440.0, 493.88)):                          # scene change: one soft muted note, no noise
    put(sfx, marimba(f, 0.8), D(t0, False), 0.075); put(sfx, felt(0.07), D(t0, False) - 0.01, 0.06)
put(sfx, thump(), D(12.45), 0.40); put(sfx, felt(), D(12.45), 0.4)                                                   # command key
T3 = D(14.5); put(sfx, glass(PENT[2]), T3 + 0.6, 0.20); put(sfx, glass(PENT[4]), T3 + 0.71, 0.18)
T4 = D(18.2); put(sfx, felt(), T4 + 1.5, 0.30); put(sfx, glass(PENT[3]), T4 + 2.0, 0.17); put(sfx, glass(PENT[5]), T4 + 2.11, 0.14)                          # replaced
for old, p in ((23.12, 3), (23.35, 4)): put(sfx, marimba(PENT[p], 0.4), D(old), 0.18)                               # wordbook add
for old in (25.7, 26.83, 27.69): put(sfx, felt(), D(old), 0.32); put(sfx, marimba(PENT[2], 0.4), D(old), 0.10)      # models: tile, download
for old in (28.55, 28.8): put(sfx, felt(), D(old), 0.28)                                                             # history open
T5 = D(30.1); put(sfx, felt(), T5 + 0.2, 0.30); put(sfx, glass(PENT[3]), T5 + 0.65, 0.18); put(sfx, glass(PENT[5]), T5 + 0.76, 0.16)               # copy check
# typed intro lines
for t0, t1, k in PILL:
    full = TEXT[k]; shown = len(full)
    for c in range(0, shown, 2):
        t = D(t0 + c * 0.040, False)
        put(sfx, click(0.02, 1.2), t, 0.06 + 0.02 * rng.random(), rng.uniform(-0.2, 0.2))
# close: the music has already faded out, so the dot arrives into near silence
def pop(d=0.5):
    x = tt(d); f = 330 + 620 * (1 - np.exp(-x / 0.045)); ph = 2 * np.pi * np.cumsum(f) / SR
    return (np.sin(ph) * np.exp(-x / 0.11) + 0.25 * np.sin(2 * ph) * np.exp(-x / 0.05)) * (1 - np.exp(-x / 0.003))
def shine(d=0.8):
    n = rng.standard_normal(int(d * SR)); x = np.linspace(0, 1, len(n)); hp = n - np.convolve(n, np.ones(14) / 14, "same")
    return hp * np.sin(np.pi * x) ** 3 * (0.4 + 0.6 * x)
tdot = D(32.5, False)
put(sfx, pop(), tdot, 0.42); put(sfx, felt(0.06), tdot, 0.10)
put(sfx, glass(1174.66, 2.6), tdot + 0.05, 0.24); put(sfx, glass(1760.0, 1.8), tdot + 0.14, 0.10); put(sfx, marimba(293.66, 1.4), tdot, 0.16)
put(sfx, marimba(PENT[3], 0.5), D(33.0, False), 0.07); put(sfx, marimba(PENT[5], 0.5), D(33.4, False), 0.07)
tsw = D(34.55)                                                                                                      # light crosses the start button
put(sfx, glass(1567.98, 2.0), tsw + 0.30, 0.12); put(sfx, glass(2093.0, 1.4), tsw + 0.38, 0.05)

# ---- voices
def wavload(path):
    raw = subprocess.check_output(["ffmpeg", "-loglevel", "error", "-i", path, "-f", "f32le", "-ac", "1", "-ar", str(SR), "-"])
    return np.frombuffer(raw, np.float32)
vA = wavload(f"{AUD}/proc/vA_{LANG}.wav"); vB = wavload(f"{AUD}/proc/vB_{LANG}.wav")
tA = SPEAK_NEW0 + 0.05; tB = D(12.55)
put(voc, vA, tA, 0.9); put(voc, vB, tB, 0.9)

# ---- music: first DURATION seconds, fade in and out, ducked under voice
m = np.frombuffer(subprocess.check_output(["ffmpeg", "-loglevel", "error", "-i", f"{AUD}/music1.mp3", "-f", "f32le", "-ac", "2", "-ar", str(SR), "-"]), np.float32).reshape(-1, 2)
L = int(DURATION * SR); m = m[:L].copy(); t = np.arange(len(m)) / SR
m *= np.minimum(1, t / 1.5)[:, None] ** 1.5 * np.clip((tdot - 0.4 - t) / 3.4, 0, 1)[:, None] ** 1.4
mus[:len(m)] = m
def duck_env(buf_voice, depth_db, atk=0.06, rel=0.5):
    e = np.abs(buf_voice).max(axis=1); w = int(0.05 * SR); e = np.convolve(e, np.ones(w) / w, "same"); on = (e > 0.02).astype(np.float32)
    a, r = math.exp(-1 / (atk * SR)), math.exp(-1 / (rel * SR)); out = np.zeros_like(on); s = 0.0
    for i in range(0, len(on)):
        s = on[i] + (s - on[i]) * (a if on[i] > s else r); out[i] = s
    return 10 ** (-depth_db * out / 20)
ir_t = tt(1.6); ir = rng.standard_normal(len(ir_t)) * np.exp(-ir_t / 0.42); ir = np.convolve(ir, np.ones(6) / 6, "same"); ir /= np.abs(ir).sum() ** 0.5 * 4
nfft = 1 << int(np.ceil(np.log2(len(sfx) + len(ir))))
wet = np.stack([np.fft.irfft(np.fft.rfft(sfx[:, c], nfft) * np.fft.rfft(ir, nfft), nfft)[:len(sfx)] for c in range(2)], 1).astype(np.float32)
sfx = sfx + wet * 0.55
tr = np.arange(len(sfx)) / SR
room = np.convolve(rng.standard_normal(len(sfx)), np.ones(260) / 260, "same").astype(np.float32) * 0.9
room *= (0.7 + 0.3 * np.sin(2 * np.pi * tr / 9.0))
room *= np.minimum(1, tr / 1.0) * np.clip((DURATION - tr) / 2.5, 0, 1)
sfx[:, 0] += room * 0.0035; sfx[:, 1] += np.roll(room, 37) * 0.0035
dm = duck_env(voc[::1], 8.0)[:, None]; ds = duck_env(voc[::1], 4.0)[:, None]
mix = mus * 0.50 * dm + sfx * 1.0 * ds + voc * 1.0
np.save(f"{OUT}/mix-{LANG}.npy", mix.astype(np.float32)) if False else None
pk = np.abs(mix).max(); print("peak", pk)
raw = (mix[: int(DURATION * SR)] / max(1.0, pk * 1.05)).astype(np.float32)
open(f"{OUT}/mix-{LANG}.f32", "wb").write(raw.tobytes())
subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "f32le", "-ar", str(SR), "-ac", "2", "-i", f"{OUT}/mix-{LANG}.f32", "-af", "loudnorm=I=-14:TP=-1.5:LRA=9", "-ar", "48000", f"{OUT}/audio-{LANG}.wav"], check=True)
for src, dst in ((f"{OUT}/silent-{LANG}.mp4", f"{OUT}/dito-intro-{LANG}.mp4"), (f"{OUT}/silent-{LANG}-h264.mp4", f"{OUT}/dito-intro-{LANG}-h264.mp4"), (f"{OUT}/silent-{LANG}-app.mp4", f"{OUT}/dito-intro-{LANG}-app.mp4")):
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", src, "-i", f"{OUT}/audio-{LANG}.wav", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-shortest", "-movflags", "+faststart"] + (["-tag:v", "hvc1"] if "h264" not in dst else []) + [dst], check=True)
print("done", LANG)
