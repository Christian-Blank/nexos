use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[cfg(feature = "otel")]
use {
    opentelemetry::KeyValue,
    opentelemetry_sdk::{self, trace as sdktrace, Resource},
    opentelemetry_otlp::WithExportConfig,
    tracing_opentelemetry::OpenTelemetryLayer,
};

pub fn init_tracing(service_name: &str, version: &str, verbosity: u8) {
    let level = match verbosity { 0 => "info", 1 => "debug", _ => "trace" };
    let env_filter = EnvFilter::from_default_env().add_directive(level.parse().unwrap());

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false);

    #[cfg(feature = "otel")]
    let registry = if let Some(layer) = build_otel_layer(service_name, version) {
        tracing_subscriber::registry().with(env_filter).with(fmt_layer).with(layer)
    } else {
        tracing_subscriber::registry().with(env_filter).with(fmt_layer)
    };

    #[cfg(not(feature = "otel"))]
    let registry = tracing_subscriber::registry().with(env_filter).with(fmt_layer);

    registry.init();
}

#[cfg(feature = "otel")]
fn build_otel_layer<S>(service_name: &str, version: &str) -> Option<OpenTelemetryLayer<S>>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    // Only enable if endpoint env is present
    let _ = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok()?;

    // Build OTLP pipeline (env vars like protocol, headers, etc. are honored)
    let exporter = opentelemetry_otlp::new_exporter().tonic();
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(
            sdktrace::Config::default().with_resource(Resource::new(vec![
                KeyValue::new("service.name", service_name.to_string()),
                KeyValue::new("service.version", version.to_string()),
            ])),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .ok()?;

    Some(tracing_opentelemetry::layer().with_tracer(tracer))
}

#[cfg(feature = "otel")]
pub fn shutdown_tracing() {
    opentelemetry::global::shutdown_tracer_provider();
}

#[cfg(not(feature = "otel"))]
pub fn shutdown_tracing() {}
