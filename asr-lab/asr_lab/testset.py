"""Synthetic benchmark clips synthesized with macOS `say`.

Lets the whole benchmark run without a microphone. Ground truth is stored in
audio/manifest.json next to the generated WAV files.
"""

from __future__ import annotations

import json
import subprocess

from .paths import AUDIO_DIR

CLIPS = [
    {
        "name": "zh_01",
        "voice": "Tingting",
        "lang": "zh",
        "text": "今天下午三点，我们在会议室讨论新版本的功能设计，然后把结果同步给测试团队。",
    },
    {
        "name": "en_01",
        "voice": "Samantha",
        "lang": "en",
        "text": "Let's ship the new release tomorrow morning, and sync the results with the QA team.",
    },
    {
        "name": "ja_01",
        "voice": "Kyoko",
        "lang": "ja",
        "text": "明日の午後三時に、会議室で新機能について話し合います。",
    },
    {
        "name": "ja_en_mix_01",
        "voice": "Kyoko",
        "lang": "mixed",
        "text": "このissueは次のsprintでfixする予定なので、まずquickなreviewをお願いします。",
    },
    {
        "name": "zh_en_mix_01",
        "voice": "Tingting",
        "lang": "mixed",
        "text": "这个需求先做一个quick prototype，然后我们再review一下design。",
    },
]


def _synthesize(clip: dict) -> None:
    wav = AUDIO_DIR / f"{clip['name']}.wav"
    # `say` writes a WAVE container for a .wav path; 16 kHz little-endian PCM16.
    subprocess.run(
        [
            "say",
            "-v",
            clip["voice"],
            "-o",
            str(wav),
            "--data-format=LEI16@16000",
            clip["text"],
        ],
        check=True,
        capture_output=True,
        text=True,
    )


def ensure_testset(force: bool = False) -> list[dict]:
    """Generate missing clips and (re)write the manifest; returns the manifest."""
    AUDIO_DIR.mkdir(parents=True, exist_ok=True)
    for clip in CLIPS:
        wav = AUDIO_DIR / f"{clip['name']}.wav"
        if force or not wav.exists():
            _synthesize(clip)
    manifest = [{**c, "path": str(AUDIO_DIR / f"{c['name']}.wav")} for c in CLIPS]
    (AUDIO_DIR / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2)
    )
    return manifest


def load_manifest() -> list[dict]:
    data = json.loads((AUDIO_DIR / "manifest.json").read_text())
    return data
