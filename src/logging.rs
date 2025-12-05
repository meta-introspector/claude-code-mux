use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Arc, RwLock};
use tracing::{field::Field, field::Visit, Event, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

/// A structured log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// A visitor to extract the message from a log event's fields.
#[derive(Default)]
struct LogVisitor {
    message: Option<String>,
}

impl Visit for LogVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value));
        }
    }
}

/// A tracing layer that stores logs in a ring buffer and on disk.
#[derive(Debug)]
pub struct QueryableLogLayer {
    buffer: Arc<RwLock<VecDeque<LogEntry>>>,
    log_file: Arc<RwLock<File>>,
}

impl QueryableLogLayer {
    pub fn new(
        buffer: Arc<RwLock<VecDeque<LogEntry>>>,
        log_file_path: &str,
    ) -> anyhow::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(log_file_path)?;

        Ok(Self {
            buffer,
            log_file: Arc::new(RwLock::new(file)),
        })
    }
}

impl<S> Layer<S> for QueryableLogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = LogVisitor::default();
        event.record(&mut visitor);

        if let Some(message) = visitor.message {
            let log_entry = LogEntry {
                timestamp: Utc::now(),
                level: event.metadata().level().to_string(),
                target: event.metadata().target().to_string(),
                message,
            };

            // Write to in-memory ring buffer
            if let Ok(mut buffer) = self.buffer.write() {
                buffer.push_back(log_entry.clone());
                // Keep the buffer at a max size, e.g., 1000 entries
                if buffer.len() > 1000 {
                    buffer.pop_front();
                }
            }

            // Write to disk
            if let Ok(mut file) = self.log_file.write() {
                if let Ok(json) = serde_json::to_string(&log_entry) {
                    let _ = writeln!(file, "{}", json);
                }
            }
        }
    }
}