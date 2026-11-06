use dito_core::config::InsertMethod;
use dito_core::controller::{Delivery, Sink};

/// Logs the text instead of typing it into another app.
pub struct LogSink;

impl Sink for LogSink {
    fn deliver(&self, text: &str, _: InsertMethod) -> Delivery {
        tracing::info!("would insert: {text}");
        Delivery::Inserted
    }
}
