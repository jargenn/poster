use std::fs::OpenOptions;

use tracing_bunyan_formatter::BunyanFormattingLayer;
use tracing_subscriber::{
    EnvFilter, Registry, layer::SubscriberExt as _, util::SubscriberInitExt as _,
};
use tracing_tree::HierarchicalLayer;

pub fn init_tracing() {
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .write(true)
        .open("logs.json")
        .expect("Failed to open log file");

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            EnvFilter::new("debug")
        } else {
            EnvFilter::new("info")
        }
    });
    let formatting_layer = BunyanFormattingLayer::new("tracing_demo".into(), log_file);

    let layer = HierarchicalLayer::new(2)
        .with_bracketed_fields(false)
        .with_indent_lines(true)
        .with_targets(false);

    Registry::default()
        .with(filter)
        .with(formatting_layer)
        .with(layer)
        .init();
}
