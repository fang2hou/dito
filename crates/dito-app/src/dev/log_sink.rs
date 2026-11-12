use dito_core::config::InsertMethod;
use dito_core::controller::{Delivery, Sink};

pub struct LogSink;

impl Sink for LogSink {
    fn deliver(&self, text: &str, _: InsertMethod) -> Delivery {
        tracing::info!("would insert: {text}");
        Delivery::Inserted
    }

    fn copy(&self, text: &str) -> Delivery {
        tracing::info!("would copy: {text}");
        Delivery::Copied
    }
}
