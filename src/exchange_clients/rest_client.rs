//! REST client skeleton.
//!
//! Wraps a [`reqwest::Client`] with exchange-specific authentication
//! (HMAC-SHA256 signature) and rate-limit awareness.
//! Concrete endpoint implementations live in exchange-specific adapters.

use anyhow::Result;
use reqwest::{Client, RequestBuilder};
use tracing::debug;

/// Authenticated REST client for a single exchange.
pub struct RestClient {
    name: String,
    base_url: String,
    api_key: String,
    api_secret: String,
    inner: Client,
}

impl RestClient {
    /// Construct a new REST client with keep-alive and TLS enabled.
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        api_secret: impl Into<String>,
    ) -> Result<Self> {
        let inner = Client::builder()
            .connection_verbose(false)
            .tcp_keepalive(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            name: name.into(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            api_secret: api_secret.into(),
            inner,
        })
    }

    /// Build a signed GET request for `path`.
    ///
    /// Concrete implementations should call this, append query parameters,
    /// then add the exchange-specific signature before `.send()`.
    pub fn get(&self, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        debug!(exchange = %self.name, url = %url, "GET");
        self.inner.get(url).header("X-API-KEY", &self.api_key)
    }

    /// Build a signed POST request for `path`.
    pub fn post(&self, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        debug!(exchange = %self.name, url = %url, "POST");
        self.inner.post(url).header("X-API-KEY", &self.api_key)
    }

    /// Build a signed DELETE request for `path`.
    pub fn delete(&self, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        debug!(exchange = %self.name, url = %url, "DELETE");
        self.inner.delete(url).header("X-API-KEY", &self.api_key)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn api_secret(&self) -> &str {
        &self.api_secret
    }
}
