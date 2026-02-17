from pathlib import Path
from dotenv import load_dotenv
import os

PROJECT_ROOT = Path(__file__).resolve().parent.parent
HISTORY_DIR = PROJECT_ROOT / "history"

load_dotenv(PROJECT_ROOT / ".env")

OPENROUTER_API_KEY = os.getenv("OPENROUTER_API_KEY", "")
TAVILY_API_KEY = os.getenv("TAVILY_API_KEY", "")

_FALLBACK_MODEL = "openai/gpt-4o"
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
    "openai/gpt-4o",
    "openai/gpt-4o-mini",
    "anthropic/claude-sonnet-4",
    "anthropic/claude-haiku-4",
    "google/gemini-2.0-flash-001",
    "google/gemini-2.5-pro-preview",
    "meta-llama/llama-4-maverick",
    "deepseek/deepseek-chat-v3-0324",
    "mistralai/mistral-large",
]
