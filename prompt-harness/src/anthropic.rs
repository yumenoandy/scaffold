//! Anthropic Messages API client.
//!
//! See <https://docs.anthropic.com/en/api/messages> for API documentation.

use serde::{Deserialize, Serialize};

use crate::client::{check_status, create_default_agent, require_env_var};
use crate::error::{ProviderError, Result};

pub struct Request<'a> {
    pub system_prompt: Option<&'a str>,
    pub user_content: &'a str,
    pub max_tokens: usize,
}

#[derive(Debug)]
pub struct Response {
    pub content: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
}

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: usize,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ApiContent>,
    usage: Option<ApiUsage>,
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct ApiContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiUsage {
    input_tokens: usize,
    output_tokens: usize,
}

#[derive(Deserialize)]
struct ApiError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

/// Anthropic API client for Claude models.
pub struct AnthropicProvider {
    client: ureq::Agent,
    api_key: String,
    model: String,
    max_tokens: usize,
}

impl AnthropicProvider {
    const API_ENDPOINT: &'static str = "https://api.anthropic.com/v1/messages";
    const API_VERSION: &'static str = "2023-06-01";

    /// Create a new Anthropic provider.
    ///
    /// # Arguments
    /// * `model` - Model name (e.g., "claude-sonnet-4-20250514")
    /// * `max_tokens` - Maximum tokens for responses (default: 8192)
    ///
    /// # Environment Variables
    /// * `ANTHROPIC_API_KEY` - Required API key
    pub fn new(model: impl Into<String>, max_tokens: Option<usize>) -> Result<Self> {
        let api_key = require_env_var("ANTHROPIC_API_KEY")?;
        let client = create_default_agent();

        Ok(Self {
            client,
            api_key,
            model: model.into(),
            max_tokens: max_tokens.unwrap_or(8192),
        })
    }

    /// Returns the provider name.
    pub fn name(&self) -> &str {
        "anthropic"
    }

    /// Returns the model name.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Send a completion request to the Anthropic Messages API.
    pub fn complete(&self, request: Request<'_>) -> Result<Response> {
        let api_request = ApiRequest {
            model: self.model.clone(),
            max_tokens: request.max_tokens.min(self.max_tokens),
            messages: vec![ApiMessage {
                role: "user".to_string(),
                content: request.user_content.to_string(),
            }],
            system: request.system_prompt.map(|s| s.to_string()),
        };

        let mut response = self
            .client
            .post(Self::API_ENDPOINT)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", Self::API_VERSION)
            .header("Content-Type", "application/json")
            .send_json(&api_request)
            .map_err(|e| ProviderError::network(format!("request failed: {}", e)))?;

        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| ProviderError::network(format!("failed to read response: {}", e)))?;
        check_status(status, &body)?;

        self.parse_response(&body)
    }

    fn parse_response(&self, body: &str) -> Result<Response> {
        let response: ApiResponse = serde_json::from_str(body).map_err(|e| {
            ProviderError::parse(format!("failed to parse response: {}. Body: {}", e, body))
        })?;

        if let Some(error) = response.error {
            return Err(ProviderError::api_with_type(
                400,
                error.message,
                error.error_type,
            ));
        }

        let content = response
            .content
            .iter()
            .filter(|c| c.content_type == "text")
            .filter_map(|c| c.text.clone())
            .collect::<Vec<_>>()
            .join("");

        let usage = response.usage.unwrap_or(ApiUsage {
            input_tokens: 0,
            output_tokens: 0,
        });

        Ok(Response {
            content,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
        })
    }
}
