//! Shared test helpers for unit tests and integration tests in the adapters crate.
//!
//! Gated by `#[cfg(test)]`; only compiled when running tests.

#![allow(clippy::unwrap_used, clippy::disallowed_methods)]
// reason: test-only code; panics are the correct failure mode in tests

use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// MessageVisitor
// ---------------------------------------------------------------------------

/// Visits tracing event fields and extracts the `"message"` field value.
pub struct MessageVisitor<'a> {
    /// Receives the message string when found.
    pub message: &'a mut Option<String>,
}

impl tracing::field::Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            *self.message = Some(format!("{value:?}"));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            *self.message = Some(value.to_owned());
        }
    }
}

// ---------------------------------------------------------------------------
// EventCollector
// ---------------------------------------------------------------------------

/// A [`tracing_subscriber::Layer`] that collects the `"message"` field of
/// every tracing event into a shared `Vec<String>`.
pub struct EventCollector {
    /// Shared storage for collected event messages.
    pub events: Arc<Mutex<Vec<String>>>,
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for EventCollector {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut msg: Option<String> = None;
        event.record(&mut MessageVisitor { message: &mut msg });
        if let Some(m) = msg {
            self.events.lock().unwrap().push(m);
        }
    }
}
