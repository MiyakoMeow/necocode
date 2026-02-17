//! Model management for fetching, caching, and recommending Anthropic models.

use super::ApiError;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Model information from the Anthropic API.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelInfo {
    /// Model identifier (e.g., "claude-opus-4-6")
    pub id: String,
    /// Human-readable display name
    pub display_name: String,
    /// Creation timestamp
    pub created_at: String,
    /// Type of the resource
    #[serde(rename = "type")]
    pub model_type: String,
}

/// Response from the /v1/models endpoint.
#[derive(Debug, Deserialize)]
struct ModelsListResponse {
    /// List of available models
    data: Vec<ModelInfo>,
    /// Whether there are more results (for pagination)
    #[allow(dead_code)]
    has_more: bool,
}

/// Cached models data with timestamp.
#[derive(Debug, Serialize, Deserialize)]
struct ModelsCache {
    /// List of cached models
    models: Vec<ModelInfo>,
    /// Unix timestamp when cache was created
    cached_at: u64,
}

/// Model preference for intelligent recommendation.
#[derive(Debug, Clone, Copy)]
pub enum ModelPreference {
    /// Opus mode: prioritize Opus models (most intelligent)
    Opus,
    /// Sonnet mode: prioritize Sonnet models (speed + intelligence)
    Sonnet,
    /// Haiku mode: prioritize Haiku models (fastest and cheapest)
    Haiku,
}

/// Fetch available models from the Anthropic API with caching.
///
/// This function will:
/// 1. Check if a valid cache exists (24 hour TTL)
/// 2. If cache is valid, return cached models
/// 3. Otherwise, fetch from API and update cache
/// 4. If network fails, return empty list (graceful degradation)
///
/// # Arguments
///
/// * `client` - HTTP client for making requests
/// * `base_url` - Base URL for the Anthropic API
/// * `api_key` - API authentication key
///
/// # Returns
///
/// List of available models, or empty list on network failure.
pub async fn fetch_available_models(
    client: &HttpClient,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<ModelInfo>, ApiError> {
    // Check cache first
    if let Ok(cached) = load_cached_models()
        && is_cache_valid(&cached) {
        return Ok(cached.models);
    }

    // Fetch from API
    let response = client
        .get(format!("{}/v1/models", base_url))
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|e| ApiError::NetworkError(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(ApiError::HttpError {
            status: status.as_u16(),
            message: error_text,
        });
    }

    let models_response: ModelsListResponse = response
        .json()
        .await
        .map_err(|e| ApiError::ParseError(e.to_string()))?;

    // Save to cache
    save_models_to_cache(&models_response.data);

    Ok(models_response.data)
}

/// Recommend the best model based on preference and availability.
///
/// # Arguments
///
/// * `available_models` - List of models fetched from the API
/// * `preference` - Optional model preference (defaults to Opus)
///
/// # Returns
///
/// The recommended model ID, or None if no models are available.
pub fn recommend_model(
    available_models: &[ModelInfo],
    preference: Option<ModelPreference>,
) -> Option<String> {
    let pref = preference.unwrap_or(ModelPreference::Opus);

    // Priority lists (most preferred first)
    let priority_models = match pref {
        ModelPreference::Opus => {
            vec!["claude-opus-4-6", "claude-opus-4-5", "claude-sonnet-4-5"]
        }
        ModelPreference::Sonnet => {
            vec!["claude-sonnet-4-5", "claude-opus-4-6", "claude-haiku-4-5"]
        }
        ModelPreference::Haiku => {
            vec!["claude-haiku-4-5", "claude-sonnet-4-5", "claude-opus-4-5"]
        }
    };

    // Find first available model in priority list
    for model_id in priority_models {
        if available_models.iter().any(|m| m.id == model_id) {
            return Some(model_id.to_string());
        }
    }

    // Fallback: return first available model
    available_models.first().map(|m| m.id.clone())
}

/// Validate if a model ID exists in the available models list.
///
/// # Arguments
///
/// * `model_id` - The model ID to validate
/// * `available_models` - List of available models
///
/// # Returns
///
/// true if the model exists, false otherwise.
pub fn validate_model(model_id: &str, available_models: &[ModelInfo]) -> bool {
    available_models.iter().any(|m| m.id == model_id)
}

// === Helper functions ===

/// Get the cache file path (cross-platform).
fn get_cache_path() -> PathBuf {
    #[cfg(unix)]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".cache").join("necocode").join("models.json")
    }
    #[cfg(windows)]
    {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join(".cache")
            .join("necocode")
            .join("models.json")
    }
    #[cfg(not(any(unix, windows)))]
    {
        PathBuf::from(".cache").join("necocode").join("models.json")
    }
}

/// Load cached models from disk.
fn load_cached_models() -> Result<ModelsCache, Box<dyn std::error::Error>> {
    let path = get_cache_path();
    let content = std::fs::read_to_string(path)?;
    let cache: ModelsCache = serde_json::from_str(&content)?;
    Ok(cache)
}

/// Check if the cache is still valid (within 24 hours).
fn is_cache_valid(cache: &ModelsCache) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let cache_age = now.saturating_sub(cache.cached_at);
    cache_age < 24 * 60 * 60 // 24 hours
}

/// Save models to cache file.
fn save_models_to_cache(models: &[ModelInfo]) {
    let path = get_cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let cache = ModelsCache {
        models: models.to_vec(),
        cached_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    let _ = std::fs::write(&path, serde_json::to_string_pretty(&cache).unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recommend_model_performance() {
        let models = vec![
            ModelInfo {
                id: "claude-sonnet-4-5".to_string(),
                display_name: "Sonnet".to_string(),
                created_at: "2024-01-01".to_string(),
                model_type: "model".to_string(),
            },
            ModelInfo {
                id: "claude-opus-4-6".to_string(),
                display_name: "Opus".to_string(),
                created_at: "2025-01-01".to_string(),
                model_type: "model".to_string(),
            },
        ];

        let recommended = recommend_model(&models, Some(ModelPreference::Opus));
        assert_eq!(recommended, Some("claude-opus-4-6".to_string()));
    }

    #[test]
    fn test_recommend_model_economy() {
        let models = vec![
            ModelInfo {
                id: "claude-haiku-4-5".to_string(),
                display_name: "Haiku".to_string(),
                created_at: "2024-01-01".to_string(),
                model_type: "model".to_string(),
            },
        ];

        let recommended = recommend_model(&models, Some(ModelPreference::Haiku));
        assert_eq!(recommended, Some("claude-haiku-4-5".to_string()));
    }

    #[test]
    fn test_recommend_model_balanced() {
        let models = vec![
            ModelInfo {
                id: "claude-sonnet-4-5".to_string(),
                display_name: "Sonnet".to_string(),
                created_at: "2024-01-01".to_string(),
                model_type: "model".to_string(),
            },
            ModelInfo {
                id: "claude-opus-4-6".to_string(),
                display_name: "Opus".to_string(),
                created_at: "2025-01-01".to_string(),
                model_type: "model".to_string(),
            },
        ];

        let recommended = recommend_model(&models, Some(ModelPreference::Sonnet));
        assert_eq!(recommended, Some("claude-sonnet-4-5".to_string()));
    }

    #[test]
    fn test_validate_model_valid() {
        let models = vec![ModelInfo {
            id: "claude-opus-4-6".to_string(),
            display_name: "Opus".to_string(),
            created_at: "2025-01-01".to_string(),
            model_type: "model".to_string(),
        }];

        assert!(validate_model("claude-opus-4-6", &models));
        assert!(!validate_model("claude-opus-3-5", &models));
    }

    #[test]
    fn test_recommend_model_fallback() {
        let models = vec![
            ModelInfo {
                id: "claude-opus-4-1".to_string(),
                display_name: "Opus 4.1".to_string(),
                created_at: "2024-08-05".to_string(),
                model_type: "model".to_string(),
            },
        ];

        let recommended = recommend_model(&models, Some(ModelPreference::Opus));
        assert_eq!(recommended, Some("claude-opus-4-1".to_string()));
    }

    #[test]
    fn test_recommend_model_empty() {
        let models = vec![];
        let recommended = recommend_model(&models, Some(ModelPreference::Opus));
        assert!(recommended.is_none());
    }
}
