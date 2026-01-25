use std::time::Duration;

use crate::error::{ProviderError, Result};

pub struct ClientConfig {
    pub timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
        }
    }
}

pub fn require_env_var(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| ProviderError::missing_env_var(name))
}

pub fn create_agent(config: &ClientConfig) -> ureq::Agent {
    let ureq_config = ureq::Agent::config_builder()
        .timeout_global(Some(config.timeout))
        .http_status_as_error(false)
        .build();
    ureq::Agent::new_with_config(ureq_config)
}

pub fn create_default_agent() -> ureq::Agent {
    create_agent(&ClientConfig::default())
}

pub fn check_status(status: u16, body: &str) -> Result<()> {
    if status >= 400 {
        Err(ProviderError::api(status, body.to_string()))
    } else {
        Ok(())
    }
}

pub fn truncate_body(body: &str, limit: usize) -> String {
    if body.len() <= limit {
        body.to_string()
    } else {
        format!("{}...[truncated]", &body[..limit])
    }
}

pub fn format_ureq_error(err: &ureq::Error) -> String {
    match err {
        ureq::Error::Timeout(t) => format!("timeout: {:?}", t),
        ureq::Error::Io(e) => format!("IO error: {}", e),
        ureq::Error::StatusCode(code) => format!("HTTP status error: {}", code),
        _ => err.to_string(),
    }
}
