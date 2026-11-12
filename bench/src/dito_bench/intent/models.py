"""The models the intent bench covers, and where their GGUF files are."""

from pathlib import Path

MODEL_KEYS = ("qwen3.5-2b", "qwen3.5-4b", "qwen3.5-9b")
# Dito's own model folders, release build first.
APP_MODEL_DIRS = tuple(
    Path.home() / "Library/Application Support" / bundle / "models"
    for bundle in ("com.fang2hou.Dito", "com.fang2hou.Dito-dev")
)


def catalog_id(key: str) -> str:
    if key not in MODEL_KEYS:
        msg = f"unknown model {key!r}; available: {', '.join(MODEL_KEYS)}"
        raise SystemExit(msg)
    return f"{key}-instruct"


def gguf_name(key: str) -> str:
    size = key.rsplit("-", 1)[1].upper()
    return f"Qwen3.5-{size}-Q4_K_M.gguf"


def short_name(key: str) -> str:
    """`qwen3.5-4b` as `4b`, for chart labels."""
    return key.rsplit("-", 1)[1]


def resolve_gguf(key: str, override: Path | None = None) -> Path:
    """The GGUF of a model key: `override`, else the file in Dito's model folder."""
    if override is not None:
        if not override.is_file():
            msg = f"{override} does not exist"
            raise FileNotFoundError(msg)
        return override
    candidates = [d / catalog_id(key) / gguf_name(key) for d in APP_MODEL_DIRS]
    for path in candidates:
        if path.is_file():
            return path
    msg = (
        f"no GGUF for {key}; looked for {', '.join(str(p) for p in candidates)}. "
        "Download the model in Dito or pass --gguf."
    )
    raise FileNotFoundError(msg)
