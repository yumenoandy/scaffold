use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("configuration error: {message}")]
    Config {
        message: String,
        env_var: Option<String>,
    },

    #[error("network error: {message}")]
    Network {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("API error (HTTP {status}): {message}")]
    Api {
        status: u16,
        message: String,
        error_type: Option<String>,
    },

    #[error("parse error: {message}")]
    Parse {
        message: String,
        #[source]
        source: Option<serde_json::Error>,
    },
}

impl ProviderError {
    pub fn missing_env_var(name: &str) -> Self {
        Self::Config {
            message: format!("{} environment variable not set", name),
            env_var: Some(name.to_string()),
        }
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::Network {
            message: message.into(),
            source: None,
        }
    }

    pub fn api(status: u16, message: impl Into<String>) -> Self {
        Self::Api {
            status,
            message: message.into(),
            error_type: None,
        }
    }

    pub fn api_with_type(
        status: u16,
        message: impl Into<String>,
        error_type: impl Into<String>,
    ) -> Self {
        Self::Api {
            status,
            message: message.into(),
            error_type: Some(error_type.into()),
        }
    }

    pub fn parse(message: impl Into<String>) -> Self {
        Self::Parse {
            message: message.into(),
            source: None,
        }
    }

    pub fn parse_with_source(message: impl Into<String>, source: serde_json::Error) -> Self {
        Self::Parse {
            message: message.into(),
            source: Some(source),
        }
    }
}

pub type Result<T> = std::result::Result<T, ProviderError>;
