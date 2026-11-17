"""Voice lines through OpenRouter's speech endpoint. Needs OR_KEY in the environment (read it from the owner's key file,
never print it or write it anywhere).

    python3 tts.py <lang> [model voice]      writes $FILM_WORK/audio/raw/<lang>-A.mp3 and -B.mp3
"""
import json
import os
import sys
import urllib.error
import urllib.request

WORK = os.environ.get("FILM_WORK", "film-work")
LINES = {
    "ja": {"A": "えーと、来週の水曜日に、あの、お打ち合わせの、時間を、いただけますか？", "B": "丁寧に書き直して"},
    "en": {"A": "Um, could we find a time, uh, to meet next Wednesday?", "B": "Rewrite this politely"},
    "zh": {"A": "呃，下周三，那个，方便，抽出时间，开个会吗？", "B": "改得礼貌一点"},
    "ko": {"A": "음, 다음 주 수요일에, 저기, 회의 시간을, 잡을 수 있을까요?", "B": "정중하게 다시 써 줘"},
}


def speak(model, voice, text, out):
    body = {"model": model, "input": text, "voice": voice, "response_format": "mp3"}
    req = urllib.request.Request(
        "https://openrouter.ai/api/v1/audio/speech",
        json.dumps(body).encode(),
        {"Authorization": f"Bearer {os.environ['OR_KEY']}", "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            open(out, "wb").write(r.read())
    except urllib.error.HTTPError as e:
        print("ERR", model, e.code, e.read()[:200])


if __name__ == "__main__":
    lang = sys.argv[1]
    model, voice = (sys.argv[2], sys.argv[3]) if len(sys.argv) > 3 else ("elevenlabs/eleven-v4-turbo", "sarah")
    os.makedirs(f"{WORK}/audio/raw", exist_ok=True)
    for k, text in LINES[lang].items():
        speak(model, voice, text, f"{WORK}/audio/raw/{lang}-{k}.mp3")
        print(lang, k, "done")
