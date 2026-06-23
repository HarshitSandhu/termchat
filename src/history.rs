use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use chrono::Local;
use serde_json::{json, Value};

use crate::config::history_dir;

fn ensure_history_dir() -> Result<()> {
    fs::create_dir_all(history_dir())?;
    Ok(())
}

pub fn save_conversation(messages: &[Value], model: &str) -> Result<PathBuf> {
    ensure_history_dir()?;
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let filepath = history_dir().join(format!("{timestamp}.json"));
    let data = json!({"model": model, "messages": messages});
    fs::write(&filepath, serde_json::to_string_pretty(&data)?)?;
    Ok(filepath)
}

/// List saved conversation files, newest first (by filename).
pub fn list_conversations() -> Vec<PathBuf> {
    let _ = ensure_history_dir();
    let mut files: Vec<PathBuf> = match fs::read_dir(history_dir()) {
        Ok(entries) => entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files.reverse();
    files
}

pub fn load_conversation(filepath: &PathBuf) -> Result<(Vec<Value>, String)> {
    let data: Value = serde_json::from_str(&fs::read_to_string(filepath)?)?;
    let messages = data
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let model = data
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("llama3.1-8b")
        .to_string();
    Ok((messages, model))
}
