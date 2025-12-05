use anyhow::Result;
use std::time::{Duration, Instant, SystemTime}; // Add SystemTime for UNIX_EPOCH
use serde::{Serialize, Deserialize};
use reqwest::{RequestBuilder, Response};
use uuid; // For request_id

use crate::reqwest_simd_json::{ReqwestSimdJsonExt, ResponseSimdJsonExt}; // Assuming this path is correct

/// Core telemetry data for request handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestTelemetry {
    pub request_id: String,
    #[serde(with = "crate::telemetry::serde_duration")] // Custom serializer for Duration
    pub start_time: Duration, // Changed to Duration from Instant
    #[serde(with = "crate::telemetry::serde_duration")]
    pub duration: Duration,
    pub success: bool,
    pub error_message: Option<String>,
    pub request_size_bytes: usize,
    pub response_size_bytes: usize,
}

/// Trait for adding telemetry capabilities to request handlers
pub trait RequestTelemetryExt {
    /// Track request execution with automatic timing and error capture
    async fn track_telemetry<F, T>(&self, operation: F) -> Result<(T, RequestTelemetry)>
    where
        F: FnOnce() -> Result<T> + Send, // Added Send bound
        T: Serialize + Send + 'static;
    
    /// Record request metrics for upload to Splitrail Cloud
    fn record_metrics(&self, telemetry: RequestTelemetry) -> Result<()>;
    
    /// Get telemetry configuration settings
    fn get_telemetry_config(&self) -> TelemetryConfig;
}

/// Trait for response telemetry and validation
pub trait ResponseTelemetryExt {
    /// Parse response with telemetry tracking
    async fn parse_with_telemetry<T>(self) -> Result<(T, ResponseTelemetry)>
    where
        T: serde::de::DeserializeOwned + Send + 'static;
    
    /// Validate response structure and collect metrics
    fn validate_response(&self, expected_size: Option<usize>) -> Result<ResponseValidation>;
}

/// Configuration for telemetry collection
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub upload_endpoint: String,
    pub batch_size: usize,
    pub retry_attempts: u32,
}

/// Response validation metrics
#[derive(Debug, Clone)]
pub struct ResponseValidation {
    pub is_valid: bool,
    pub size_matches: bool,
    pub parse_success: bool,
    pub validation_errors: Vec<String>,
}

/// Response telemetry data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTelemetry {
    #[serde(with = "crate::telemetry::serde_duration")] // Custom serializer for Duration
    pub parse_duration: Duration,
    pub parse_success: bool,
    pub response_size: usize,
    pub content_type: Option<String>,
    pub status_code: u16,
}

impl RequestTelemetryExt for RequestBuilder {
    async fn track_telemetry<F, T>(&self, operation: F) -> Result<(T, RequestTelemetry)>
    where
        F: FnOnce() -> Result<T> + Send,
        T: Serialize + Send + 'static,
    {
        let start_time = Instant::now();
        let request_id = uuid::Uuid::new_v4().to_string();
        
        let result = operation();
        let duration = start_time.elapsed();
        
        let telemetry = RequestTelemetry {
            request_id,
            start_time: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("Time went backwards"),
            duration,
            success: result.is_ok(),
            error_message: result.as_ref().err().map(|e| e.to_string()),
            request_size_bytes: 0, // Would be populated from actual request
            response_size_bytes: 0, // Would be populated from actual response
        };
        
        result.map(|data| (data, telemetry))
    }
    
    fn record_metrics(&self, telemetry: RequestTelemetry) -> Result<()> {
        // Integration with existing upload system
        // Similar to upload_message_stats in upload.rs
        println!("Recording telemetry: {:?}", telemetry);
        Ok(())
    }
    
    fn get_telemetry_config(&self) -> TelemetryConfig {
        TelemetryConfig {
            enabled: true,
            upload_endpoint: "https://api.splitrail.dev/telemetry".to_string(),
            batch_size: 100,
            retry_attempts: 3,
        }
    }
}

impl ResponseTelemetryExt for Response {
    async fn parse_with_telemetry<T>(self) -> Result<(T, ResponseTelemetry)>
    where
        T: serde::de::DeserializeOwned + Send + 'static,
    {
        let start_time = Instant::now();
        let size = self.content_length().unwrap_or(0) as usize;
        let content_type = self.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let status_code = self.status().as_u16(); // Get status before moving self

        let result = self.simd_json::<T>().await;
        let duration = start_time.elapsed();
        
        let telemetry = ResponseTelemetry {
            parse_duration: duration,
            parse_success: result.is_ok(),
            response_size: size,
            content_type,
            status_code,
        };
        
        result.map(|data| (data, telemetry))
    }
    
    fn validate_response(&self, expected_size: Option<usize>) -> Result<ResponseValidation> {
        let actual_size = self.content_length().unwrap_or(0) as usize;
        let size_matches = expected_size
            .map(|expected| actual_size == expected)
            .unwrap_or(true);
        
        let validation = ResponseValidation {
            is_valid: self.status().is_success() && size_matches,
            size_matches,
            parse_success: true, // Would be determined after parsing
            validation_errors: if !self.status().is_success() {
                vec![format!("HTTP error: {}", self.status())]
            } else if !size_matches {
                vec![format!("Size mismatch: expected {:?}, got {}", expected_size, actual_size)]
            } else {
                vec![]
            },
        };
        
        Ok(validation)
    }
}

// Custom serialization/deserialization for std::time::Duration
mod serde_duration {
    use serde::{Serializer, Deserializer, Deserialize, de};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}