import os, sys, json, base64, urllib.request
KEY = os.environ["OR_KEY"]
def stt(path, lang, model="openai/whisper-large-v3-turbo"):
    fmt = path.rsplit(".", 1)[1]
    body = {"model": model, "language": lang, "input_audio": {"data": base64.b64encode(open(path, "rb").read()).decode(), "format": fmt}}
    req = urllib.request.Request("https://openrouter.ai/api/v1/audio/transcriptions", json.dumps(body).encode(), {"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"})
    try:
        return json.load(urllib.request.urlopen(req, timeout=120)).get("text")
    except urllib.error.HTTPError as e: return f"ERR {e.code} {e.read()[:150]}"
for p in sys.argv[1:]:
    lang = p.split("-")[1]; print(p, "->", stt(p, lang))
