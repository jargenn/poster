use std::sync::LazyLock;

use http::{HeaderName, HeaderValue};
use serde::Deserialize;

static IDEMPOTENCY_KEY: LazyLock<HeaderName> =
    LazyLock::new(|| HeaderName::from_static("idempotency-key"));

#[derive(Debug, Deserialize)]
pub struct IdempotencyKey(String);

impl TryFrom<String> for IdempotencyKey {
    type Error = eyre::Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.is_empty() {
            eyre::bail!("The idempotency key cannot be empty")
        }
        let max_length = 50;
        if s.len() > max_length {
            eyre::bail!("The idempotencyw ket must be shorter than {max_length} characters.")
        }

        Ok(Self(s))
    }
}

impl From<IdempotencyKey> for String {
    fn from(key: IdempotencyKey) -> Self {
        key.0
    }
}

impl AsRef<str> for IdempotencyKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl axum_extra::headers::Header for IdempotencyKey {
    fn name() -> &'static http::HeaderName {
        &IDEMPOTENCY_KEY
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum_extra::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i http::HeaderValue>,
    {
        let value = values
            .next()
            .ok_or_else(axum_extra::headers::Error::invalid)?;

        let s = value
            .to_str()
            .map_err(|_| axum_extra::headers::Error::invalid())?;

        IdempotencyKey::try_from(s.to_owned()).map_err(|_| axum_extra::headers::Error::invalid())
    }

    fn encode<E: Extend<http::HeaderValue>>(&self, values: &mut E) {
        let value = HeaderValue::from_str(self.as_ref())
            .expect("IdempotencyKey must be a valid header value");

        values.extend(std::iter::once(value));
    }
}
