"""Trim silence, set the tempo and normalise the raw voice lines; records their lengths in voice.json.

    python3 process_voice.py <lang> [tempo for the dictation line]
"""
import json
import os
import subprocess
import sys

WORK = os.environ.get("FILM_WORK", "film-work")
OUT = os.environ.get("FILM_OUT", "film-out")
TEMPO = {"ja": 1.12, "zh": 1.1, "en": 1.0, "ko": 1.08}

lang = sys.argv[1]
tempo = float(sys.argv[2]) if len(sys.argv) > 2 else TEMPO[lang]
os.makedirs(f"{WORK}/audio/proc", exist_ok=True)
os.makedirs(OUT, exist_ok=True)
info = {}
for k in ("A", "B"):
    out = f"{WORK}/audio/proc/v{k}_{lang}.wav"
    t = tempo if k == "A" else 1.0
    trim = "silenceremove=start_periods=1:start_threshold=-45dB:start_silence=0.05"
    af = f"{trim},areverse,{trim},areverse,atempo={t},highpass=f=70,loudnorm=I=-19:TP=-2:LRA=7"
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", f"{WORK}/audio/raw/{lang}-{k}.mp3", "-af", af, "-ar", "48000", "-ac", "1", out], check=True)
    d = float(subprocess.check_output(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", out]))
    info[k] = round(d, 2)
path = f"{OUT}/voice.json"
data = json.load(open(path)) if os.path.exists(path) else {}
data[lang] = info
json.dump(data, open(path, "w"))
print(lang, info)
