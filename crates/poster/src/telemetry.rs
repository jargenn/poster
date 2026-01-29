use opentelemetry::global;
use opentelemetry_otlp::WithHttpConfig;
use secrecy::ExposeSecret as _;
use std::collections::HashMap;
use std::time::Duration;

use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::{SpanExporterBuilder, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{BatchConfigBuilder, BatchSpanProcessor, SdkTracerProvider};
use tokio::task::JoinHandle;
use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

use crate::configuration::AppConfig;

pub fn init_tracing(config: &AppConfig) {
    LogTracer::init().expect("Failed to set logger");

    let honeycomb_layer = {
        let mut headers = HashMap::new();
        headers.insert(
            "x-honeycomb-team".to_owned(),
            config.tracing.api_key.expose_secret().to_string(),
        );

        match SpanExporterBuilder::default()
            .with_http()
            .with_protocol(config.tracing.protocol)
            .with_headers(headers)
            .with_endpoint(config.tracing.endpoint.clone())
            .build()
        {
            Ok(otlp_exporter) => {
                let batch_processor = BatchSpanProcessor::builder(otlp_exporter)
                    .with_batch_config(
                        BatchConfigBuilder::default()
                            .with_max_export_batch_size(512)
                            // .with_max_export_timeout(Duration::from_secs(30))
                            .with_scheduled_delay(Duration::from_millis(500))
                            .build(),
                    )
                    .build();

                let provider = SdkTracerProvider::builder()
                    .with_span_processor(batch_processor)
                    .with_resource(
                        Resource::builder()
                            .with_service_name(config.tracing.service_name.clone())
                            .build(),
                    )
                    .build();

                global::set_tracer_provider(provider.clone());

                Some(tracing_opentelemetry::layer().with_tracer(provider.tracer("poster")))
            }
            Err(err) => {
                tracing::warn!(
                    target: "telemetry",
                    error = %err,
                    "Telemetry exporter disabled"
                );
                None
            }
        }
    };

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            EnvFilter::new("debug,opentelemetry_sdk=info,opentelemetry_otlp=info")
        } else {
            EnvFilter::new("info,opentelemetry_sdk=warn,opentelemetry_otlp=warn")
        }
    });

    let formatting_layer = BunyanFormattingLayer::new("poster".into(), std::io::stdout);

    let subscriber = Registry::default()
        .with(env_filter)
        .with(honeycomb_layer)
        .with(JsonStorageLayer)
        .with(formatting_layer);

    set_global_default(subscriber).expect("Failed to set subscriber");
}

pub fn spawn_blocking_with_tracing<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let current_span = tracing::Span::current();
    tokio::task::spawn_blocking(move || current_span.in_scope(f))
}
