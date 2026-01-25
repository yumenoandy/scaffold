//! OpenAI Responses API client.
//!
//! See <https://platform.openai.com/docs/api-reference/responses> for API documentation.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::client::{
    check_status, create_agent, format_ureq_error, require_env_var, truncate_body, ClientConfig,
};
use crate::error::{ProviderError, Result};

pub struct Request<'a> {
    pub system_prompt: Option<&'a str>,
    pub user_content: &'a str,
    pub max_tokens: usize,
    pub response_format: Option<ResponseFormat>,
}

#[derive(Debug, Clone)]
pub enum ResponseFormat {
    JsonSchema {
        name: String,
        schema: serde_json::Value,
        strict: bool,
    },
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
    input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<ApiTextConfig>,
}

#[derive(Deserialize)]
struct ApiResponse {
    output: Option<Vec<ApiOutputItem>>,
    usage: Option<ApiUsage>,
    error: Option<ApiError>,
    status: Option<String>,
    #[serde(default)]
    incomplete_details: Option<ApiIncompleteDetails>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ApiOutputItem {
    #[serde(rename = "type")]
    item_type: String,
    content: Option<Vec<ApiOutputContent>>,
}

#[derive(Deserialize)]
struct ApiOutputContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
    #[serde(default)]
    json: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ApiUsage {
    input_tokens: usize,
    output_tokens: usize,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
    #[serde(rename = "type")]
    error_type: Option<String>,
}

#[derive(Deserialize)]
struct ApiIncompleteDetails {
    reason: Option<String>,
}

#[derive(Serialize)]
struct ApiTextConfig {
    format: ApiTextFormat,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ApiTextFormat {
    JsonSchema {
        name: String,
        schema: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
    },
}

/// OpenAI API client.
pub struct OpenAIProvider {
    client: ureq::Agent,
    api_key: String,
    model: String,
    max_tokens: usize,
    api_base: String,
}

impl OpenAIProvider {
    const DEFAULT_API_BASE: &'static str = "https://api.openai.com/v1";

    /// Create a new OpenAI provider.
    ///
    /// # Arguments
    /// * `model` - Model name (e.g., "gpt-4o")
    /// * `max_tokens` - Maximum tokens for responses (default: 16384)
    /// * `api_base` - Optional custom API base URL
    /// * `timeout_seconds` - Request timeout in seconds (default: 120)
    ///
    /// # Environment Variables
    /// * `OPENAI_API_KEY` - Required API key
    pub fn new(
        model: impl Into<String>,
        max_tokens: Option<usize>,
        api_base: Option<String>,
        timeout_seconds: Option<u64>,
    ) -> Result<Self> {
        let api_key = require_env_var("OPENAI_API_KEY")?;

        let config = ClientConfig {
            timeout: Duration::from_secs(timeout_seconds.unwrap_or(120)),
        };
        let client = create_agent(&config);

        Ok(Self {
            client,
            api_key,
            model: model.into(),
            max_tokens: max_tokens.unwrap_or(16384),
            api_base: api_base.unwrap_or_else(|| Self::DEFAULT_API_BASE.to_string()),
        })
    }

    /// Returns the provider name.
    pub fn name(&self) -> &str {
        "openai"
    }

    /// Returns the model name.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Send a completion request to the OpenAI Responses API.
    pub fn complete(&self, request: Request<'_>) -> Result<Response> {
        let api_request = ApiRequest {
            model: self.model.clone(),
            input: request.user_content.to_string(),
            instructions: request.system_prompt.map(|s| s.to_string()),
            max_output_tokens: Some(request.max_tokens.min(self.max_tokens)),
            text: request.response_format.as_ref().map(map_response_format),
        };

        let url = format!("{}/responses", self.api_base);

        let mut response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .send_json(&api_request)
            .map_err(|e| {
                ProviderError::network(format!("request failed: {}", format_ureq_error(&e)))
            })?;

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
            return Err(ProviderError::Api {
                status: 400,
                message: format!("{} (type: {:?})", error.message, error.error_type),
                error_type: error.error_type,
            });
        }

        if matches!(response.status.as_deref(), Some("incomplete")) {
            let reason = response
                .incomplete_details
                .as_ref()
                .and_then(|details| details.reason.as_deref())
                .unwrap_or("unknown");
            return Err(ProviderError::api(
                200,
                format!("response incomplete (reason: {})", reason),
            ));
        }

        // Extract content from response
        let mut parts = Vec::new();
        for content in response
            .output
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .flat_map(|item| item.content.as_deref().unwrap_or(&[]))
        {
            match content.content_type.as_str() {
                "output_text" => {
                    if let Some(text) = content.text.as_ref() {
                        parts.push(text.clone());
                    }
                }
                "output_json" => {
                    if let Some(json) = content.json.as_ref() {
                        if let Ok(serialized) = serde_json::to_string(json) {
                            parts.push(serialized);
                        }
                    }
                }
                _ => {}
            }
        }

        let content = parts.join("");
        if content.trim().is_empty() {
            return Err(ProviderError::parse(format!(
                "response contained no output_text. Raw response: {}",
                truncate_body(body, 2000)
            )));
        }

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

fn map_response_format(format: &ResponseFormat) -> ApiTextConfig {
    match format {
        ResponseFormat::JsonSchema {
            name,
            schema,
            strict,
        } => ApiTextConfig {
            format: ApiTextFormat::JsonSchema {
                name: name.clone(),
                schema: schema.clone(),
                strict: Some(*strict),
            },
        },
    }
}
