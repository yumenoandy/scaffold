//! Google Gemini API client.
//!
//! See <https://ai.google.dev/api/generate-content> for API documentation.

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
    contents: Vec<ApiContent>,
    #[serde(rename = "generationConfig")]
    generation_config: ApiGenerationConfig,
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<ApiContent>,
}

#[derive(Serialize)]
struct ApiContent {
    parts: Vec<ApiPart>,
}

#[derive(Serialize)]
struct ApiPart {
    text: String,
}

#[derive(Serialize)]
struct ApiGenerationConfig {
    temperature: f32,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: usize,
}

#[derive(Deserialize)]
struct ApiResponse {
    candidates: Option<Vec<ApiCandidate>>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<ApiUsage>,
    error: Option<ApiError>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ApiCandidate {
    content: Option<ApiResponseContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ApiResponseContent {
    parts: Vec<ApiResponsePart>,
}

#[derive(Deserialize)]
struct ApiResponsePart {
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiUsage {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<usize>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<usize>,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
    status: Option<String>,
}

/// Google Gemini API client.
pub struct GeminiProvider {
    client: ureq::Agent,
    api_key: String,
    model: String,
    max_tokens: usize,
}

impl GeminiProvider {
    const API_BASE: &'static str = "https://generativelanguage.googleapis.com/v1beta/models";

    /// Create a new Gemini provider.
    ///
    /// # Arguments
    /// * `model` - Model name (e.g., "gemini-2.0-flash")
    /// * `max_tokens` - Maximum tokens for responses (default: 16384)
    ///
    /// # Environment Variables
    /// * `GEMINI_API_KEY` - Required API key
    pub fn new(model: impl Into<String>, max_tokens: Option<usize>) -> Result<Self> {
        let api_key = require_env_var("GEMINI_API_KEY")?;
        let client = create_default_agent();

        Ok(Self {
            client,
            api_key,
            model: model.into(),
            max_tokens: max_tokens.unwrap_or(16384),
        })
    }

    /// Returns the provider name.
    pub fn name(&self) -> &str {
        "gemini"
    }

    /// Returns the model name.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Send a completion request to the Gemini API.
    pub fn complete(&self, request: Request<'_>) -> Result<Response> {
        let parts = vec![ApiPart {
            text: request.user_content.to_string(),
        }];

        let system_instruction = request.system_prompt.map(|prompt| ApiContent {
            parts: vec![ApiPart {
                text: prompt.to_string(),
            }],
        });

        let api_request = ApiRequest {
            contents: vec![ApiContent { parts }],
            generation_config: ApiGenerationConfig {
                temperature: 0.0,
                max_output_tokens: request.max_tokens.min(self.max_tokens),
            },
            system_instruction,
        };

        let url = format!("{}/{}:generateContent", Self::API_BASE, self.model);

        let mut response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
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
            return Err(ProviderError::Api {
                status: 400,
                message: format!("{} (status: {:?})", error.message, error.status),
                error_type: error.status,
            });
        }

        let content = response
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .and_then(|c| c.content.as_ref())
            .map(|c| {
                c.parts
                    .iter()
                    .filter_map(|p| p.text.clone())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        let (input_tokens, output_tokens) = response
            .usage_metadata
            .map(|u| {
                (
                    u.prompt_token_count.unwrap_or(0),
                    u.candidates_token_count.unwrap_or(0),
                )
            })
            .unwrap_or((0, 0));

        Ok(Response {
            content,
            input_tokens,
            output_tokens,
        })
    }
}
