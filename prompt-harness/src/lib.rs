//! Simple LLM API clients for Anthropic, OpenAI, Gemini, and Mistral OCR.
//!
//! This crate provides straightforward API clients for popular LLM providers.
//! Each provider has its own module with provider-specific request and response types.
//!
//! # Providers
//!
//! - [`anthropic::AnthropicProvider`] - Claude models via Messages API
//! - [`openai::OpenAIProvider`] - GPT models via Responses API
//! - [`gemini::GeminiProvider`] - Gemini models via GenerativeAI API
//! - [`mistral::MistralOcrProvider`] - Document OCR via Mistral OCR API
//!
//! # Example
//!
//! ```ignore
//! use prompt_harness::anthropic::{AnthropicProvider, Request};
//!
//! let provider = AnthropicProvider::new("claude-sonnet-4-20250514", None)?;
//! let response = provider.complete(Request {
//!     system_prompt: Some("You are a helpful assistant."),
//!     user_content: "What is 2 + 2?",
//!     max_tokens: 100,
//! })?;
//! println!("{}", response.content);
//! ```

pub mod anthropic;
pub mod gemini;
pub mod mistral;
pub mod openai;

mod client;
mod error;

pub use error::{ProviderError, Result};
