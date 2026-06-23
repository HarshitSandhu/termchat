use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::project_root;

/// A model provider exposing an OpenAI-compatible chat completions endpoint.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Provider {
    Cerebras,
    OpenRouter,
    Fireworks,
}

impl Provider {
    pub const ALL: [Provider; 3] = [
        Provider::Cerebras,
        Provider::OpenRouter,
        Provider::Fireworks,
    ];

    /// Stable identifier used for persistence.
    pub fn id(self) -> &'static str {
        match self {
            Provider::Cerebras => "cerebras",
            Provider::OpenRouter => "openrouter",
            Provider::Fireworks => "fireworks",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Provider::Cerebras => "Cerebras",
            Provider::OpenRouter => "OpenRouter",
            Provider::Fireworks => "Fireworks AI",
        }
    }

    pub fn base_url(self) -> &'static str {
        match self {
            Provider::Cerebras => "https://api.cerebras.ai/v1/chat/completions",
            Provider::OpenRouter => "https://openrouter.ai/api/v1/chat/completions",
            Provider::Fireworks => "https://api.fireworks.ai/inference/v1/chat/completions",
        }
    }

    /// Environment variable consulted as a fallback when no key is saved.
    pub fn env_var(self) -> &'static str {
        match self {
            Provider::Cerebras => "CEREBRAS_API_KEY",
            Provider::OpenRouter => "OPENROUTER_API_KEY",
            Provider::Fireworks => "FIREWORKS_API_KEY",
        }
    }

    pub fn popular_models(self) -> &'static [&'static str] {
        match self {
            Provider::Cerebras => &[
                "gpt-oss-120b",
                "zai-glm-4.7",
                "qwen-3-235b-a22b-instruct-2507",
                "llama3.1-8b",
            ],
            Provider::OpenRouter => &[
                "openai/gpt-4o",
                "anthropic/claude-3.5-sonnet",
                "meta-llama/llama-3.1-70b-instruct",
                "google/gemini-flash-1.5",
            ],
            Provider::Fireworks => &[
                "accounts/fireworks/models/llama-v3p1-8b-instruct",
                "accounts/fireworks/models/llama-v3p1-70b-instruct",
                "accounts/fireworks/models/qwen2p5-72b-instruct",
                "accounts/fireworks/models/mixtral-8x22b-instruct",
            ],
        }
    }

    pub fn default_model(self) -> &'static str {
        self.popular_models()[0]
    }

    /// Parse a provider from an id or display name (case-insensitive).
    pub fn from_loose(s: &str) -> Option<Provider> {
        let s = s.trim().to_lowercase();
        Provider::ALL
            .into_iter()
            .find(|p| p.id() == s || p.display_name().to_lowercase() == s)
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Settings {
    #[serde(default)]
    provider: String,
    #[serde(default)]
    keys: HashMap<String, String>,
}

fn settings_path() -> PathBuf {
    project_root().join(".termchat.json")
}

fn load_settings() -> Settings {
    fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(settings: &Settings) {
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(settings_path(), json);
    }
}

/// The currently selected provider (defaults to Cerebras).
pub fn current_provider() -> Provider {
    Provider::from_loose(&load_settings().provider).unwrap_or(Provider::Cerebras)
}

pub fn set_provider(provider: Provider) {
    let mut settings = load_settings();
    settings.provider = provider.id().to_string();
    save_settings(&settings);
}

/// Resolve the API key for a provider: a saved key takes precedence, otherwise
/// fall back to the provider's environment variable.
pub fn api_key_for(provider: Provider) -> String {
    let settings = load_settings();
    if let Some(key) = settings.keys.get(provider.id()) {
        if !key.is_empty() {
            return key.clone();
        }
    }
    env::var(provider.env_var()).unwrap_or_default()
}

pub fn set_api_key(provider: Provider, key: &str) {
    let mut settings = load_settings();
    settings
        .keys
        .insert(provider.id().to_string(), key.to_string());
    save_settings(&settings);
}

/// Mask a secret for display, e.g. `sk-a…3f9`.
pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        "•".repeat(chars.len().max(1))
    } else {
        let head: String = chars[..4].iter().collect();
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{head}…{tail}")
    }
}
