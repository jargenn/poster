use opentelemetry_otlp::Protocol;
use secrecy::SecretString;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct TracingSettings {
    pub api_key: SecretString,
    pub protocol: Protocol,
    pub endpoint: String,
    pub service_name: String,
}
