use std::env;
use std::fs;
use std::path::PathBuf;

pub const MAX_TOKENS: u32 = 4096;

/// Project root is the current working directory, mirroring how the Python
/// version anchored history and the last-model file to the repo root.
pub fn project_root() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn history_dir() -> PathBuf {
    project_root().join("history")
}

fn last_model_file() -> PathBuf {
    project_root().join(".last_model")
}

/// Load environment variables from a `.env` file in the project root, if present.
pub fn load_env() {
    dotenvy::from_path(project_root().join(".env")).ok();
    // Also try the default lookup (walks up from cwd) as a fallback.
    dotenvy::dotenv().ok();
}

pub fn tavily_api_key() -> String {
    env::var("TAVILY_API_KEY").unwrap_or_default()
}

pub fn load_last_model(fallback: &str) -> String {
    fs::read_to_string(last_model_file())
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

pub fn save_last_model(model: &str) {
    let _ = fs::write(last_model_file(), model);
}
