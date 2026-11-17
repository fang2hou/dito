import os, sys, json, base64, urllib.request
KEY = os.environ["OR_KEY"]
model, out, prompt = sys.argv[1], sys.argv[2], sys.argv[3]
body = {"model": model, "modalities": ["text", "audio"], "stream": True, "messages": [{"role": "user", "content": prompt}]}
req = urllib.request.Request("https://openrouter.ai/api/v1/chat/completions", json.dumps(body).encode(), {"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"})
chunks, text, gid, other = [], "", None, set()
try:
    r = urllib.request.urlopen(req, timeout=300)
except urllib.error.HTTPError as e:
    print("ERR", e.code, e.read()[:400]); sys.exit(1)
for line in r:
    line = line.decode().strip()
    if not line.startswith("data:") or line.endswith("[DONE]"): continue
    try: j = json.loads(line[5:])
    except Exception: continue
    gid = gid or j.get("id")
    for ch in j.get("choices", []):
        d = ch.get("delta", {})
        a = d.get("audio")
        if a and a.get("data"): chunks.append(a["data"])
        if d.get("content"): text += d["content"] if isinstance(d["content"], str) else ""
        other |= set(d.keys())
raw = base64.b64decode("".join(chunks))
open(out, "wb").write(raw); print("bytes", len(raw), "id", gid, "delta keys", other, "text", text[:200])
