//! Mistral OCR API client.
//!
//! See <https://docs.mistral.ai/capabilities/document_ai/basic_ocr> for API documentation.

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::client::{check_status, create_default_agent, require_env_var};
use crate::error::{ProviderError, Result};

fn is_false(b: &bool) -> bool {
    !*b
}

pub struct Request<'a> {
    pub document: &'a [u8],
    pub mime_type: &'a str,
    pub pages: Option<&'a [u32]>,
    pub include_images: bool,
    pub image_limit: Option<u32>,
    pub image_min_size: Option<u32>,
    pub table_format: Option<TableFormat>,
    pub extract_headers: bool,
    pub extract_footers: bool,
}

impl Default for Request<'_> {
    fn default() -> Self {
        Self {
            document: &[],
            mime_type: "application/pdf",
            pages: None,
            include_images: false,
            image_limit: None,
            image_min_size: None,
            table_format: None,
            extract_headers: false,
            extract_footers: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TableFormat {
    #[default]
    Markdown,
    Html,
}

/// The parsed OCR response.
#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub pages: Vec<Page>,
    pub model: String,
    pub pages_processed: u32,
    pub doc_size_bytes: Option<u64>,
}

/// Result of `process_raw()`: the parsed response plus the raw API JSON body.
#[derive(Debug, Clone)]
pub struct ProcessResult {
    pub response: Response,
    pub raw_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub index: u32,
    pub markdown: String,
    pub dimensions: Option<PageDimensions>,
    pub images: Vec<Image>,
    pub tables: Vec<Table>,
    pub hyperlinks: Vec<Hyperlink>,
    pub header: Option<String>,
    pub footer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageDimensions {
    pub dpi: Option<u32>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub id: String,
    pub bbox: ImageBbox,
    pub image_base64: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageBbox {
    pub top_left_x: u32,
    pub top_left_y: u32,
    pub bottom_right_x: u32,
    pub bottom_right_y: u32,
}

impl ImageBbox {
    pub fn width(&self) -> u32 {
        self.bottom_right_x.saturating_sub(self.top_left_x)
    }

    pub fn height(&self) -> u32 {
        self.bottom_right_y.saturating_sub(self.top_left_y)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub content: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hyperlink {
    pub text: String,
    pub url: String,
}

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    document: ApiDocument,
    #[serde(skip_serializing_if = "Option::is_none")]
    pages: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "is_false")]
    include_image_base64: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_min_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    table_format: Option<TableFormat>,
    #[serde(skip_serializing_if = "is_false")]
    extract_header: bool,
    #[serde(skip_serializing_if = "is_false")]
    extract_footer: bool,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ApiDocument {
    DocumentUrl { document_url: String },
    ImageUrl { image_url: ApiImageUrl },
}

#[derive(Serialize)]
struct ApiImageUrl {
    url: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    pages: Vec<ApiPage>,
    model: String,
    usage_info: Option<ApiUsageInfo>,
}

#[derive(Deserialize)]
struct ApiPage {
    index: u32,
    markdown: String,
    #[serde(default)]
    images: Vec<ApiImage>,
    #[serde(default)]
    dimensions: Option<PageDimensions>,
    #[serde(default)]
    tables: Option<Vec<ApiTable>>,
    #[serde(default)]
    hyperlinks: Option<Vec<ApiHyperlink>>,
    #[serde(default)]
    header: Option<String>,
    #[serde(default)]
    footer: Option<String>,
}

#[derive(Deserialize)]
struct ApiImage {
    id: String,
    #[serde(default)]
    top_left_x: u32,
    #[serde(default)]
    top_left_y: u32,
    #[serde(default)]
    bottom_right_x: u32,
    #[serde(default)]
    bottom_right_y: u32,
    #[serde(default)]
    image_base64: Option<String>,
}



#[derive(Deserialize)]
struct ApiTable {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    markdown: Option<String>,
    #[serde(default)]
    html: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ApiHyperlink {
    Simple(String),
    Structured { text: String, url: String },
}

#[derive(Deserialize)]
struct ApiUsageInfo {
    pages_processed: u32,
    #[serde(default)]
    doc_size_bytes: Option<u64>,
}



impl From<ApiImage> for Image {
    fn from(api: ApiImage) -> Self {
        Self {
            id: api.id,
            bbox: ImageBbox {
                top_left_x: api.top_left_x,
                top_left_y: api.top_left_y,
                bottom_right_x: api.bottom_right_x,
                bottom_right_y: api.bottom_right_y,
            },
            image_base64: api.image_base64,
        }
    }
}

impl From<ApiTable> for Table {
    fn from(api: ApiTable) -> Self {
        // Prefer html, then markdown, then content
        if let Some(html) = api.html {
            Self {
                content: html,
                format: "html".to_string(),
            }
        } else if let Some(md) = api.markdown {
            Self {
                content: md,
                format: "markdown".to_string(),
            }
        } else {
            Self {
                content: api.content.unwrap_or_default(),
                format: "text".to_string(),
            }
        }
    }
}

impl From<ApiHyperlink> for Hyperlink {
    fn from(api: ApiHyperlink) -> Self {
        match api {
            ApiHyperlink::Simple(url) => Self {
                text: url.clone(),
                url,
            },
            ApiHyperlink::Structured { text, url } => Self { text, url },
        }
    }
}

impl From<ApiPage> for Page {
    fn from(api: ApiPage) -> Self {
        Self {
            index: api.index,
            markdown: api.markdown,
            dimensions: api.dimensions,
            images: api.images.into_iter().map(Into::into).collect(),
            tables: api
                .tables
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            hyperlinks: api
                .hyperlinks
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            header: api.header,
            footer: api.footer,
        }
    }
}

/// Mistral OCR API client for document processing.
pub struct MistralOcrProvider {
    client: ureq::Agent,
    api_key: String,
    model: String,
}

impl MistralOcrProvider {
    const API_ENDPOINT: &'static str = "https://api.mistral.ai/v1/ocr";

    /// Create a new Mistral OCR provider.
    ///
    /// # Arguments
    /// * `model` - Model name (e.g., "mistral-ocr-2512" for OCR 3)
    ///
    /// # Environment Variables
    /// * `MISTRAL_API_KEY` - Required API key
    pub fn new(model: impl Into<String>) -> Result<Self> {
        let api_key = require_env_var("MISTRAL_API_KEY")?;
        let client = create_default_agent();

        Ok(Self {
            client,
            api_key,
            model: model.into(),
        })
    }

    /// Create a provider with the latest OCR model (mistral-ocr-2512).
    pub fn latest() -> Result<Self> {
        Self::new("mistral-ocr-2512")
    }

    /// Returns the provider name.
    pub fn name(&self) -> &str {
        "mistral"
    }

    /// Returns the model name.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Process a document with OCR.
    ///
    /// # Example
    /// ```ignore
    /// let provider = MistralOcrProvider::latest()?;
    /// let pdf_bytes = std::fs::read("document.pdf")?;
    /// let response = provider.process(&Request {
    ///     document: &pdf_bytes,
    ///     mime_type: "application/pdf",
    ///     include_images: true,
    ///     table_format: Some(TableFormat::Html),
    ///     ..Default::default()
    /// })?;
    /// for page in response.pages {
    ///     println!("Page {}: {}", page.index, page.markdown);
    /// }
    /// ```
    pub fn process(&self, request: &Request<'_>) -> Result<Response> {
        self.process_raw(request).map(|r| r.response)
    }

    /// Process a document with OCR, returning both the parsed response and the
    /// raw API JSON body. Use this when you want to save the unprocessed
    /// response for later replay via [`Self::parse_raw_response`].
    pub fn process_raw(&self, request: &Request<'_>) -> Result<ProcessResult> {
        let document = self.create_document(request.document, request.mime_type)?;

        let api_request = ApiRequest {
            model: self.model.clone(),
            document,
            pages: request.pages.map(|p| p.to_vec()),
            include_image_base64: request.include_images,
            image_limit: request.image_limit,
            image_min_size: request.image_min_size,
            table_format: request.table_format,
            extract_header: request.extract_headers,
            extract_footer: request.extract_footers,
        };

        let mut response = self
            .client
            .post(Self::API_ENDPOINT)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .send_json(&api_request)
            .map_err(|e| ProviderError::network(format!("request failed: {}", e)))?;

        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| ProviderError::network(format!("failed to read response: {}", e)))?;
        check_status(status, &body)?;

        let parsed = Self::parse_raw_response(&body)?;
        Ok(ProcessResult {
            response: parsed,
            raw_json: body,
        })
    }

    /// Parse a raw API response JSON string into a [`Response`].
    ///
    /// Use this to replay a previously saved response without making an API call.
    pub fn parse_raw_response(body: &str) -> Result<Response> {
        let api_response: ApiResponse = serde_json::from_str(body).map_err(|e| {
            ProviderError::parse(format!("failed to parse response: {}. Body: {}", e, body))
        })?;

        let usage = api_response.usage_info.unwrap_or(ApiUsageInfo {
            pages_processed: api_response.pages.len() as u32,
            doc_size_bytes: None,
        });

        Ok(Response {
            pages: api_response.pages.into_iter().map(Into::into).collect(),
            model: api_response.model,
            pages_processed: usage.pages_processed,
            doc_size_bytes: usage.doc_size_bytes,
        })
    }

    fn create_document(&self, bytes: &[u8], mime_type: &str) -> Result<ApiDocument> {
        let base64_data = base64::engine::general_purpose::STANDARD.encode(bytes);
        let data_url = format!("data:{};base64,{}", mime_type, base64_data);

        if mime_type == "application/pdf" {
            Ok(ApiDocument::DocumentUrl {
                document_url: data_url,
            })
        } else if mime_type.starts_with("image/") {
            Ok(ApiDocument::ImageUrl {
                image_url: ApiImageUrl { url: data_url },
            })
        } else {
            Err(ProviderError::Config {
                message: format!(
                    "unsupported MIME type: {}. Expected application/pdf or image/*",
                    mime_type
                ),
                env_var: None,
            })
        }
    }

}
