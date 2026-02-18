from pathlib import Path
from dotenv import load_dotenv
import os

PROJECT_ROOT = Path(__file__).resolve().parent.parent
HISTORY_DIR = PROJECT_ROOT / "history"

load_dotenv(PROJECT_ROOT / ".env")

CEREBRAS_API_KEY = os.getenv("CEREBRAS_API_KEY", "")
TAVILY_API_KEY = os.getenv("TAVILY_API_KEY", "")

_FALLBACK_MODEL = "llama3.1-8b"
_MODEL_FILE = PROJECT_ROOT / ".last_model"
MAX_TOKENS = 4096


def load_last_model() -> str:
    try:
        return _MODEL_FILE.read_text().strip()
    except FileNotFoundError:
        return _FALLBACK_MODEL


def save_last_model(model: str):
    _MODEL_FILE.write_text(model)

POPULAR_MODELS = [
    "llama3.1-8b",
    "gpt-oss-120b",
    "qwen-3-235b-a22b-instruct-2507",
    "zai-glm-4.7",
]
