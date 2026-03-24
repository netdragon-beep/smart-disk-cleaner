use serde::{Deserialize, Serialize};

/// Application-level configuration persisted as TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    /// Large-file threshold in megabytes (default 512).
    #[serde(default = "default_large_file_threshold_mb")]
    pub large_file_threshold_mb: u64,

    /// Maximum number of items sent to the AI advisor.
    #[serde(default = "default_max_ai_items")]
    pub max_ai_items: usize,

    /// OpenAI-compatible API key (optional).
    #[serde(default)]
    pub api_key: Option<String>,

    /// AI base URL.
    #[serde(default = "default_ai_base_url")]
    pub ai_base_url: String,

    /// AI model name.
    #[serde(default = "default_ai_model")]
    pub ai_model: String,

    /// Whether file-level AI explanation must use remote AI and never silently fall back.
    #[serde(default)]
    pub strict_file_ai_remote_only: bool,

    /// Glob patterns to exclude from scanning.
    #[serde(default)]
    pub exclude_patterns: Vec<String>,

    /// UI theme: "light" or "dark".
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            large_file_threshold_mb: default_large_file_threshold_mb(),
            max_ai_items: default_max_ai_items(),
            api_key: None,
            ai_base_url: default_ai_base_url(),
            ai_model: default_ai_model(),
            strict_file_ai_remote_only: false,
            exclude_patterns: Vec::new(),
            theme: default_theme(),
        }
    }
}

fn default_large_file_threshold_mb() -> u64 {
    512
}

fn default_max_ai_items() -> usize {
    20
}

fn default_ai_base_url() -> String {
    "https://api.openai.com".to_string()
}

fn default_ai_model() -> String {
    "gpt-4.1-mini".to_string()
}

fn default_theme() -> String {
    "dark".to_string()
}

impl AppConfig {
    /// Load config from a TOML file. Returns default config if the file doesn't exist.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&text)?;
        Ok(config)
    }

    /// Save config to a TOML file.
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }
}
