import json
from datetime import datetime
from pathlib import Path

from termchat.config import HISTORY_DIR


def ensure_history_dir():
    HISTORY_DIR.mkdir(parents=True, exist_ok=True)


def save_conversation(messages: list[dict], model: str) -> Path:
    ensure_history_dir()
    timestamp = datetime.now().strftime("%Y-%m-%d_%H-%M-%S")
    filepath = HISTORY_DIR / f"{timestamp}.json"
    data = {"model": model, "messages": messages}
    filepath.write_text(json.dumps(data, indent=2))
    return filepath


def list_conversations() -> list[Path]:
    ensure_history_dir()
    files = sorted(HISTORY_DIR.glob("*.json"), reverse=True)
    return files


def load_conversation(filepath: Path) -> tuple[list[dict], str]:
    data = json.loads(filepath.read_text())
    return data["messages"], data.get("model", "openai/gpt-4o")
