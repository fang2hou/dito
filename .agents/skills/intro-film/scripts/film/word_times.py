"""When each word chip's word is spoken, from word timestamps of a transcription (OpenRouter, whisper-large-v3).

    python3 word_times.py <lang> chip1,chip2,... filler_index,filler_index

The transcription drops fillers, so a filler gets the time of the next matched chip minus 0.35 s. The result goes
into voice_words.json and is worth a look: adjust by hand where the transcript disagrees with the audio.
"""
import base64
import json
import os
import re
import sys
import urllib.request

WORK = os.environ.get("FILM_WORK", "film-work")
OUT = os.environ.get("FILM_OUT", "film-out")
lang, chips, fillers = sys.argv[1], sys.argv[2].split(","), {int(i) for i in sys.argv[3].split(",") if i}
audio = base64.b64encode(open(f"{WORK}/audio/proc/vA_{lang}.wav", "rb").read()).decode()
body = {"model": "openai/whisper-large-v3", "language": lang, "response_format": "verbose_json", "timestamp_granularities": ["word"], "input_audio": {"data": audio, "format": "wav"}}
req = urllib.request.Request("https://openrouter.ai/api/v1/audio/transcriptions", json.dumps(body).encode(), {"Authorization": f"Bearer {os.environ['OR_KEY']}", "Content-Type": "application/json"})
r = json.load(urllib.request.urlopen(req, timeout=120))
norm = lambda s: re.sub(r"[\s,.、。，？?！!]", "", s).lower()
chars = []
for w in r["words"]:
    t = norm(w["word"])
    for i, ch in enumerate(t):
        chars.append((ch, w["start"] + (w["end"] - w["start"]) * i / max(1, len(t))))
full = "".join(c for c, _ in chars)
starts, pos = [None] * len(chips), 0
for i, chip in enumerate(chips):
    if i in fillers:
        continue
    key = norm(chip)
    for plen in (min(3, len(key)), 2, 1):
        j = full.find(key[:plen], pos)
        if j >= 0:
            break
    if j >= 0:
        starts[i] = chars[j][1]
        pos = j + 1
starts[0] = 0.0
for i in range(len(chips)):
    if starts[i] is None:
        nxt = next((starts[k] for k in range(i + 1, len(chips)) if starts[k] is not None), r.get("duration", 4.0))
        starts[i] = max(starts[i - 1] + 0.2, nxt - 0.35)
print(r["text"], [round(x, 2) for x in starts])
path = f"{OUT}/voice_words.json"
data = json.load(open(path)) if os.path.exists(path) else {}
data[lang] = [round(x, 3) for x in starts]
json.dump(data, open(path, "w"))
