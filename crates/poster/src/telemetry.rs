use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::{Protocol, WithExportConfig};
use tokio::task::JoinHandle;
use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

pub fn init_tracing() {
    LogTracer::init().expect("Failed to set logger");

    let opentelemetry_layer = {
        opentelemetry::global::set_text_map_propagator(
            opentelemetry_jaeger_propagator::Propagator::new(),
        );

        let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint("http://localhost:4318/v1/traces")
            .with_protocol(Protocol::HttpBinary)
            .build()
            .expect("Failed to build the SpanExporter");
        let batch_processor =
            opentelemetry_sdk::trace::BatchSpanProcessor::builder(otlp_exporter).build();

        let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_span_processor(batch_processor)
            .with_resource(
                opentelemetry_sdk::Resource::builder()
                    .with_service_name("poster")
                    .build(),
            )
            .build();

        opentelemetry::global::set_tracer_provider(tracer_provider.clone());

        let tracer = tracer_provider.tracer("poster");
        tracing_opentelemetry::layer().with_tracer(tracer)
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
        .with(opentelemetry_layer)
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
