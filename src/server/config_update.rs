use serde::Deserialize;

/// Update configuration
#[derive(serde::Deserialize)]
pub struct ConfigUpdate {
    // Router models
    pub default_model: String,
    pub background_model: Option<String>,
    pub think_model: Option<String>,
    pub websearch_model: Option<String>,
}